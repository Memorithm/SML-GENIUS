//! Inter-skill dependency graph (DAG), including cross-domain edges.
//!
//! A lesson may only be added once all its prerequisites exist, so the graph is
//! acyclic by construction. Extra edges added later (for example a discovered
//! transfer dependency) go through [`DependencyGraph::add_edge`], which rejects
//! any edge that would close a cycle.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::domain::Domain;
use crate::lesson::LessonId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphError {
    Duplicate(LessonId),
    MissingPrerequisite { lesson: LessonId, missing: LessonId },
    UnknownNode(LessonId),
    Cycle { from: LessonId, to: LessonId },
    SelfLoop(LessonId),
}

impl core::fmt::Display for GraphError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            GraphError::Duplicate(l) => write!(f, "lesson {} already in graph", l.short()),
            GraphError::MissingPrerequisite { lesson, missing } => write!(
                f,
                "lesson {} requires unknown prerequisite {}",
                lesson.short(),
                missing.short()
            ),
            GraphError::UnknownNode(l) => write!(f, "unknown lesson {}", l.short()),
            GraphError::Cycle { from, to } => write!(
                f,
                "edge {} -> {} would create a cycle",
                from.short(),
                to.short()
            ),
            GraphError::SelfLoop(l) => write!(f, "self dependency on {}", l.short()),
        }
    }
}

impl std::error::Error for GraphError {}

#[derive(Clone, Debug, Default)]
pub struct DependencyGraph {
    prereqs: BTreeMap<LessonId, BTreeSet<LessonId>>,
    dependents: BTreeMap<LessonId, BTreeSet<LessonId>>,
    domain: BTreeMap<LessonId, Domain>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.prereqs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.prereqs.is_empty()
    }

    pub fn contains(&self, id: &LessonId) -> bool {
        self.prereqs.contains_key(id)
    }

    pub fn add_node(
        &mut self,
        id: LessonId,
        domain: Domain,
        prerequisites: &[LessonId],
    ) -> Result<(), GraphError> {
        if self.prereqs.contains_key(&id) {
            return Err(GraphError::Duplicate(id));
        }
        for p in prerequisites {
            if *p == id {
                return Err(GraphError::SelfLoop(id));
            }
            if !self.prereqs.contains_key(p) {
                return Err(GraphError::MissingPrerequisite {
                    lesson: id,
                    missing: *p,
                });
            }
        }
        self.prereqs
            .insert(id, prerequisites.iter().copied().collect());
        self.dependents.entry(id).or_default();
        self.domain.insert(id, domain);
        for p in prerequisites {
            self.dependents.entry(*p).or_default().insert(id);
        }
        Ok(())
    }

    /// Adds `prerequisite -> lesson` after the fact, rejecting cycles.
    pub fn add_edge(&mut self, prerequisite: LessonId, lesson: LessonId) -> Result<(), GraphError> {
        if prerequisite == lesson {
            return Err(GraphError::SelfLoop(lesson));
        }
        for n in [prerequisite, lesson] {
            if !self.prereqs.contains_key(&n) {
                return Err(GraphError::UnknownNode(n));
            }
        }
        // A cycle appears iff `prerequisite` is already reachable from `lesson`
        // through dependents.
        if self.reachable(lesson, prerequisite) {
            return Err(GraphError::Cycle {
                from: prerequisite,
                to: lesson,
            });
        }
        self.prereqs.entry(lesson).or_default().insert(prerequisite);
        self.dependents
            .entry(prerequisite)
            .or_default()
            .insert(lesson);
        Ok(())
    }

    fn reachable(&self, from: LessonId, target: LessonId) -> bool {
        let mut queue = VecDeque::from([from]);
        let mut seen = BTreeSet::from([from]);
        while let Some(n) = queue.pop_front() {
            if n == target {
                return true;
            }
            if let Some(ds) = self.dependents.get(&n) {
                for d in ds {
                    if seen.insert(*d) {
                        queue.push_back(*d);
                    }
                }
            }
        }
        false
    }

    pub fn prerequisites(&self, id: &LessonId) -> impl Iterator<Item = &LessonId> {
        self.prereqs.get(id).into_iter().flatten()
    }

    pub fn dependents(&self, id: &LessonId) -> impl Iterator<Item = &LessonId> {
        self.dependents.get(id).into_iter().flatten()
    }

    /// All transitive dependents (lessons whose validity relies on `id`).
    pub fn transitive_dependents(&self, id: &LessonId) -> BTreeSet<LessonId> {
        let mut out = BTreeSet::new();
        let mut queue = VecDeque::from([*id]);
        while let Some(n) = queue.pop_front() {
            for d in self.dependents(&n) {
                if out.insert(*d) {
                    queue.push_back(*d);
                }
            }
        }
        out
    }

    /// Kahn topological order with deterministic tie-breaking by id.
    pub fn topological_order(&self) -> Vec<LessonId> {
        let mut indegree: BTreeMap<LessonId, usize> =
            self.prereqs.iter().map(|(k, v)| (*k, v.len())).collect();
        let mut ready: BTreeSet<LessonId> = indegree
            .iter()
            .filter(|(_, d)| **d == 0)
            .map(|(k, _)| *k)
            .collect();
        let mut out = Vec::with_capacity(indegree.len());
        while let Some(n) = ready.pop_first() {
            out.push(n);
            for d in self.dependents(&n) {
                if let Some(deg) = indegree.get_mut(d) {
                    *deg -= 1;
                    if *deg == 0 {
                        ready.insert(*d);
                    }
                }
            }
        }
        out
    }

    /// Number of edges whose endpoints belong to different domains.
    pub fn cross_domain_edges(&self) -> usize {
        self.prereqs
            .iter()
            .map(|(l, ps)| {
                ps.iter()
                    .filter(|p| self.domain.get(p) != self.domain.get(l))
                    .count()
            })
            .sum()
    }

    pub fn domain_of(&self, id: &LessonId) -> Option<Domain> {
        self.domain.get(id).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::Digest;

    fn id(i: u8) -> LessonId {
        Digest::of(&[i])
    }

    #[test]
    fn requires_known_prerequisites_and_orders_topologically() {
        let mut g = DependencyGraph::new();
        assert!(matches!(
            g.add_node(id(2), Domain::Rust, &[id(1)]),
            Err(GraphError::MissingPrerequisite { .. })
        ));
        g.add_node(id(1), Domain::Mathematics, &[]).unwrap();
        g.add_node(id(2), Domain::Rust, &[id(1)]).unwrap();
        g.add_node(id(3), Domain::Rust, &[id(2), id(1)]).unwrap();
        let order = g.topological_order();
        let pos = |x| order.iter().position(|y| *y == x).unwrap();
        assert!(pos(id(1)) < pos(id(2)) && pos(id(2)) < pos(id(3)));
        assert_eq!(g.cross_domain_edges(), 2);
        assert_eq!(
            g.transitive_dependents(&id(1)),
            BTreeSet::from([id(2), id(3)])
        );
        assert_eq!(
            g.add_node(id(1), Domain::Mathematics, &[]),
            Err(GraphError::Duplicate(id(1)))
        );
    }

    #[test]
    fn late_edges_cannot_create_cycles() {
        let mut g = DependencyGraph::new();
        g.add_node(id(1), Domain::French, &[]).unwrap();
        g.add_node(id(2), Domain::English, &[id(1)]).unwrap();
        g.add_node(id(3), Domain::English, &[id(2)]).unwrap();
        assert!(matches!(
            g.add_edge(id(3), id(1)),
            Err(GraphError::Cycle { .. })
        ));
        assert_eq!(g.add_edge(id(1), id(1)), Err(GraphError::SelfLoop(id(1))));
        g.add_edge(id(1), id(3)).unwrap();
        assert_eq!(g.topological_order().len(), 3);
    }
}
