//! Cycloid-1: bounded, cost-aware access paths to a MOR-selected SML page.
//!
//! A longer control path can have a lower estimated access latency when it
//! avoids an expensive direct read. Only relay/control nodes may be traversed:
//! intermediate SML expert pages are never executed. The destination remains
//! the exact page already selected by MOR; this module cannot change semantics.
//!
//! This is an offline deterministic reference planner, NOT a runtime router,
//! a trained metric, a physical speed result, or a Riemannian geodesic solver.
//! Edge costs must be supplied from a separately qualified, frozen profile.
//! Search/planning time and the cost of collecting that profile are NOT included.
//! Only a caller-declared online dispatch overhead is included for indirect
//! paths. No candidate may become the default runtime path without physical
//! end-to-end qualification and a versioned execution adapter.

use core::fmt;
use std::collections::BTreeSet;

use crate::{mor::MorDecision, PageId};

pub const MAX_CYCLOID_NODES: usize = 64;
pub const MAX_CYCLOID_EDGES: usize = 256;
pub const MAX_CYCLOID_HOPS: u8 = 8;
pub const MAX_CYCLOID_EXPANSIONS: u32 = 20_000;

/// Node identity is a bounded control-plane index, not a semantic embedding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CycloidNodeId(pub u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycloidNode {
    /// The already-computed MOR decision enters the access fabric here.
    Entry,
    /// Routing/address translation only. A relay never executes an expert.
    Relay,
    /// The one authoritative destination page to be accessed.
    Page(PageId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CycloidEdge {
    pub from: CycloidNodeId,
    pub to: CycloidNodeId,
    /// Estimated cost of this transition, supplied by an external profile.
    pub estimated_ns: u64,
    /// Estimated bytes read, supplied by the same frozen cost profile.
    pub estimated_read_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CycloidBudget {
    pub max_hops: u8,
    pub max_expansions: u32,
    pub max_read_bytes: u64,
}

impl CycloidBudget {
    fn validate(self) -> Result<Self, CycloidError> {
        if self.max_hops == 0
            || self.max_hops > MAX_CYCLOID_HOPS
            || self.max_expansions == 0
            || self.max_expansions > MAX_CYCLOID_EXPANSIONS
        {
            return Err(CycloidError::InvalidBudget);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CycloidPlan {
    pub page: PageId,
    /// Inclusive: entry node, zero or more relays, final page node.
    pub nodes: Vec<CycloidNodeId>,
    /// Direct baseline from the same frozen cost profile.
    pub direct_estimated_ns: u64,
    pub direct_estimated_read_bytes: u64,
    /// Estimated transition cost plus online dispatch overhead if indirect.
    pub selected_estimated_ns: u64,
    pub selected_estimated_read_bytes: u64,
    /// Zero for the baseline; caller-supplied only for an indirect route.
    pub online_dispatch_ns: u64,
    pub indirect_selected: bool,
    /// Planning work, not an inference-time or physical hardware counter.
    pub examined_edges: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycloidError {
    InvalidGraph,
    InvalidBudget,
    InvalidEdge,
    DuplicateEdge,
    DuplicatePage,
    PageNotSelected,
    MissingDestination,
    MissingDirectBaseline,
    DirectExceedsBudget,
    SearchBudgetExceeded,
    ArithmeticOverflow,
}

impl fmt::Display for CycloidError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for CycloidError {}

/// Immutable, bounded access graph. Root must be node 0; other nodes may be
/// relays or unique terminal SML pages. Pages have no outgoing edges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CycloidGraph {
    nodes: Vec<CycloidNode>,
    edges: Vec<CycloidEdge>,
}

impl CycloidGraph {
    pub fn new(nodes: Vec<CycloidNode>, mut edges: Vec<CycloidEdge>) -> Result<Self, CycloidError> {
        if !(2..=MAX_CYCLOID_NODES).contains(&nodes.len())
            || edges.is_empty()
            || edges.len() > MAX_CYCLOID_EDGES
            || nodes[0] != CycloidNode::Entry
            || nodes[1..].iter().any(|node| *node == CycloidNode::Entry)
        {
            return Err(CycloidError::InvalidGraph);
        }

        let mut seen_pages = BTreeSet::new();
        for node in &nodes {
            if let CycloidNode::Page(page) = node {
                if !seen_pages.insert(page.0) {
                    return Err(CycloidError::DuplicatePage);
                }
            }
        }
        if seen_pages.is_empty() {
            return Err(CycloidError::InvalidGraph);
        }

        for edge in &edges {
            let source = usize::from(edge.from.0);
            let dest = usize::from(edge.to.0);
            if source >= nodes.len()
                || dest >= nodes.len()
                || source == dest
                || dest == 0
                || matches!(nodes[source], CycloidNode::Page(_))
                || edge.estimated_ns == 0
            {
                return Err(CycloidError::InvalidEdge);
            }
        }
        edges.sort_unstable_by_key(|edge| (edge.from, edge.to));
        if edges
            .windows(2)
            .any(|pair| pair[0].from == pair[1].from && pair[0].to == pair[1].to)
        {
            return Err(CycloidError::DuplicateEdge);
        }
        Ok(Self { nodes, edges })
    }

    /// Plan only for an already selected MOR expert, to the same unique page.
    /// The exact direct edge must exist and satisfy the read budget. Otherwise
    /// this research path fails closed rather than inventing a baseline.
    pub fn plan_for_mor(
        &self,
        decision: &MorDecision,
        target: PageId,
        budget: CycloidBudget,
        online_dispatch_ns: u64,
    ) -> Result<CycloidPlan, CycloidError> {
        let budget = budget.validate()?;
        if !decision.selected_experts.contains(&target) {
            return Err(CycloidError::PageNotSelected);
        }
        let goal = self
            .nodes
            .iter()
            .position(|node| *node == CycloidNode::Page(target))
            .ok_or(CycloidError::MissingDestination)?;
        let goal = CycloidNodeId(u16::try_from(goal).map_err(|_| CycloidError::InvalidGraph)?);
        let entry = CycloidNodeId(0);
        let direct = self
            .edges
            .iter()
            .find(|edge| edge.from == entry && edge.to == goal)
            .ok_or(CycloidError::MissingDirectBaseline)?;
        if direct.estimated_read_bytes > budget.max_read_bytes {
            return Err(CycloidError::DirectExceedsBudget);
        }

        let mut search = CycloidSearch {
            graph: self,
            goal,
            baseline_ns: direct.estimated_ns,
            budget,
            online_dispatch_ns,
            examined: 0,
            best: None,
        };
        let mut path = vec![entry];
        search.walk(entry, 0, 0, &mut path)?;

        match search.best {
            Some((total_ns, bytes, nodes)) => Ok(CycloidPlan {
                page: target,
                nodes,
                direct_estimated_ns: direct.estimated_ns,
                direct_estimated_read_bytes: direct.estimated_read_bytes,
                selected_estimated_ns: total_ns,
                selected_estimated_read_bytes: bytes,
                online_dispatch_ns,
                indirect_selected: true,
                examined_edges: search.examined,
            }),
            None => Ok(CycloidPlan {
                page: target,
                nodes: vec![entry, goal],
                direct_estimated_ns: direct.estimated_ns,
                direct_estimated_read_bytes: direct.estimated_read_bytes,
                selected_estimated_ns: direct.estimated_ns,
                selected_estimated_read_bytes: direct.estimated_read_bytes,
                online_dispatch_ns: 0,
                indirect_selected: false,
                examined_edges: search.examined,
            }),
        }
    }
}

struct CycloidSearch<'a> {
    graph: &'a CycloidGraph,
    goal: CycloidNodeId,
    baseline_ns: u64,
    budget: CycloidBudget,
    online_dispatch_ns: u64,
    examined: u32,
    // Total time, bytes, node sequence. Deterministic tie-breaking.
    best: Option<(u64, u64, Vec<CycloidNodeId>)>,
}

impl CycloidSearch<'_> {
    fn walk(
        &mut self,
        current: CycloidNodeId,
        elapsed: u64,
        bytes: u64,
        path: &mut Vec<CycloidNodeId>,
    ) -> Result<(), CycloidError> {
        if path.len() - 1 >= usize::from(self.budget.max_hops) {
            return Ok(());
        }
        // Index iteration avoids holding an immutable edge-slice borrow during
        // recursive mutation. The graph is strictly bounded at construction.
        for index in 0..self.graph.edges.len() {
            let edge = self.graph.edges[index];
            if edge.from != current {
                continue;
            }
            self.examined = self
                .examined
                .checked_add(1)
                .ok_or(CycloidError::ArithmeticOverflow)?;
            if self.examined > self.budget.max_expansions {
                // Fail closed: a truncated search is not an optimality proof.
                return Err(CycloidError::SearchBudgetExceeded);
            }
            if path.contains(&edge.to) {
                continue;
            }
            let next_bytes = bytes
                .checked_add(edge.estimated_read_bytes)
                .ok_or(CycloidError::ArithmeticOverflow)?;
            if next_bytes > self.budget.max_read_bytes {
                continue;
            }
            let next_ns = elapsed
                .checked_add(edge.estimated_ns)
                .ok_or(CycloidError::ArithmeticOverflow)?;
            if next_ns >= self.baseline_ns {
                continue;
            }
            path.push(edge.to);
            if edge.to == self.goal {
                // A direct edge must use the baseline path. Only >=2-hop
                // alternatives pay the estimated online dispatch overhead.
                if path.len() > 2 {
                    let candidate_ns = next_ns
                        .checked_add(self.online_dispatch_ns)
                        .ok_or(CycloidError::ArithmeticOverflow)?;
                    if candidate_ns < self.baseline_ns {
                        let better = match &self.best {
                            Some((best_ns, best_bytes, best_path)) => {
                                (candidate_ns, next_bytes, path.as_slice())
                                    < (*best_ns, *best_bytes, best_path.as_slice())
                            }
                            None => true,
                        };
                        if better {
                            self.best = Some((candidate_ns, next_bytes, path.clone()));
                        }
                    }
                }
            } else {
                self.walk(edge.to, next_ns, next_bytes, path)?;
            }
            path.pop();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decision(pages: &[u32]) -> MorDecision {
        MorDecision {
            active_routers: vec![],
            ranked_experts: vec![],
            selected_experts: pages.iter().copied().map(PageId).collect(),
            noise_ppm: 0,
            structural_disagreement_ppm: 0,
            expert_mass_threshold_ppm: 0,
            request_precision_escalation: false,
            hysteresis_retained_previous_primary: false,
        }
    }

    fn budget() -> CycloidBudget {
        CycloidBudget {
            max_hops: 4,
            max_expansions: 100,
            max_read_bytes: 1_000,
        }
    }

    fn edge(from: u16, to: u16, ns: u64, bytes: u64) -> CycloidEdge {
        CycloidEdge {
            from: CycloidNodeId(from),
            to: CycloidNodeId(to),
            estimated_ns: ns,
            estimated_read_bytes: bytes,
        }
    }

    #[test]
    fn longer_path_wins_when_total_estimated_latency_is_lower() {
        let graph = CycloidGraph::new(
            vec![
                CycloidNode::Entry,
                CycloidNode::Relay,
                CycloidNode::Page(PageId(7)),
            ],
            vec![edge(0, 2, 120, 900), edge(0, 1, 10, 10), edge(1, 2, 20, 20)],
        )
        .unwrap();
        let plan = graph
            .plan_for_mor(&decision(&[7]), PageId(7), budget(), 5)
            .unwrap();
        assert_eq!(
            plan.nodes,
            vec![CycloidNodeId(0), CycloidNodeId(1), CycloidNodeId(2)]
        );
        assert_eq!(plan.selected_estimated_ns, 35);
        assert_eq!(plan.direct_estimated_ns, 120);
        assert_eq!(plan.selected_estimated_read_bytes, 30);
        assert!(plan.indirect_selected);
    }

    #[test]
    fn direct_wins_ties_and_expensive_dispatch() {
        let graph = CycloidGraph::new(
            vec![
                CycloidNode::Entry,
                CycloidNode::Relay,
                CycloidNode::Page(PageId(7)),
            ],
            vec![edge(0, 2, 40, 50), edge(0, 1, 10, 10), edge(1, 2, 20, 10)],
        )
        .unwrap();
        let plan = graph
            .plan_for_mor(&decision(&[7]), PageId(7), budget(), 10)
            .unwrap();
        assert!(!plan.indirect_selected);
        assert_eq!(plan.selected_estimated_ns, 40);
        assert_eq!(plan.online_dispatch_ns, 0);
    }

    #[test]
    fn budget_excludes_cheaper_but_more_expensive_memory_path() {
        let graph = CycloidGraph::new(
            vec![
                CycloidNode::Entry,
                CycloidNode::Relay,
                CycloidNode::Page(PageId(7)),
            ],
            vec![edge(0, 2, 60, 40), edge(0, 1, 5, 200), edge(1, 2, 5, 200)],
        )
        .unwrap();
        let mut limits = budget();
        limits.max_read_bytes = 100;
        let plan = graph
            .plan_for_mor(&decision(&[7]), PageId(7), limits, 0)
            .unwrap();
        assert!(!plan.indirect_selected);
        assert_eq!(plan.selected_estimated_read_bytes, 40);
    }

    #[test]
    fn cannot_reroute_to_a_nonselected_expert() {
        let graph = CycloidGraph::new(
            vec![CycloidNode::Entry, CycloidNode::Page(PageId(7))],
            vec![edge(0, 1, 20, 1)],
        )
        .unwrap();
        assert_eq!(
            graph.plan_for_mor(&decision(&[8]), PageId(7), budget(), 0),
            Err(CycloidError::PageNotSelected)
        );
    }

    #[test]
    fn malformed_graph_and_missing_baseline_fail_closed() {
        assert_eq!(
            CycloidGraph::new(
                vec![CycloidNode::Entry, CycloidNode::Page(PageId(7))],
                vec![edge(1, 0, 1, 1)],
            ),
            Err(CycloidError::InvalidEdge)
        );
        assert_eq!(
            CycloidGraph::new(
                vec![
                    CycloidNode::Entry,
                    CycloidNode::Page(PageId(7)),
                    CycloidNode::Page(PageId(7)),
                ],
                vec![edge(0, 1, 1, 1)],
            ),
            Err(CycloidError::DuplicatePage)
        );
        let graph = CycloidGraph::new(
            vec![
                CycloidNode::Entry,
                CycloidNode::Relay,
                CycloidNode::Page(PageId(7)),
            ],
            vec![edge(0, 1, 1, 1), edge(1, 2, 1, 1)],
        )
        .unwrap();
        assert_eq!(
            graph.plan_for_mor(&decision(&[7]), PageId(7), budget(), 0),
            Err(CycloidError::MissingDirectBaseline)
        );
    }

    #[test]
    fn search_limit_is_an_error_not_a_partial_success() {
        let graph = CycloidGraph::new(
            vec![
                CycloidNode::Entry,
                CycloidNode::Relay,
                CycloidNode::Page(PageId(7)),
            ],
            vec![edge(0, 1, 1, 1), edge(0, 2, 100, 10), edge(1, 2, 1, 1)],
        )
        .unwrap();
        let mut limits = budget();
        limits.max_expansions = 1;
        assert_eq!(
            graph.plan_for_mor(&decision(&[7]), PageId(7), limits, 0),
            Err(CycloidError::SearchBudgetExceeded)
        );
    }

    #[test]
    fn tie_breaking_is_deterministic_and_avoids_cycles() {
        let graph = CycloidGraph::new(
            vec![
                CycloidNode::Entry,
                CycloidNode::Relay,
                CycloidNode::Relay,
                CycloidNode::Page(PageId(7)),
            ],
            vec![
                edge(2, 3, 10, 5),
                edge(1, 2, 5, 5),
                edge(0, 3, 100, 100),
                edge(0, 2, 10, 5),
                edge(1, 3, 10, 5),
                edge(0, 1, 10, 5),
                edge(2, 1, 5, 5),
            ],
        )
        .unwrap();
        let first = graph
            .plan_for_mor(&decision(&[7]), PageId(7), budget(), 0)
            .unwrap();
        let second = graph
            .plan_for_mor(&decision(&[7]), PageId(7), budget(), 0)
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.selected_estimated_ns, 20);
        assert_eq!(
            first.nodes,
            vec![CycloidNodeId(0), CycloidNodeId(1), CycloidNodeId(3)]
        );
    }
}
