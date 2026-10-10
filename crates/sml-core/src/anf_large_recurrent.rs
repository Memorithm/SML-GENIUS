//! Bounded large-bank recurrent adapter, with atomic cycle commits.
use crate::{
    anf_large_bank::{LargeAnfBank, LargeBankError},
    semantic::{SemanticRewriteStep, SemanticRewriter, SemanticState},
};
use std::sync::Arc;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargeBankRecurrent<const S: usize> {
    bank: LargeAnfBank,
    max_depth: usize,
    history: Arc<Vec<Vec<u64>>>,
    routing_projection: Vec<(usize, usize)>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LargeRecurrentError {
    Depth,
    Width,
    Allocation,
    Bank(LargeBankError),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LargeRoutingProjectionError {
    Empty,
    Width,
    TooManyDestinations,
    DuplicateDestination(usize),
    NotFresh,
}
impl<const S: usize> LargeBankRecurrent<S> {
    pub fn new(bank: LargeAnfBank, max_depth: usize) -> Result<Self, LargeRecurrentError> {
        if S == 0
            || S > 31
            || bank.address_bits() < S as u32
            || bank.gate_count() < S
            || max_depth == 0
        {
            return Err(LargeRecurrentError::Width);
        }
        Ok(Self {
            bank,
            max_depth,
            history: Arc::new(Vec::new()),
            routing_projection: Vec::new(),
        })
    }
    /// Opt in to post-ANF value-bit -> SBG routing-bit assignments.
    /// Each destination is unique; broadcasting a source is allowed. Bits not
    /// assigned remain unchanged. This does not infer or learn a routing plan.
    pub fn with_routing_projection(
        mut self,
        projection: Vec<(usize, usize)>,
    ) -> Result<Self, LargeRoutingProjectionError> {
        if !self.history.is_empty() {
            return Err(LargeRoutingProjectionError::NotFresh);
        }
        if projection.is_empty() {
            return Err(LargeRoutingProjectionError::Empty);
        }
        if projection.len() > S {
            return Err(LargeRoutingProjectionError::TooManyDestinations);
        }
        for (index, &(source, destination)) in projection.iter().enumerate() {
            if source >= S || destination >= S {
                return Err(LargeRoutingProjectionError::Width);
            }
            if projection[..index]
                .iter()
                .any(|&(_, previous)| previous == destination)
            {
                return Err(LargeRoutingProjectionError::DuplicateDestination(
                    destination,
                ));
            }
        }
        self.routing_projection = projection;
        Ok(self)
    }
    #[must_use]
    pub fn routing_projection(&self) -> &[(usize, usize)] {
        &self.routing_projection
    }
    #[must_use]
    pub fn history(&self) -> &[Vec<u64>] {
        &self.history
    }
    /// Rewind the committed cycle history to a previous checkpoint.
    ///
    /// The caller must supply the semantic state corresponding to this
    /// checkpoint on the next rewrite; the history alone does not reconstruct
    /// a prior semantic state. A rejected checkpoint leaves history untouched.
    /// The configured routing projection is preserved.
    pub fn rewind_to(&mut self, completed_cycles: usize) -> Result<(), LargeRecurrentError> {
        if completed_cycles > self.history.len() {
            return Err(LargeRecurrentError::Depth);
        }
        if completed_cycles == self.history.len() {
            return Ok(());
        }
        let mut history = self.history.clone();
        Arc::make_mut(&mut history).truncate(completed_cycles);
        self.history = history;
        Ok(())
    }
}
impl<const S: usize> SemanticRewriter<S> for LargeBankRecurrent<S> {
    type Error = LargeRecurrentError;
    fn rewrite(
        &mut self,
        state: &SemanticState<S>,
        depth: u32,
    ) -> Result<SemanticRewriteStep<S>, Self::Error> {
        if usize::try_from(depth).ok() != self.history.len().checked_add(1)
            || self.history.len() >= self.max_depth
        {
            return Err(LargeRecurrentError::Depth);
        }
        let mut query = 0u32;
        for index in 0..S {
            if state
                .value
                .get(index)
                .map_err(|_| LargeRecurrentError::Width)?
            {
                query |= 1u32 << index;
            }
        }
        let packed = self
            .bank
            .evaluate_packed(&[query], 1, self.bank.gate_count().div_ceil(64))
            .map_err(LargeRecurrentError::Bank)?;
        let mut next = state.clone();
        for index in 0..S {
            let bit = (packed[index / 64] >> (index % 64)) & 1 != 0;
            next.value
                .set(index, bit)
                .map_err(|_| LargeRecurrentError::Width)?;
        }
        for &(source, destination) in &self.routing_projection {
            let value = next
                .value
                .get(source)
                .map_err(|_| LargeRecurrentError::Width)?;
            next.routing
                .set(destination, value)
                .map_err(|_| LargeRecurrentError::Width)?;
        }
        let mut history = self.history.clone();
        Arc::make_mut(&mut history)
            .try_reserve(1)
            .map_err(|_| LargeRecurrentError::Allocation)?;
        Arc::make_mut(&mut history).push(packed);
        self.history = history;
        Ok(SemanticRewriteStep {
            state: next,
            stability_blocked: false,
        })
    }
}
