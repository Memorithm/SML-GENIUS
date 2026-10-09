//! Model-owned ANF expert page bodies indexed by native MOR PageId.
//! These are declared ANF pages, not aliases for PackedBooleanPage encodings.
use crate::{
    pascal_vector_projection::{Pvp6PopulationProfile, Pvp6Selection, Pvp6Workload},
    pvp6_executor::{Pvp6Bank, Pvp6ExecutionError},
    pvp6_sbg::{SbgAnfError, SbgAnfPlan},
    semantic::SemanticState,
    PageId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SbgAnfExpert<const S: usize, const D: usize> {
    page: PageId,
    revision: u64,
    bank: Pvp6Bank<D>,
    plan: SbgAnfPlan<S, D>,
    population: Pvp6PopulationProfile,
    density: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SbgExpertEvidence {
    pub page: PageId,
    pub revision: u64,
    pub queries: Vec<usize>,
    pub outputs: Vec<u64>,
    pub gate_query_evaluations: u64,
    pub selection: Pvp6Selection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SbgExpertResult<const S: usize> {
    pub state: SemanticState<S>,
    pub evidence: SbgExpertEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SbgExpertError {
    Shape,
    Density,
    Overflow,
    Projection(SbgAnfError),
    Execution(Pvp6ExecutionError),
}

impl<const S: usize, const D: usize> SbgAnfExpert<S, D> {
    pub fn new(
        page: PageId,
        bank: Pvp6Bank<D>,
        plan: SbgAnfPlan<S, D>,
        population: Pvp6PopulationProfile,
    ) -> Result<Self, SbgExpertError> {
        if bank.gates().len() != plan.gate_count() {
            return Err(SbgExpertError::Shape);
        }
        let density = u32::try_from(bank.gates()[0].monomials().len())
            .map_err(|_| SbgExpertError::Overflow)?;
        if bank
            .gates()
            .iter()
            .any(|gate| gate.monomials().len() != density as usize)
        {
            return Err(SbgExpertError::Density);
        }
        u32::try_from(plan.input_lanes().len()).map_err(|_| SbgExpertError::Overflow)?;
        Ok(Self {
            page,
            revision: 0,
            bank,
            plan,
            population,
            density,
        })
    }

    #[must_use]
    pub fn page(&self) -> PageId {
        self.page
    }
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }
    #[must_use]
    pub fn bank(&self) -> &Pvp6Bank<D> {
        &self.bank
    }
    #[must_use]
    pub fn plan(&self) -> &SbgAnfPlan<S, D> {
        &self.plan
    }

    /// Logical gate/query work only; excludes physical allocation and timing.
    pub fn gate_query_evaluations(&self) -> Result<u64, SbgExpertError> {
        u64::try_from(self.plan.destinations().len()).map_err(|_| SbgExpertError::Overflow)
    }

    /// Stateless page execution: every query reads the same incoming SBG state.
    /// No inactive page is consulted and no truth-table cache is retained.
    pub fn execute(&self, state: &SemanticState<S>) -> Result<SbgExpertResult<S>, SbgExpertError> {
        let queries = self
            .plan
            .queries(state)
            .map_err(SbgExpertError::Projection)?;
        let workload = Pvp6Workload::new(self.density, queries.len() as u32, 1, self.population);
        let batch = self
            .bank
            .evaluate(workload, &queries)
            .map_err(SbgExpertError::Execution)?;
        let next = self
            .plan
            .apply(state, &batch)
            .map_err(SbgExpertError::Projection)?;
        let outputs = batch
            .to_query_major_words()
            .map_err(SbgExpertError::Execution)?;
        Ok(SbgExpertResult {
            state: next,
            evidence: SbgExpertEvidence {
                page: self.page,
                revision: self.revision,
                queries,
                outputs,
                gate_query_evaluations: self.gate_query_evaluations()?,
                selection: batch.selection(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        pvp6_executor::Pvp6Gate,
        pvp6_sbg::{SbgCoordinate, SbgField},
    };

    fn expert<const S: usize>() -> SbgAnfExpert<S, 64> {
        let fields = [
            SbgField::Value,
            SbgField::Meaning,
            SbgField::Ambiguity,
            SbgField::Routing,
        ];
        let plan = SbgAnfPlan::new(
            vec![(0..6)
                .map(|bit| SbgCoordinate {
                    field: SbgField::Value,
                    bit,
                })
                .collect()],
            fields
                .into_iter()
                .map(|field| SbgCoordinate { field, bit: S - 1 })
                .collect(),
        )
        .unwrap();
        let bank = Pvp6Bank::new(
            [vec![0, 1], vec![1, 2], vec![0, 3], vec![3, 7]]
                .iter()
                .map(|terms| Pvp6Gate::from_monomials(terms).unwrap())
                .collect(),
        )
        .unwrap();
        SbgAnfExpert::new(PageId(9), bank, plan, Pvp6PopulationProfile::AffineUniform).unwrap()
    }

    #[test]
    fn actual_page_body_matches_literal_oracle_and_preserves_unwritten_fields() {
        let page = expert::<65>();
        for x in 0..64 {
            let mut state = SemanticState::zeroed().unwrap();
            for bit in 0..6 {
                state.value.set(bit, x & (1 << bit) != 0).unwrap();
            }
            state.meaning_mask.set(33, true).unwrap();
            state.routing.set(31, true).unwrap();
            let before = page.clone();
            let result = page.execute(&state).unwrap();
            let mut expected = state.clone();
            let mut expected_word = 0;
            for (gate, dest) in page.plan().destinations().iter().enumerate() {
                let value =
                    page.bank().gates()[gate]
                        .monomials()
                        .iter()
                        .fold(false, |parity, mask| {
                            parity ^ (0..6).all(|bit| mask & (1 << bit) == 0 || x & (1 << bit) != 0)
                        });
                dest.write(&mut expected, value).unwrap();
                expected_word |= u64::from(value) << gate;
            }
            assert_eq!(result.state, expected);
            assert_eq!(result.evidence.outputs, vec![expected_word]);
            assert_eq!(result.evidence.queries, vec![x]);
            assert_eq!(result.evidence.gate_query_evaluations, 4);
            assert_eq!(result.evidence.page, PageId(9));
            assert!(result.evidence.selection.diagnostic_only);
            assert_eq!(page, before);
        }
    }

    #[test]
    fn malformed_page_shape_and_density_are_rejected_before_execution() {
        let page = expert::<65>();
        let bad = Pvp6Bank::new(vec![Pvp6Gate::from_monomials(&[0]).unwrap()]).unwrap();
        assert_eq!(
            SbgAnfExpert::new(
                PageId(1),
                bad,
                page.plan().clone(),
                Pvp6PopulationProfile::AffineUniform
            ),
            Err(SbgExpertError::Shape)
        );
        let bad = Pvp6Bank::new(vec![
            Pvp6Gate::from_monomials(&[0]).unwrap(),
            Pvp6Gate::from_monomials(&[0, 1]).unwrap(),
            Pvp6Gate::from_monomials(&[0]).unwrap(),
            Pvp6Gate::from_monomials(&[0]).unwrap(),
        ])
        .unwrap();
        assert_eq!(
            SbgAnfExpert::new(
                PageId(1),
                bad,
                page.plan().clone(),
                Pvp6PopulationProfile::AffineUniform
            ),
            Err(SbgExpertError::Density)
        );
    }
}
