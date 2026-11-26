//! Lesson registry: idempotent registration, deduplication and audited state
//! transitions.

use std::collections::BTreeMap;

use crate::domain::{Domain, KnowledgeState};
use crate::graph::{DependencyGraph, GraphError};
use crate::lesson::{ErrorRecord, ErrorStage, Lesson, LessonId, LessonSpec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Registration {
    Inserted,
    /// Same content already registered: no-op (idempotent).
    Duplicate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegistryError {
    Graph(GraphError),
    UnknownLesson(LessonId),
    IllegalTransition {
        lesson: LessonId,
        from: KnowledgeState,
        to: KnowledgeState,
    },
    PrerequisitesNotMet(LessonId),
}

impl core::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RegistryError::Graph(e) => write!(f, "{e}"),
            RegistryError::UnknownLesson(l) => write!(f, "unknown lesson {}", l.short()),
            RegistryError::IllegalTransition { lesson, from, to } => write!(
                f,
                "illegal transition {} -> {} for {}",
                from.name(),
                to.name(),
                lesson.short()
            ),
            RegistryError::PrerequisitesNotMet(l) => {
                write!(f, "prerequisites of {} not certified", l.short())
            }
        }
    }
}

impl std::error::Error for RegistryError {}

impl From<GraphError> for RegistryError {
    fn from(e: GraphError) -> Self {
        RegistryError::Graph(e)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransitionEvent {
    pub lesson: LessonId,
    pub from: KnowledgeState,
    pub to: KnowledgeState,
}

#[derive(Default)]
pub struct LessonRegistry {
    lessons: BTreeMap<LessonId, Lesson>,
    graph: DependencyGraph,
    events: Vec<TransitionEvent>,
}

impl LessonRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.lessons.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lessons.is_empty()
    }

    pub fn graph(&self) -> &DependencyGraph {
        &self.graph
    }

    pub fn get(&self, id: &LessonId) -> Option<&Lesson> {
        self.lessons.get(id)
    }

    pub fn get_mut(&mut self, id: &LessonId) -> Option<&mut Lesson> {
        self.lessons.get_mut(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Lesson> {
        self.lessons.values()
    }

    pub fn events(&self) -> &[TransitionEvent] {
        &self.events
    }

    /// Registers a lesson and moves it to `Identified`. Re-registering the same
    /// spec is a no-op.
    pub fn register(
        &mut self,
        spec: LessonSpec,
    ) -> Result<(LessonId, Registration), RegistryError> {
        let id = spec.id();
        if self.lessons.contains_key(&id) {
            return Ok((id, Registration::Duplicate));
        }
        self.graph.add_node(id, spec.domain, &spec.prerequisites)?;
        let mut lesson = Lesson::new(spec);
        lesson.state = KnowledgeState::Identified;
        self.events.push(TransitionEvent {
            lesson: id,
            from: KnowledgeState::Unknown,
            to: KnowledgeState::Identified,
        });
        self.lessons.insert(id, lesson);
        Ok((id, Registration::Inserted))
    }

    pub fn state(&self, id: &LessonId) -> Option<KnowledgeState> {
        self.lessons.get(id).map(|l| l.state)
    }

    pub fn prerequisites_met(&self, id: &LessonId) -> bool {
        self.graph.prerequisites(id).all(|p| {
            self.lessons
                .get(p)
                .is_some_and(|l| l.state.satisfies_prerequisite())
        })
    }

    pub fn transition(
        &mut self,
        id: &LessonId,
        to: KnowledgeState,
    ) -> Result<KnowledgeState, RegistryError> {
        let from = self
            .lessons
            .get(id)
            .map(|l| l.state)
            .ok_or(RegistryError::UnknownLesson(*id))?;
        if !from.can_transition(to) {
            return Err(RegistryError::IllegalTransition {
                lesson: *id,
                from,
                to,
            });
        }
        if to == KnowledgeState::Acquiring && !self.prerequisites_met(id) {
            return Err(RegistryError::PrerequisitesNotMet(*id));
        }
        if let Some(l) = self.lessons.get_mut(id) {
            l.state = to;
        }
        self.events.push(TransitionEvent {
            lesson: *id,
            from,
            to,
        });
        Ok(from)
    }

    /// Forces a replayed state during resume (journal replay already validated
    /// every transition).
    pub fn restore_state(
        &mut self,
        id: &LessonId,
        state: KnowledgeState,
    ) -> Result<(), RegistryError> {
        let l = self
            .lessons
            .get_mut(id)
            .ok_or(RegistryError::UnknownLesson(*id))?;
        l.state = state;
        Ok(())
    }

    pub fn record_error(
        &mut self,
        id: &LessonId,
        stage: ErrorStage,
        detail: String,
    ) -> Result<(), RegistryError> {
        let l = self
            .lessons
            .get_mut(id)
            .ok_or(RegistryError::UnknownLesson(*id))?;
        l.errors.push(ErrorRecord {
            attempt: l.attempts,
            stage,
            detail,
        });
        Ok(())
    }

    /// Lessons ready for acquisition: `Identified` or `NeedsRevision`, with all
    /// prerequisites certified or consolidated.
    pub fn ready(&self) -> Vec<LessonId> {
        self.lessons
            .values()
            .filter(|l| {
                matches!(
                    l.state,
                    KnowledgeState::Identified | KnowledgeState::NeedsRevision
                ) && self.prerequisites_met(&l.id)
            })
            .map(|l| l.id)
            .collect()
    }

    /// Marks every consolidated dependent of `id` for revision (used when a
    /// prerequisite turns out contradictory or is revised).
    pub fn invalidate_dependents(&mut self, id: &LessonId) -> Vec<LessonId> {
        let deps = self.graph.transitive_dependents(id);
        let mut changed = Vec::new();
        for d in deps {
            if self.state(&d) == Some(KnowledgeState::Consolidated)
                && self.transition(&d, KnowledgeState::NeedsRevision).is_ok()
            {
                changed.push(d);
            }
        }
        changed
    }

    pub fn count_by_state(&self, domain: Domain) -> BTreeMap<KnowledgeState, usize> {
        let mut out = BTreeMap::new();
        for l in self.lessons.values().filter(|l| l.spec.domain == domain) {
            *out.entry(l.state).or_insert(0) += 1;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lesson::test_spec;
    use KnowledgeState::*;

    #[test]
    fn idempotent_registration_and_gated_acquisition() {
        let mut r = LessonRegistry::new();
        let (a, ins) = r
            .register(test_spec(Domain::Mathematics, "a", vec![]))
            .unwrap();
        assert_eq!(ins, Registration::Inserted);
        let (a2, dup) = r
            .register(test_spec(Domain::Mathematics, "a", vec![]))
            .unwrap();
        assert_eq!((a2, dup), (a, Registration::Duplicate));
        let (b, _) = r.register(test_spec(Domain::Rust, "b", vec![a])).unwrap();
        assert_eq!(r.ready(), vec![a]);
        assert_eq!(
            r.transition(&b, Acquiring),
            Err(RegistryError::PrerequisitesNotMet(b))
        );
        for s in [Acquiring, Learned, Validating, Certified] {
            r.transition(&a, s).unwrap();
        }
        assert_eq!(r.ready(), vec![b]);
        assert!(matches!(
            r.transition(&b, Certified),
            Err(RegistryError::IllegalTransition { .. })
        ));
    }

    #[test]
    fn invalidation_marks_consolidated_dependents_for_revision() {
        let mut r = LessonRegistry::new();
        let (a, _) = r.register(test_spec(Domain::French, "a", vec![])).unwrap();
        let (b, _) = r
            .register(test_spec(Domain::English, "b", vec![a]))
            .unwrap();
        for s in [Acquiring, Learned, Validating, Certified, Consolidated] {
            r.transition(&a, s).unwrap();
        }
        for s in [Acquiring, Learned, Validating, Certified, Consolidated] {
            r.transition(&b, s).unwrap();
        }
        assert_eq!(r.invalidate_dependents(&a), vec![b]);
        assert_eq!(r.state(&b), Some(NeedsRevision));
    }
}
