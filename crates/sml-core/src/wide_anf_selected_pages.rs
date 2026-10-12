//! Model-owned selected ANF pages with elastic sparse fan-in.
//! MOR chooses PageIds, and each page reads the SAME immutable SBG snapshot.
//! No previous LargeAnfPageAtlas or PVP6 API is changed.

use crate::{
    anf_wide_sparse::{WideAnfError, WideAnfGate},
    mor::MorDecision,
    pvp6_sbg::{SbgAnfError, SbgCoordinate},
    semantic::SemanticState,
    PageId,
};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WideAnfPage<const S: usize> {
    pub id: PageId,
    /// Each gate's physical indexes refer to `inputs`, not to all four
    /// semantic bitfields. The gate owns its compiled sparse coefficients.
    pub gates: Vec<WideAnfGate>,
    pub inputs: Vec<SbgCoordinate>,
    pub outputs: Vec<SbgCoordinate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WidePageError {
    Empty,
    Duplicate(PageId),
    Missing(PageId),
    Conflict(SbgCoordinate),
    Shape,
    Budget,
    Allocation,
    Projection(SbgAnfError),
    Execution(WideAnfError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WidePageTrace {
    pub id: PageId,
    pub inputs: usize,
    pub gates: usize,
    pub scratch_words: usize,
    pub packed: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WidePageResult<const S: usize> {
    pub state: SemanticState<S>,
    pub traces: Vec<WidePageTrace>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WideAnfPageAtlas<const S: usize> {
    pages: Vec<WideAnfPage<S>>,
    max_selected: usize,
    max_scratch_words_per_page: usize,
}

impl<const S: usize> WideAnfPageAtlas<S> {
    pub fn new(
        mut pages: Vec<WideAnfPage<S>>,
        max_selected: usize,
    ) -> Result<Self, WidePageError> {
        if S == 0 || pages.is_empty() || max_selected == 0 {
            return Err(WidePageError::Empty);
        }
        pages.sort_by_key(|page| page.id.0);
        let mut previous = None;
        for page in &pages {
            if previous == Some(page.id) {
                return Err(WidePageError::Duplicate(page.id));
            }
            previous = Some(page.id);
            if page.inputs.is_empty()
                || page.outputs.is_empty()
                || page.gates.len() != page.outputs.len()
            {
                return Err(WidePageError::Shape);
            }
            let mut sources = BTreeSet::new();
            let mut destinations = BTreeSet::new();
            for &c in &page.inputs {
                if c.bit >= S || !sources.insert(c) {
                    return Err(WidePageError::Shape);
                }
            }
            for &c in &page.outputs {
                if c.bit >= S || !destinations.insert(c) {
                    return Err(WidePageError::Shape);
                }
            }
            if page
                .gates
                .iter()
                .any(|gate| gate.input_positions().iter().any(|&i| i >= page.inputs.len()))
            {
                return Err(WidePageError::Shape);
            }
        }
        Ok(Self {
            pages,
            max_selected,
            max_scratch_words_per_page: usize::MAX,
        })
    }

    /// Explicit optional limit to reject expensive sparse-prefix scratch
    /// BEFORE allocating it. Compile-time gate limits are preserved.
    pub fn with_scratch_word_budget(mut self, max_words: usize) -> Result<Self, WidePageError> {
        if max_words == 0 {
            return Err(WidePageError::Budget);
        }
        self.max_scratch_words_per_page = max_words;
        Ok(self)
    }

    pub fn gate_count(&self, id: PageId) -> Result<usize, WidePageError> {
        let index = self
            .pages
            .binary_search_by_key(&id.0, |page| page.id.0)
            .map_err(|_| WidePageError::Missing(id))?;
        Ok(self.pages[index].gates.len())
    }

    /// Each selected page uses the unchanged input snapshot. All selected
    /// page IDs, destination conflicts and budgets are checked before
    /// any clone is written. Trace output is packed per selected page.
    pub fn execute(
        &self,
        state: &SemanticState<S>,
        decision: &MorDecision,
        max_gate_evaluations: usize,
        max_trace_words: usize,
    ) -> Result<WidePageResult<S>, WidePageError> {
        let ids = &decision.selected_experts;
        if ids.is_empty() || ids.len() > self.max_selected {
            return Err(WidePageError::Budget);
        }
        let mut selected = Vec::new();
        selected
            .try_reserve_exact(ids.len())
            .map_err(|_| WidePageError::Allocation)?;
        let mut unique = BTreeSet::new();
        let mut destinations = BTreeSet::new();
        let mut evaluations = 0usize;
        let mut trace_words = 0usize;
        for &id in ids {
            if !unique.insert(id.0) {
                return Err(WidePageError::Duplicate(id));
            }
            let index = self
                .pages
                .binary_search_by_key(&id.0, |page| page.id.0)
                .map_err(|_| WidePageError::Missing(id))?;
            let page = &self.pages[index];
            evaluations = evaluations
                .checked_add(page.gates.len())
                .ok_or(WidePageError::Budget)?;
            trace_words = trace_words
                .checked_add(page.gates.len().div_ceil(64))
                .ok_or(WidePageError::Budget)?;
            if evaluations > max_gate_evaluations || trace_words > max_trace_words {
                return Err(WidePageError::Budget);
            }
            if page.gates.iter().map(WideAnfGate::scratch_words).max().unwrap_or(0)
                > self.max_scratch_words_per_page
            {
                return Err(WidePageError::Budget);
            }
            for &coordinate in &page.outputs {
                if !destinations.insert(coordinate) {
                    return Err(WidePageError::Conflict(coordinate));
                }
            }
            selected.push(page);
        }
        let mut next = state.clone();
        let mut traces = Vec::new();
        traces
            .try_reserve_exact(selected.len())
            .map_err(|_| WidePageError::Allocation)?;
        for page in selected {
            // SBG coordinates are scalar per state; explicitly broadcast
            // into all 64 u64 lanes and execute the real wide gate backend.
            let mut input_words = Vec::new();
            input_words
                .try_reserve_exact(page.inputs.len())
                .map_err(|_| WidePageError::Allocation)?;
            for &coordinate in &page.inputs {
                let bit = coordinate.read(state).map_err(WidePageError::Projection)?;
                input_words.push(if bit { u64::MAX } else { 0 });
            }
            let scratch_words = page.gates.iter().map(WideAnfGate::scratch_words).max().unwrap_or(0);
            let mut scratch = Vec::new();
            scratch
                .try_reserve_exact(scratch_words)
                .map_err(|_| WidePageError::Allocation)?;
            scratch.resize(scratch_words, 0u64);
            let mut packed = Vec::new();
            packed
                .try_reserve_exact(page.gates.len().div_ceil(64))
                .map_err(|_| WidePageError::Allocation)?;
            packed.resize(page.gates.len().div_ceil(64), 0u64);
            for (index, gate) in page.gates.iter().enumerate() {
                let value = gate
                    .evaluate_lanes(&input_words, &mut scratch)
                    .map_err(WidePageError::Execution)?;
                // Broadcast input ensures every u64 lane has the same bit.
                if value & 1 != 0 {
                    packed[index / 64] |= 1u64 << (index % 64);
                }
            }
            for (index, &coordinate) in page.outputs.iter().enumerate() {
                coordinate
                    .write(&mut next, (packed[index / 64] >> (index % 64)) & 1 != 0)
                    .map_err(WidePageError::Projection)?;
            }
            traces.push(WidePageTrace {
                id: page.id,
                inputs: page.inputs.len(),
                gates: page.gates.len(),
                scratch_words,
                packed,
            });
        }
        Ok(WidePageResult { state: next, traces })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        anf_wide_sparse::WideAnfLimits,
        mor::MorDecision,
        pvp6_sbg::SbgField,
    };

    fn limit() -> WideAnfLimits {
        WideAnfLimits::new(2048, 256, 8192, 8192, 8192)
    }

    fn coord(field: SbgField, bit: usize) -> SbgCoordinate {
        SbgCoordinate { field, bit }
    }

    fn gate(k: usize, terms: Vec<Vec<usize>>) -> WideAnfGate {
        WideAnfGate::from_monomials((0..k).collect(), terms, k, limit()).unwrap()
    }

    fn page<const S: usize>(id: u32, k: usize, terms: Vec<Vec<usize>>, output_bit: usize)
      -> WideAnfPage<S>
    {
        WideAnfPage {
            id: PageId(id),
            gates: vec![gate(k, terms)],
            inputs: (0..k).map(|bit| coord(SbgField::Value, bit)).collect(),
            outputs: vec![coord(SbgField::Meaning, output_bit)],
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
    fn mor_selected_1024_input_and_reads_all_coordinates() {
        let atlas = WideAnfPageAtlas::<1024>::new(
            vec![
                page(0, 1024, vec![(0..1024).collect()], 0),
                page(1, 16, vec![vec![]], 1),
            ],
            2,
        )
        .unwrap()
        .with_scratch_word_budget(1025)
        .unwrap();
        let mut initial = SemanticState::<1024>::zeroed().unwrap();
        for i in 0..1024 {
            initial.value.set(i, true).unwrap();
        }
        let all = atlas.execute(&initial, &decision(&[0]), 1, 1).unwrap();
        assert_eq!(all.traces.len(), 1);
        assert_eq!(all.traces[0].inputs, 1024);
        assert_eq!(all.traces[0].scratch_words, 1025);
        assert!(all.state.meaning_mask.get(0).unwrap());
        assert!(!all.state.meaning_mask.get(1).unwrap());
        assert_eq!(all.traces[0].packed, vec![1]);

        for index in [0, 512, 1023] {
            let mut lowered = initial.clone();
            lowered.value.set(index, false).unwrap();
            let result = atlas.execute(&lowered, &decision(&[0]), 1, 1).unwrap();
            assert!(!result.state.meaning_mask.get(0).unwrap());
            assert_eq!(result.traces[0].packed, vec![0]);
        }
    }

    #[test]
    fn concurrent_pages_read_same_immutable_snapshot() {
        let page0 = WideAnfPage::<8> {
            id: PageId(0),
            gates: vec![gate(1, vec![vec![]])],
            inputs: vec![coord(SbgField::Value, 0)],
            outputs: vec![coord(SbgField::Meaning, 0)],
        };
        let page1 = WideAnfPage::<8> {
            id: PageId(1),
            gates: vec![gate(1, vec![vec![0]])],
            inputs: vec![coord(SbgField::Meaning, 0)],
            outputs: vec![coord(SbgField::Meaning, 1)],
        };
        let atlas = WideAnfPageAtlas::new(vec![page0, page1], 2).unwrap();
        let original = SemanticState::<8>::zeroed().unwrap();
        let result = atlas.execute(&original, &decision(&[0, 1]), 2, 2).unwrap();
        assert!(result.state.meaning_mask.get(0).unwrap());
        assert!(!result.state.meaning_mask.get(1).unwrap());
        assert_eq!(result.traces.len(), 2);
        assert_eq!(original, SemanticState::zeroed().unwrap());
    }

    #[test]
    fn conflict_missing_duplicate_and_all_budgets_fail_closed() {
        let first = page::<24>(0, 16, vec![vec![0]], 0);
        let mut conflict = page::<24>(1, 16, vec![vec![0]], 1);
        conflict.outputs[0] = first.outputs[0];
        let atlas = WideAnfPageAtlas::new(vec![first.clone(), conflict], 2).unwrap();
        let original = SemanticState::<24>::zeroed().unwrap();
        let before = atlas.clone();
        for (ids, gates, trace) in [
            (vec![0, 1], 2, 2),
            (vec![0, 0], 2, 2),
            (vec![99], 2, 2),
            (vec![0], 0, 1),
            (vec![0], 1, 0),
        ] {
            assert!(atlas.execute(&original, &decision(&ids), gates, trace).is_err());
            assert_eq!(atlas, before);
            assert_eq!(original, SemanticState::zeroed().unwrap());
        }
        assert_eq!(
            WideAnfPageAtlas::new(vec![first], 1)
                .unwrap()
                .with_scratch_word_budget(1)
                .unwrap()
                .execute(&original, &decision(&[0]), 1, 1),
            Err(WidePageError::Budget)
        );
    }

    #[test]
    fn constructor_rejects_invalid_and_duplicated_wiring() {
        let first = page::<16>(7, 16, vec![vec![0]], 0);
        assert_eq!(
            WideAnfPageAtlas::new(vec![first.clone(), first.clone()], 2),
            Err(WidePageError::Duplicate(PageId(7)))
        );
        let mut malformed = first.clone();
        malformed.inputs.push(malformed.inputs[0]);
        assert_eq!(WideAnfPageAtlas::new(vec![malformed], 1), Err(WidePageError::Shape));
        let mut out = first;
        out.outputs[0].bit = 16;
        assert_eq!(WideAnfPageAtlas::new(vec![out], 1), Err(WidePageError::Shape));
    }
}
