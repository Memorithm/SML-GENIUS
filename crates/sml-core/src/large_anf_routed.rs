//! Transactional routed large-ANF experts; PVP6 remains an independent control.
use crate::{
    large_anf_selected_pages::{LargeAnfPageAtlas, LargePageError, LargePageTrace},
    pvp6_sbg_routing::{RoutedSemantic, RoutedSemanticError},
    semantic::{SemanticRewriteStep, SemanticRewriter, SemanticState},
};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargeRoutedCycle {
    pub depth: u32,
    pub pages: Vec<LargePageTrace>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LargeRoutedError<E> {
    Depth,
    Budget,
    Routing(RoutedSemanticError<E>),
    Page(LargePageError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargeRoutedExperts<K, const S: usize> {
    routed: RoutedSemantic<K, S>,
    atlas: LargeAnfPageAtlas<S>,
    max_total_gates: usize,
    used_gates: usize,
    cycles: Arc<Vec<LargeRoutedCycle>>,
}

impl<K: SemanticRewriter<S> + Clone, const S: usize> LargeRoutedExperts<K, S> {
    pub fn new(
        routed: RoutedSemantic<K, S>,
        atlas: LargeAnfPageAtlas<S>,
        max_total_gates: usize,
    ) -> Result<Self, LargeRoutedError<K::Error>> {
        if !routed.cycles().is_empty() || max_total_gates == 0 {
            return Err(LargeRoutedError::Budget);
        }
        Ok(Self {
            routed,
            atlas,
            max_total_gates,
            used_gates: 0,
            cycles: Arc::new(Vec::new()),
        })
    }
    pub fn routed(&self) -> &RoutedSemantic<K, S> {
        &self.routed
    }
    pub fn cycles(&self) -> &[LargeRoutedCycle] {
        &self.cycles
    }
    pub fn used_gates(&self) -> usize {
        self.used_gates
    }
}

impl<K: SemanticRewriter<S> + Clone, const S: usize> SemanticRewriter<S>
    for LargeRoutedExperts<K, S>
{
    type Error = LargeRoutedError<K::Error>;

    fn rewrite(
        &mut self,
        state: &SemanticState<S>,
        depth: u32,
    ) -> Result<SemanticRewriteStep<S>, Self::Error> {
        if usize::try_from(depth).ok() != self.cycles.len().checked_add(1) {
            return Err(LargeRoutedError::Depth);
        }
        let mut candidate = self.routed.clone();
        let inner = candidate
            .rewrite(state, depth)
            .map_err(LargeRoutedError::Routing)?;
        let decision = &candidate
            .cycles()
            .last()
            .ok_or(LargeRoutedError::Depth)?
            .route
            .decision;
        let remaining = self
            .max_total_gates
            .checked_sub(self.used_gates)
            .ok_or(LargeRoutedError::Budget)?;
        let batch = self
            .atlas
            .execute(&inner.state, decision, remaining)
            .map_err(LargeRoutedError::Page)?;
        let count = batch
            .traces
            .iter()
            .try_fold(0usize, |sum, trace| sum.checked_add(trace.packed.len()))
            .ok_or(LargeRoutedError::Budget)?;
        // The atlas enforces actual gate evaluations. Packed words are an
        // additional bounded accounting signal, not a substitute for gates.
        let _ = count;
        let selected_gate_count = decision
            .selected_experts
            .iter()
            .try_fold(0usize, |sum, id| {
                self.atlas
                    .gate_count(*id)
                    .ok()
                    .and_then(|gates| sum.checked_add(gates))
            })
            .ok_or(LargeRoutedError::Budget)?;
        let used = self
            .used_gates
            .checked_add(selected_gate_count)
            .ok_or(LargeRoutedError::Budget)?;
        if used > self.max_total_gates {
            return Err(LargeRoutedError::Budget);
        }
        let mut cycles = self.cycles.clone();
        Arc::make_mut(&mut cycles)
            .try_reserve(1)
            .map_err(|_| LargeRoutedError::Budget)?;
        Arc::make_mut(&mut cycles).push(LargeRoutedCycle {
            depth,
            pages: batch.traces,
        });
        self.routed = candidate;
        self.cycles = cycles;
        self.used_gates = used;
        Ok(SemanticRewriteStep {
            state: batch.state,
            stability_blocked: inner.stability_blocked,
        })
    }
}
