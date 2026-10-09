//! Sparse, sorted page atlas: only MOR-selected ANF bodies execute.
//! Selected pages read one snapshot and must write disjoint SBG coordinates.
use crate::{
    mor::MorDecision,
    pvp6_sbg::{SbgAnfError, SbgCoordinate},
    pvp6_sbg_experts::{SbgAnfExpert, SbgExpertError, SbgExpertEvidence},
    semantic::SemanticState,
    PageId,
};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SbgExpertAtlas<const S: usize, const D: usize> {
    pages: Vec<SbgAnfExpert<S, D>>,
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
        Ok(Self { pages })
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
        let mut writes = BTreeSet::new();
        let mut selected = Vec::new();
        for &id in ids {
            if !unique.insert(id.0) {
                return Err(SbgAtlasError::DuplicateSelection(id));
            }
            let page = self.page(id)?;
            for &dest in page.plan().destinations() {
                if !writes.insert(dest) {
                    return Err(SbgAtlasError::WriteConflict(dest));
                }
            }
            selected.push(page);
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
        self.pages[index]
            .replace_bank(expected_revision, bank)
            .map_err(SbgAtlasError::Page)
    }

    /// Logical execution/retained query-output cost, validated before any body executes.
    pub fn work_for(&self, ids: &[PageId]) -> Result<SbgExpertWork, SbgAtlasError> {
        let selected = self.selected(ids)?;
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
        let work = self.work_for(&decision.selected_experts)?;
        let selected = self.selected(&decision.selected_experts)?;
        let mut next = state.clone();
        let mut evidence = Vec::new();
        evidence
            .try_reserve_exact(selected.len())
            .map_err(|_| SbgAtlasError::Allocation)?;
        for page in selected {
            let result = page.execute(state).map_err(SbgAtlasError::Page)?;
            for &dest in page.plan().destinations() {
                let value = dest
                    .read(&result.state)
                    .map_err(SbgAtlasError::Projection)?;
                dest.write(&mut next, value)
                    .map_err(SbgAtlasError::Projection)?;
            }
            evidence.push(result.evidence);
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
}
