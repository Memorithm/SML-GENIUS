//! Transactional routed wide-sparse ANF experts; existing large/PVP6 paths remain independent.
use crate::{
    pvp6_sbg_routing::{RoutedSemantic, RoutedSemanticError},
    semantic::{SemanticRewriteStep, SemanticRewriter, SemanticState},
    wide_anf_selected_pages::{WideAnfPageAtlas, WidePageError, WidePageTrace},
};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WideRoutedCycle {
    pub depth: u32,
    pub pages: Vec<WidePageTrace>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WideRoutedError<E> {
    Depth,
    Budget,
    Routing(RoutedSemanticError<E>),
    Page(WidePageError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WideRoutedExperts<K, const S: usize> {
    routed: RoutedSemantic<K, S>,
    atlas: WideAnfPageAtlas<S>,
    max_total_gates: usize,
    used_gates: usize,
    max_trace_words: usize,
    used_trace_words: usize,
    cycles: Arc<Vec<WideRoutedCycle>>,
}

impl<K: SemanticRewriter<S> + Clone, const S: usize> WideRoutedExperts<K, S> {
    pub fn new(
        routed: RoutedSemantic<K, S>,
        atlas: WideAnfPageAtlas<S>,
        max_total_gates: usize,
    ) -> Result<Self, WideRoutedError<K::Error>> {
        if !routed.cycles().is_empty() || max_total_gates == 0 {
            return Err(WideRoutedError::Budget);
        }
        Ok(Self {
            routed,
            atlas,
            max_total_gates,
            used_gates: 0,
            max_trace_words: usize::MAX,
            used_trace_words: 0,
            cycles: Arc::new(Vec::new()),
        })
    }
    pub fn routed(&self) -> &RoutedSemantic<K, S> {
        &self.routed
    }
    pub fn cycles(&self) -> &[WideRoutedCycle] {
        &self.cycles
    }
    pub fn used_gates(&self) -> usize {
        self.used_gates
    }
    pub fn used_trace_words(&self) -> usize {
        self.used_trace_words
    }
    /// Set a cumulative retained packed-output budget before the first cycle.
    pub fn with_trace_word_budget(
        mut self,
        max_words: usize,
    ) -> Result<Self, WideRoutedError<K::Error>> {
        if !self.cycles.is_empty() || max_words == 0 {
            return Err(WideRoutedError::Budget);
        }
        self.max_trace_words = max_words;
        Ok(self)
    }
}

impl<K: SemanticRewriter<S> + Clone, const S: usize> SemanticRewriter<S>
    for WideRoutedExperts<K, S>
{
    type Error = WideRoutedError<K::Error>;

    fn rewrite(
        &mut self,
        state: &SemanticState<S>,
        depth: u32,
    ) -> Result<SemanticRewriteStep<S>, Self::Error> {
        if usize::try_from(depth).ok() != self.cycles.len().checked_add(1) {
            return Err(WideRoutedError::Depth);
        }
        let mut candidate = self.routed.clone();
        let inner = candidate
            .rewrite(state, depth)
            .map_err(WideRoutedError::Routing)?;
        let decision = &candidate
            .cycles()
            .last()
            .ok_or(WideRoutedError::Depth)?
            .route
            .decision;
        let remaining = self
            .max_total_gates
            .checked_sub(self.used_gates)
            .ok_or(WideRoutedError::Budget)?;
        let remaining_trace = self
            .max_trace_words
            .checked_sub(self.used_trace_words)
            .ok_or(WideRoutedError::Budget)?;
        let batch = self
            .atlas
            .execute(&inner.state, decision, remaining, remaining_trace)
            .map_err(WideRoutedError::Page)?;
        let count = batch
            .traces
            .iter()
            .try_fold(0usize, |sum, trace| sum.checked_add(trace.packed.len()))
            .ok_or(WideRoutedError::Budget)?;
        let used_trace_words = self
            .used_trace_words
            .checked_add(count)
            .ok_or(WideRoutedError::Budget)?;
        if used_trace_words > self.max_trace_words {
            return Err(WideRoutedError::Budget);
        }
        let selected_gate_count = decision
            .selected_experts
            .iter()
            .try_fold(0usize, |sum, id| {
                self.atlas
                    .gate_count(*id)
                    .ok()
                    .and_then(|gates| sum.checked_add(gates))
            })
            .ok_or(WideRoutedError::Budget)?;
        let used = self
            .used_gates
            .checked_add(selected_gate_count)
            .ok_or(WideRoutedError::Budget)?;
        if used > self.max_total_gates {
            return Err(WideRoutedError::Budget);
        }
        let mut cycles = self.cycles.clone();
        Arc::make_mut(&mut cycles)
            .try_reserve(1)
            .map_err(|_| WideRoutedError::Budget)?;
        Arc::make_mut(&mut cycles).push(WideRoutedCycle {
            depth,
            pages: batch.traces,
        });
        self.routed = candidate;
        self.cycles = cycles;
        self.used_gates = used;
        self.used_trace_words = used_trace_words;
        Ok(SemanticRewriteStep {
            state: batch.state,
            stability_blocked: inner.stability_blocked,
        })
    }
}
