//! Sparse, sorted page atlas: only MOR-selected ANF bodies execute.
//! Selected pages read one snapshot and must write disjoint SBG coordinates.
use crate::{
    mor::MorDecision,
    pvp6_executor::Pvp6ExecutionPath,
    pvp6_sbg::{SbgAnfError, SbgCoordinate, SbgField},
    pvp6_sbg_experts::{SbgAnfExpert, SbgExpertError, SbgExpertEvidence},
    semantic::SemanticState,
    PageId,
};
use std::{collections::BTreeSet, sync::Arc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SbgExpertAtlas<const S: usize, const D: usize> {
    pages: Arc<Vec<SbgAnfExpert<S, D>>>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SbgExpertWork {
    pub pages: u64,
    pub gate_queries: u64,
    pub retained_words: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SbgExpertBatch<const S: usize> {
    pub state: SemanticState<S>,
    pub pages: Vec<SbgExpertEvidence>,
    pub work: SbgExpertWork,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SbgAtlasError {
    Empty,
    Limit,
    DuplicatePage(PageId),
    MissingPage(PageId),
    DuplicateSelection(PageId),
    WriteConflict(SbgCoordinate),
    Overflow,
    Allocation,
    Page(SbgExpertError),
    Projection(SbgAnfError),
}
impl<const S: usize, const D: usize> SbgExpertAtlas<S, D> {
    pub fn new(
        mut pages: Vec<SbgAnfExpert<S, D>>,
        max_pages: usize,
    ) -> Result<Self, SbgAtlasError> {
        if pages.is_empty() {
            return Err(SbgAtlasError::Empty);
        }
        if pages.len() > max_pages {
            return Err(SbgAtlasError::Limit);
        }
        pages.sort_by_key(|page| page.page().0);
        for pair in pages.windows(2) {
            if pair[0].page() == pair[1].page() {
                return Err(SbgAtlasError::DuplicatePage(pair[0].page()));
            }
        }
        Ok(Self {
            pages: Arc::new(pages),
        })
    }
    #[must_use]
    pub fn pages(&self) -> &[SbgAnfExpert<S, D>] {
        &self.pages
    }
    pub fn page(&self, id: PageId) -> Result<&SbgAnfExpert<S, D>, SbgAtlasError> {
        self.pages
            .binary_search_by_key(&id.0, |page| page.page().0)
            .map(|i| &self.pages[i])
            .map_err(|_| SbgAtlasError::MissingPage(id))
    }
    fn selected(&self, ids: &[PageId]) -> Result<Vec<&SbgAnfExpert<S, D>>, SbgAtlasError> {
        if ids.is_empty() {
            return Err(SbgAtlasError::Empty);
        }
        if ids.len() > self.pages.len() {
            return Err(SbgAtlasError::Limit);
        }
        let mut unique = BTreeSet::new();
        let mut selected = Vec::new();
        selected
            .try_reserve_exact(ids.len())
            .map_err(|_| SbgAtlasError::Allocation)?;
        for &id in ids {
            if !unique.insert(id.0) {
                return Err(SbgAtlasError::DuplicateSelection(id));
            }
            selected.push(self.page(id)?);
        }
        // A plan already rejects duplicate destinations, so one page never
        // conflicts with itself; only several pages need the occupancy check.
        if selected.len() > 1 {
            let words = S.div_ceil(64);
            let mut written = Vec::new();
            written
                .try_reserve_exact(4 * words)
                .map_err(|_| SbgAtlasError::Allocation)?;
            written.resize(4 * words, 0_u64);
            for page in &selected {
                for &dest in page.plan().destinations() {
                    let field = match dest.field {
                        SbgField::Value => 0,
                        SbgField::Meaning => 1,
                        SbgField::Ambiguity => 2,
                        SbgField::Routing => 3,
                    };
                    let word = &mut written[field * words + dest.bit / 64];
                    let mask = 1_u64 << (dest.bit % 64);
                    if *word & mask != 0 {
                        return Err(SbgAtlasError::WriteConflict(dest));
                    }
                    *word |= mask;
                }
            }
        }
        Ok(selected)
    }
    pub fn replace_bank(
        &mut self,
        id: PageId,
        expected_revision: u64,
        bank: crate::pvp6_executor::Pvp6Bank<D>,
    ) -> Result<u64, SbgAtlasError> {
        let index = self
            .pages
            .binary_search_by_key(&id.0, |page| page.page().0)
            .map_err(|_| SbgAtlasError::MissingPage(id))?;
        Arc::make_mut(&mut self.pages)[index]
            .replace_bank(expected_revision, bank)
            .map_err(SbgAtlasError::Page)
    }

    /// Logical execution/retained query-output cost, validated before any body executes.
    pub fn work_for(&self, ids: &[PageId]) -> Result<SbgExpertWork, SbgAtlasError> {
        Self::work_of(&self.selected(ids)?)
    }
    fn work_of(selected: &[&SbgAnfExpert<S, D>]) -> Result<SbgExpertWork, SbgAtlasError> {
        let mut work = SbgExpertWork::default();
        for page in selected {
            let q = u64::try_from(page.plan().input_lanes().len())
                .map_err(|_| SbgAtlasError::Overflow)?;
            let g = u64::try_from(page.plan().gate_count()).map_err(|_| SbgAtlasError::Overflow)?;
            let words = q
                .checked_mul(g.div_ceil(64))
                .and_then(|n| n.checked_add(q))
                .ok_or(SbgAtlasError::Overflow)?;
            work.pages = work.pages.checked_add(1).ok_or(SbgAtlasError::Overflow)?;
            work.gate_queries = work
                .gate_queries
                .checked_add(page.gate_query_evaluations().map_err(SbgAtlasError::Page)?)
                .ok_or(SbgAtlasError::Overflow)?;
            work.retained_words = work
                .retained_words
                .checked_add(words)
                .ok_or(SbgAtlasError::Overflow)?;
        }
        Ok(work)
    }
    pub fn execute(
        &self,
        state: &SemanticState<S>,
        decision: &MorDecision,
    ) -> Result<SbgExpertBatch<S>, SbgAtlasError> {
        self.execute_on(Pvp6ExecutionPath::Selected, state, decision)
    }

    /// [`Self::execute`] with an explicit exact bank arm for every selected page.
    pub fn execute_on(
        &self,
        path: Pvp6ExecutionPath,
        state: &SemanticState<S>,
        decision: &MorDecision,
    ) -> Result<SbgExpertBatch<S>, SbgAtlasError> {
        let selected = self.selected(&decision.selected_experts)?;
        let work = Self::work_of(&selected)?;
        let mut next = state.clone();
        let mut evidence = Vec::new();
        evidence
            .try_reserve_exact(selected.len())
            .map_err(|_| SbgAtlasError::Allocation)?;
        // Every page reads the same snapshot and writes disjoint coordinates
        // straight into `next`, which is only returned once all pages succeed.
        for page in selected {
            evidence.push(
                page.execute_into(path, state, &mut next)
                    .map_err(SbgAtlasError::Page)?,
            );
        }
        Ok(SbgExpertBatch {
            state: next,
            pages: evidence,
            work,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        pascal_vector_projection::Pvp6PopulationProfile,
        pvp6_executor::{Pvp6Bank, Pvp6Gate},
        pvp6_sbg::{SbgAnfPlan, SbgField},
    };
    fn page(id: u32, out: usize, terms: &[u16]) -> SbgAnfExpert<8, 64> {
        let plan = SbgAnfPlan::new(
            vec![(0..6)
                .map(|bit| SbgCoordinate {
                    field: SbgField::Value,
                    bit,
                })
                .collect()],
            vec![SbgCoordinate {
                field: SbgField::Value,
                bit: out,
            }],
        )
        .unwrap();
        SbgAnfExpert::new(
            PageId(id),
            Pvp6Bank::new(vec![Pvp6Gate::from_monomials(terms).unwrap()]).unwrap(),
            plan,
            Pvp6PopulationProfile::AffineUniform,
        )
        .unwrap()
    }
    fn decision(pages: &[u32]) -> MorDecision {
        MorDecision {
            active_routers: vec![],
            ranked_experts: vec![],
            selected_experts: pages.iter().map(|&p| PageId(p)).collect(),
            noise_ppm: 0,
            structural_disagreement_ppm: 0,
            expert_mass_threshold_ppm: 0,
            request_precision_escalation: false,
            hysteresis_retained_previous_primary: false,
        }
    }
    #[test]
    fn sparse_ids_do_not_allocate_holes_or_execute_inactive_pages() {
        let atlas = SbgExpertAtlas::new(
            vec![
                page(u32::MAX, 0, &[0]),
                page(7, 0, &[1]),
                page(19, 1, &[0, 1]),
            ],
            3,
        )
        .unwrap();
        let result = atlas
            .execute(&SemanticState::zeroed().unwrap(), &decision(&[7]))
            .unwrap();
        assert_eq!(atlas.pages().len(), 3);
        assert_eq!(result.pages.len(), 1);
        assert_eq!(result.pages[0].page, PageId(7));
        assert!(!result.state.value.get(0).unwrap());
        assert_eq!(
            result.work,
            SbgExpertWork {
                pages: 1,
                gate_queries: 1,
                retained_words: 2
            }
        );
    }
    #[test]
    fn selected_pages_read_same_snapshot_and_disjoint_writes_commute() {
        let atlas = SbgExpertAtlas::new(vec![page(7, 0, &[0]), page(19, 1, &[1])], 2).unwrap();
        let state = SemanticState::zeroed().unwrap();
        let a = atlas.execute(&state, &decision(&[7, 19])).unwrap();
        let b = atlas.execute(&state, &decision(&[19, 7])).unwrap();
        assert_eq!(a.state, b.state);
        assert!(a.state.value.get(0).unwrap());
        assert!(!a.state.value.get(1).unwrap());
        assert_eq!(
            a.pages.iter().map(|p| p.page).collect::<Vec<_>>(),
            vec![PageId(7), PageId(19)]
        );
        assert_eq!(
            a.work,
            SbgExpertWork {
                pages: 2,
                gate_queries: 2,
                retained_words: 4
            }
        );
    }
    #[test]
    fn empty_duplicate_missing_and_conflicting_selections_fail_without_mutation() {
        let atlas = SbgExpertAtlas::new(vec![page(7, 0, &[0]), page(19, 0, &[1])], 2).unwrap();
        let before = atlas.clone();
        let state = SemanticState::zeroed().unwrap();
        for ids in [vec![], vec![7, 7], vec![8], vec![7, 19]] {
            assert!(atlas.execute(&state, &decision(&ids)).is_err());
            assert_eq!(atlas, before);
        }
        assert!(matches!(
            SbgExpertAtlas::new(vec![page(7, 0, &[0]), page(7, 1, &[1])], 2),
            Err(SbgAtlasError::DuplicatePage(PageId(7)))
        ));
        assert!(matches!(
            SbgExpertAtlas::new(vec![page(7, 0, &[0])], 0),
            Err(SbgAtlasError::Limit)
        ));
    }
    #[test]
    fn clone_shares_immutable_banks_and_update_detaches_without_stale_results() {
        let original = SbgExpertAtlas::new(vec![page(7, 0, &[0]), page(19, 1, &[1])], 2).unwrap();
        let mut candidate = original.clone();
        assert!(Arc::ptr_eq(&original.pages, &candidate.pages));
        let input = SemanticState::zeroed().unwrap();
        let old = original.execute(&input, &decision(&[7])).unwrap();
        let revision = candidate.page(PageId(7)).unwrap().revision();
        let bank = Pvp6Bank::new(vec![Pvp6Gate::from_monomials(&[1]).unwrap()]).unwrap();
        assert_eq!(
            candidate.replace_bank(PageId(7), revision, bank).unwrap(),
            revision + 1
        );
        assert!(!Arc::ptr_eq(&original.pages, &candidate.pages));
        assert_eq!(original.execute(&input, &decision(&[7])).unwrap(), old);
        assert_ne!(
            candidate.execute(&input, &decision(&[7])).unwrap().state,
            old.state
        );
        let snapshot = candidate.clone();
        assert!(Arc::ptr_eq(&snapshot.pages, &candidate.pages));
        let invalid = Pvp6Bank::new(vec![Pvp6Gate::from_monomials(&[0, 1]).unwrap()]).unwrap();
        assert!(candidate
            .replace_bank(PageId(7), revision, invalid)
            .is_err());
        assert_eq!(candidate, snapshot);
    }
    fn wide_page(id: u32, first_output: usize, seed: u64) -> SbgAnfExpert<128, 64> {
        let lanes = 9;
        let gates = 5;
        let inputs = (0..lanes)
            .map(|lane| {
                (0..6)
                    .map(|bit| SbgCoordinate {
                        field: SbgField::Value,
                        bit: (lane * 7 + bit * 11) % 128,
                    })
                    .collect()
            })
            .collect();
        let outputs = (0..lanes * gates)
            .map(|i| SbgCoordinate {
                field: SbgField::Meaning,
                bit: first_output + i,
            })
            .collect();
        let mut state = seed;
        let bank = (0..gates)
            .map(|_| {
                let mut terms = Vec::new();
                while terms.len() < 3 {
                    state = state
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(1);
                    let mask = (state >> 58) as u16;
                    if !terms.contains(&mask) {
                        terms.push(mask);
                    }
                }
                Pvp6Gate::from_monomials(&terms).unwrap()
            })
            .collect();
        SbgAnfExpert::new(
            PageId(id),
            Pvp6Bank::new(bank).unwrap(),
            SbgAnfPlan::new(inputs, outputs).unwrap(),
            Pvp6PopulationProfile::AffineUniform,
        )
        .unwrap()
    }
    #[test]
    fn every_bank_arm_yields_the_identical_selected_atlas_batch() {
        use crate::pascal_vector_projection::Pvp6Architecture;
        let atlas = SbgExpertAtlas::new(vec![wide_page(3, 0, 1), wide_page(5, 45, 2)], 2).unwrap();
        let paths = [
            Pvp6ExecutionPath::Scalar(Pvp6Architecture::PascalAnfMaterialize),
            Pvp6ExecutionPath::Scalar(Pvp6Architecture::SparseAnfScan),
            Pvp6ExecutionPath::Scalar(Pvp6Architecture::SubmaskCoefficientQuery),
            Pvp6ExecutionPath::Bitsliced,
        ];
        for seed in 0..32_u64 {
            let mut state = SemanticState::<128>::zeroed().unwrap();
            for bit in 0..128 {
                let set = seed
                    .wrapping_mul(0x9e37_79b9_7f4a_7c15)
                    .rotate_left(bit as u32)
                    & 1
                    == 1;
                state.value.set(bit, set).unwrap();
                state.meaning_mask.set(bit, !set).unwrap();
            }
            for ids in [vec![3], vec![5, 3]] {
                let reference = atlas.execute(&state, &decision(&ids)).unwrap();
                for path in paths {
                    assert_eq!(
                        atlas.execute_on(path, &state, &decision(&ids)).unwrap(),
                        reference
                    );
                }
            }
            for path in paths {
                assert_eq!(
                    atlas.execute_on(path, &state, &decision(&[3, 3])),
                    atlas.execute(&state, &decision(&[3, 3]))
                );
            }
        }
    }
}
