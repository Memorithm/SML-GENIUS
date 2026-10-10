//! Native selected-page large-ANF qualification without replacing PVP6 prematurely.
//! Every selected page is evaluated against the same immutable SBG snapshot.
use crate::{
    anf_large_bank::{LargeAnfBank, LargeBankError},
    mor::MorDecision,
    pvp6_sbg::{SbgAnfError, SbgCoordinate},
    semantic::SemanticState,
    PageId,
};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargeAnfPage<const S: usize> {
    pub id: PageId,
    pub bank: LargeAnfBank,
    pub inputs: Vec<SbgCoordinate>,
    pub outputs: Vec<SbgCoordinate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LargePageError {
    Empty,
    Duplicate(PageId),
    Missing(PageId),
    Conflict(SbgCoordinate),
    Shape,
    Budget,
    Projection(SbgAnfError),
    Bank(LargeBankError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargePageTrace {
    pub id: PageId,
    pub address: u32,
    pub packed: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargePageResult<const S: usize> {
    pub state: SemanticState<S>,
    pub traces: Vec<LargePageTrace>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargeAnfPageAtlas<const S: usize> {
    pages: Vec<LargeAnfPage<S>>,
    max_selected: usize,
}

impl<const S: usize> LargeAnfPageAtlas<S> {
    pub fn new(
        mut pages: Vec<LargeAnfPage<S>>,
        max_selected: usize,
    ) -> Result<Self, LargePageError> {
        if pages.is_empty() || max_selected == 0 {
            return Err(LargePageError::Empty);
        }
        pages.sort_by_key(|page| page.id.0);
        let mut previous = None;
        for page in &pages {
            if previous == Some(page.id) {
                return Err(LargePageError::Duplicate(page.id));
            }
            previous = Some(page.id);
            let bits = page.bank.address_bits() as usize;
            if bits == 0
                || bits > 31
                || page.inputs.len() != bits
                || page.outputs.len() != page.bank.gate_count()
                || page.outputs.is_empty()
            {
                return Err(LargePageError::Shape);
            }
            let mut sources = BTreeSet::new();
            let mut destinations = BTreeSet::new();
            for &coordinate in &page.inputs {
                if coordinate.bit >= S || !sources.insert(coordinate) {
                    return Err(LargePageError::Shape);
                }
            }
            for &coordinate in &page.outputs {
                if coordinate.bit >= S || !destinations.insert(coordinate) {
                    return Err(LargePageError::Shape);
                }
            }
        }
        Ok(Self {
            pages,
            max_selected,
        })
    }

    pub fn gate_count(&self, id: PageId) -> Result<usize, LargePageError> {
        let index = self
            .pages
            .binary_search_by_key(&id.0, |page| page.id.0)
            .map_err(|_| LargePageError::Missing(id))?;
        Ok(self.pages[index].bank.gate_count())
    }

    pub fn execute(
        &self,
        state: &SemanticState<S>,
        decision: &MorDecision,
        max_gate_evaluations: usize,
    ) -> Result<LargePageResult<S>, LargePageError> {
        let ids = &decision.selected_experts;
        if ids.is_empty() || ids.len() > self.max_selected {
            return Err(LargePageError::Budget);
        }
        let mut selected = Vec::new();
        let mut unique = BTreeSet::new();
        let mut destinations = BTreeSet::new();
        let mut evaluations = 0usize;
        for &id in ids {
            if !unique.insert(id.0) {
                return Err(LargePageError::Duplicate(id));
            }
            let index = self
                .pages
                .binary_search_by_key(&id.0, |p| p.id.0)
                .map_err(|_| LargePageError::Missing(id))?;
            let page = &self.pages[index];
            evaluations = evaluations
                .checked_add(page.bank.gate_count())
                .ok_or(LargePageError::Budget)?;
            if evaluations > max_gate_evaluations {
                return Err(LargePageError::Budget);
            }
            for &coordinate in &page.outputs {
                if !destinations.insert(coordinate) {
                    return Err(LargePageError::Conflict(coordinate));
                }
            }
            selected.push(page);
        }
        let mut next = state.clone();
        let mut traces = Vec::new();
        for page in selected {
            let mut address = 0u32;
            for (bit, &coordinate) in page.inputs.iter().enumerate() {
                if coordinate.read(state).map_err(LargePageError::Projection)? {
                    address |= 1u32 << bit;
                }
            }
            let packed = page
                .bank
                .evaluate_packed(&[address], 1, page.bank.gate_count().div_ceil(64))
                .map_err(LargePageError::Bank)?;
            for (index, &coordinate) in page.outputs.iter().enumerate() {
                coordinate
                    .write(&mut next, (packed[index / 64] >> (index % 64)) & 1 != 0)
                    .map_err(LargePageError::Projection)?;
            }
            traces.push(LargePageTrace {
                id: page.id,
                address,
                packed,
            });
        }
        Ok(LargePageResult {
            state: next,
            traces,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{anf_large::LargeAnfGate, pvp6_sbg::SbgField};
    fn coord(bit: usize) -> SbgCoordinate {
        SbgCoordinate {
            field: SbgField::Value,
            bit,
        }
    }
    fn page(id: u32, destination: usize, terms: Vec<u32>) -> LargeAnfPage<24> {
        LargeAnfPage {
            id: PageId(id),
            bank: LargeAnfBank::new(vec![LargeAnfGate::new(20, terms).unwrap()]).unwrap(),
            inputs: (0..20).map(coord).collect(),
            outputs: vec![coord(destination)],
        }
    }
    fn decision(ids: &[u32]) -> MorDecision {
        MorDecision {
            active_routers: vec![],
            ranked_experts: vec![],
            selected_experts: ids.iter().copied().map(PageId).collect(),
            noise_ppm: 0,
            structural_disagreement_ppm: 0,
            expert_mass_threshold_ppm: 0,
            request_precision_escalation: false,
            hysteresis_retained_previous_primary: false,
        }
    }
    #[test]
    fn selected_large_page_reads_full_twenty_bit_address_and_excludes_inactive() {
        let atlas =
            LargeAnfPageAtlas::new(vec![page(7, 21, vec![1 << 19]), page(19, 22, vec![0])], 2)
                .unwrap();
        let mut state = SemanticState::<24>::zeroed().unwrap();
        state.value.set(19, true).unwrap();
        let result = atlas.execute(&state, &decision(&[7]), 1).unwrap();
        assert_eq!(result.traces.len(), 1);
        assert_eq!(result.traces[0].address, 1 << 19);
        assert_eq!(result.traces[0].packed, vec![1]);
        assert!(result.state.value.get(21).unwrap());
        assert!(!result.state.value.get(22).unwrap());
    }
    #[test]
    fn conflicts_missing_pages_and_budget_fail_without_state_changes() {
        let atlas =
            LargeAnfPageAtlas::new(vec![page(7, 21, vec![0]), page(19, 21, vec![0])], 2).unwrap();
        let state = SemanticState::<24>::zeroed().unwrap();
        let before = atlas.clone();
        for (ids, budget) in [
            (vec![7, 19], 2),
            (vec![7, 7], 2),
            (vec![99], 2),
            (vec![7], 0),
        ] {
            assert!(atlas.execute(&state, &decision(&ids), budget).is_err());
            assert_eq!(atlas, before);
        }
    }
}
