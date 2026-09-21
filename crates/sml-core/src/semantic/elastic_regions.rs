//! SBG-driven activation of existing Elastic Weight pages.
//!
//! Semantic evidence selects which weight pages should be active, while the
//! existing `ElasticWeightPlanner` remains authoritative for precision,
//! residency, RAM/VRAM budgets, PLAN/VALIDATE/COMMIT and generation checks.
//! This bridge never commits implicitly and never mutates page importance.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    elastic_weights::{
        ElasticWeightError, ElasticWeightPlan, ElasticWeightPlanner, IMPORTANCE_SCALE_PPM,
    },
    PageId,
};

use super::{masked_score, SemanticBitField, SemanticMetricError, SemanticState};

/// Semantic signatures associated with one Elastic Weight page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticWeightRegion<const BITS: usize> {
    pub page_id: PageId,
    /// Expected routed semantic domain for this page.
    pub routing_signature: SemanticBitField<BITS>,
    /// Expected semantic value on context-selected coordinates.
    pub value_signature: SemanticBitField<BITS>,
}

/// Bounded semantic activation policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticElasticPolicy {
    /// Maximum number of semantically selected pages presented to the planner.
    pub max_selected_pages: usize,
    /// Minimum combined semantic evidence required for eligibility.
    pub min_combined_similarity_ppm: u32,
}

impl SemanticElasticPolicy {
    pub fn validate(self) -> Result<Self, SemanticElasticError> {
        if self.max_selected_pages == 0 {
            return Err(SemanticElasticError::ZeroPageBudget);
        }
        if self.min_combined_similarity_ppm > IMPORTANCE_SCALE_PPM {
            return Err(SemanticElasticError::SimilarityThresholdOutOfRange(
                self.min_combined_similarity_ppm,
            ));
        }
        Ok(self)
    }
}

/// Auditable semantic evidence for one weight page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticWeightRegionScore {
    pub page_id: PageId,
    pub routing_similarity_ppm: u32,
    pub contextual_value_similarity_ppm: u32,
    pub combined_similarity_ppm: u32,
}

/// Exact working-set projection from the validated Elastic Weight plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticElasticWorkingSet {
    pub selected_pages: usize,
    pub active_parameters: u64,
    pub active_storage_bits: u128,
    pub ram_bytes: u64,
    pub vram_bytes: u64,
    pub bytes_moved_to_ram: u64,
    pub bytes_moved_to_vram: u64,
}

/// Immutable semantic selection plus the native Elastic Weight plan it produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticElasticPlan {
    pub scores: Vec<SemanticWeightRegionScore>,
    pub selected_pages: Vec<PageId>,
    pub elastic_plan: ElasticWeightPlan,
}

impl SemanticElasticPlan {
    #[must_use]
    pub fn working_set(&self) -> SemanticElasticWorkingSet {
        SemanticElasticWorkingSet {
            selected_pages: self.selected_pages.len(),
            active_parameters: self.elastic_plan.active_parameters,
            active_storage_bits: self.elastic_plan.active_storage_bits,
            ram_bytes: self.elastic_plan.ram_bytes,
            vram_bytes: self.elastic_plan.vram_bytes,
            bytes_moved_to_ram: self.elastic_plan.bytes_moved_to_ram,
            bytes_moved_to_vram: self.elastic_plan.bytes_moved_to_vram,
        }
    }

    fn validate_selected_set(&self) -> Result<(), SemanticElasticError> {
        let mut expected = BTreeSet::new();
        for page_id in &self.selected_pages {
            if !expected.insert(page_id.0) {
                return Err(SemanticElasticError::DuplicateSelectedPage(*page_id));
            }
        }

        let actual = self
            .elastic_plan
            .transitions
            .iter()
            .filter(|transition| transition.to_active)
            .map(|transition| transition.page_id.0)
            .collect::<BTreeSet<_>>();

        if expected != actual {
            return Err(SemanticElasticError::PlanSelectionMismatch);
        }
        Ok(())
    }
}

/// Deterministic SBG adapter over the native Elastic Weight planner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticElasticBridge<const BITS: usize> {
    regions: BTreeMap<u32, SemanticWeightRegion<BITS>>,
    policy: SemanticElasticPolicy,
}

impl<const BITS: usize> SemanticElasticBridge<BITS> {
    pub fn new(
        regions: Vec<SemanticWeightRegion<BITS>>,
        policy: SemanticElasticPolicy,
    ) -> Result<Self, SemanticElasticError> {
        let policy = policy.validate()?;
        if regions.is_empty() {
            return Err(SemanticElasticError::EmptyRegions);
        }

        let mut indexed = BTreeMap::new();
        for region in regions {
            let page_id = region.page_id;
            if indexed.insert(page_id.0, region).is_some() {
                return Err(SemanticElasticError::DuplicateRegion(page_id));
            }
        }

        Ok(Self {
            regions: indexed,
            policy,
        })
    }

    #[must_use]
    pub const fn policy(&self) -> SemanticElasticPolicy {
        self.policy
    }

    #[must_use]
    pub fn region_count(&self) -> usize {
        self.regions.len()
    }

    /// Produces an immutable native Elastic Weight plan from SBG state.
    ///
    /// The state routing field supplies global domain evidence. The state value
    /// field is compared to each region value signature only on coordinates
    /// enabled by the context meaning mask. The two integer ppm scores are
    /// averaged with equal weight. Semantic selection does not mutate the
    /// planner and does not overwrite caller-owned page importance evidence.
    pub fn plan(
        &self,
        state: &SemanticState<BITS>,
        planner: &ElasticWeightPlanner,
        epoch: u64,
    ) -> Result<SemanticElasticPlan, SemanticElasticError> {
        self.validate_against_planner(planner)?;

        let mut scores = Vec::with_capacity(self.regions.len());
        for region in self.regions.values() {
            let routing_similarity_ppm =
                full_similarity_ppm(&state.routing, &region.routing_signature)?;
            let contextual_value_similarity_ppm =
                masked_score(&state.value, &region.value_signature, &state.meaning_mask)
                    .map_err(SemanticElasticError::Metric)?
                    .similarity_ppm();
            let combined_similarity_ppm = ((u64::from(routing_similarity_ppm)
                + u64::from(contextual_value_similarity_ppm))
                / 2) as u32;

            scores.push(SemanticWeightRegionScore {
                page_id: region.page_id,
                routing_similarity_ppm,
                contextual_value_similarity_ppm,
                combined_similarity_ppm,
            });
        }

        scores.sort_by(|left, right| {
            right
                .combined_similarity_ppm
                .cmp(&left.combined_similarity_ppm)
                .then_with(|| {
                    right
                        .routing_similarity_ppm
                        .cmp(&left.routing_similarity_ppm)
                })
                .then_with(|| {
                    right
                        .contextual_value_similarity_ppm
                        .cmp(&left.contextual_value_similarity_ppm)
                })
                .then_with(|| left.page_id.0.cmp(&right.page_id.0))
        });

        let selected_pages = scores
            .iter()
            .filter(|score| {
                score.combined_similarity_ppm >= self.policy.min_combined_similarity_ppm
            })
            .take(self.policy.max_selected_pages)
            .map(|score| score.page_id)
            .collect::<Vec<_>>();

        if selected_pages.is_empty() {
            return Err(SemanticElasticError::NoEligibleRegion);
        }

        let elastic_plan = planner
            .plan_cycle(&selected_pages, epoch)
            .map_err(SemanticElasticError::Elastic)?;
        elastic_plan
            .validate_contract_v1()
            .map_err(SemanticElasticError::Elastic)?;

        let result = SemanticElasticPlan {
            scores,
            selected_pages,
            elastic_plan,
        };
        result.validate_selected_set()?;
        Ok(result)
    }

    /// Explicitly commits a previously planned semantic activation.
    ///
    /// Native Elastic Weight validation, stale-generation protection and source
    /// state checks remain authoritative.
    pub fn commit(
        &self,
        planner: &mut ElasticWeightPlanner,
        plan: &SemanticElasticPlan,
    ) -> Result<(), SemanticElasticError> {
        self.validate_against_planner(planner)?;
        plan.elastic_plan
            .validate_contract_v1()
            .map_err(SemanticElasticError::Elastic)?;
        plan.validate_selected_set()?;
        planner
            .commit(&plan.elastic_plan)
            .map_err(SemanticElasticError::Elastic)
    }

    fn validate_against_planner(
        &self,
        planner: &ElasticWeightPlanner,
    ) -> Result<(), SemanticElasticError> {
        let planner_pages = planner
            .pages()
            .iter()
            .map(|page| page.page_id.0)
            .collect::<BTreeSet<_>>();

        for page_id in &planner_pages {
            if !self.regions.contains_key(page_id) {
                return Err(SemanticElasticError::MissingRegion(PageId(*page_id)));
            }
        }
        for page_id in self.regions.keys() {
            if !planner_pages.contains(page_id) {
                return Err(SemanticElasticError::UnknownRegion(PageId(*page_id)));
            }
        }

        Ok(())
    }
}

fn full_similarity_ppm<const BITS: usize>(
    left: &SemanticBitField<BITS>,
    right: &SemanticBitField<BITS>,
) -> Result<u32, SemanticElasticError> {
    if BITS == 0 {
        return Err(SemanticElasticError::ZeroSemanticWidth);
    }
    let matching = left.xnor_popcount(right) as u128;
    let ppm = matching
        .checked_mul(IMPORTANCE_SCALE_PPM as u128)
        .ok_or(SemanticElasticError::ArithmeticOverflow)?
        / BITS as u128;
    u32::try_from(ppm).map_err(|_| SemanticElasticError::ArithmeticOverflow)
}

/// Invalid SBG-to-Elastic-Weights integration request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticElasticError {
    ZeroPageBudget,
    SimilarityThresholdOutOfRange(u32),
    EmptyRegions,
    DuplicateRegion(PageId),
    MissingRegion(PageId),
    UnknownRegion(PageId),
    DuplicateSelectedPage(PageId),
    NoEligibleRegion,
    PlanSelectionMismatch,
    ZeroSemanticWidth,
    ArithmeticOverflow,
    Metric(SemanticMetricError),
    Elastic(ElasticWeightError),
}

impl fmt::Display for SemanticElasticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SemanticElasticError {}

#[cfg(test)]
mod tests {
    use super::{
        SemanticElasticBridge, SemanticElasticError, SemanticElasticPolicy, SemanticWeightRegion,
    };
    use crate::{
        elastic_weights::{
            ElasticWeightError, ElasticWeightPage, ElasticWeightPlanner, ElasticWeightPolicy,
            ResidencyTier, WeightPrecision,
        },
        semantic::{SemanticBitField, SemanticState},
        PageId,
    };

    fn planner() -> ElasticWeightPlanner {
        ElasticWeightPlanner::new(
            vec![
                ElasticWeightPage::new(
                    PageId(0),
                    8,
                    WeightPrecision::Boolean,
                    ResidencyTier::Disk,
                    500_000,
                )
                .expect("page 0"),
                ElasticWeightPage::new(
                    PageId(1),
                    8,
                    WeightPrecision::Boolean,
                    ResidencyTier::Disk,
                    500_000,
                )
                .expect("page 1"),
            ],
            ElasticWeightPolicy {
                ram_limit_bytes: 8,
                vram_limit_bytes: 8,
                max_active_parameters: 8,
                ternary_threshold_ppm: 600_000,
                residual4_threshold_ppm: 900_000,
                vram_threshold_ppm: 900_000,
            },
        )
        .expect("planner")
    }

    fn bridge() -> SemanticElasticBridge<8> {
        let common_routing =
            SemanticBitField::<8>::from_set_indices(&[0, 1, 2, 3]).expect("routing");
        SemanticElasticBridge::new(
            vec![
                SemanticWeightRegion {
                    page_id: PageId(0),
                    routing_signature: common_routing.clone(),
                    value_signature: SemanticBitField::from_set_indices(&[0, 2, 5, 7])
                        .expect("finance"),
                },
                SemanticWeightRegion {
                    page_id: PageId(1),
                    routing_signature: common_routing,
                    value_signature: SemanticBitField::from_set_indices(&[1, 3, 4, 6])
                        .expect("river"),
                },
            ],
            SemanticElasticPolicy {
                max_selected_pages: 1,
                min_combined_similarity_ppm: 500_000,
            },
        )
        .expect("bridge")
    }

    fn state_with_mask(mask: &[usize]) -> SemanticState<8> {
        SemanticState {
            value: SemanticBitField::from_set_indices(&[0, 2, 4, 6]).expect("value"),
            meaning_mask: SemanticBitField::from_set_indices(mask).expect("mask"),
            ambiguity: SemanticBitField::zeroed().expect("ambiguity"),
            routing: SemanticBitField::from_set_indices(&[0, 1, 2, 3]).expect("routing"),
        }
    }

    #[test]
    fn context_mask_changes_selected_weight_region() {
        let planner = planner();
        let bridge = bridge();

        let finance = bridge
            .plan(&state_with_mask(&[0, 1, 2, 3]), &planner, 1)
            .expect("finance plan");
        let river = bridge
            .plan(&state_with_mask(&[4, 5, 6, 7]), &planner, 1)
            .expect("river plan");

        assert_eq!(finance.selected_pages, vec![PageId(0)]);
        assert_eq!(river.selected_pages, vec![PageId(1)]);
        assert_eq!(finance.scores[0].combined_similarity_ppm, 1_000_000);
        assert_eq!(river.scores[0].combined_similarity_ppm, 1_000_000);
    }

    #[test]
    fn planning_is_non_mutating_and_commit_is_explicit() {
        let mut planner = planner();
        let bridge = bridge();
        let state = state_with_mask(&[0, 1, 2, 3]);

        let plan = bridge.plan(&state, &planner, 7).expect("plan");
        assert_eq!(planner.generation(), 0);
        assert_eq!(planner.accounting().expect("accounting").active_parameters, 0);

        let working_set = plan.working_set();
        assert_eq!(working_set.selected_pages, 1);
        assert_eq!(working_set.active_parameters, 8);
        assert_eq!(working_set.active_storage_bits, 8);
        assert_eq!(working_set.ram_bytes, 1);
        assert_eq!(working_set.vram_bytes, 0);
        assert_eq!(working_set.bytes_moved_to_ram, 1);

        bridge.commit(&mut planner, &plan).expect("commit");
        assert_eq!(planner.generation(), 1);
        let accounting = planner.accounting().expect("accounting");
        assert_eq!(accounting.active_parameters, 8);
        assert_eq!(accounting.ram_bytes, 1);
    }

    #[test]
    fn native_stale_generation_protection_is_preserved() {
        let mut planner = planner();
        let bridge = bridge();
        let state = state_with_mask(&[0, 1, 2, 3]);

        let first = bridge.plan(&state, &planner, 1).expect("first");
        let stale = bridge.plan(&state, &planner, 2).expect("stale");
        bridge.commit(&mut planner, &first).expect("first commit");

        assert_eq!(
            bridge.commit(&mut planner, &stale),
            Err(SemanticElasticError::Elastic(
                ElasticWeightError::StalePlan {
                    expected_generation: 1,
                    actual_generation: 0,
                }
            ))
        );
    }

    #[test]
    fn missing_region_and_empty_context_fail_closed() {
        let planner = planner();
        let incomplete = SemanticElasticBridge::new(
            vec![SemanticWeightRegion {
                page_id: PageId(0),
                routing_signature: SemanticBitField::<8>::zeroed().expect("routing"),
                value_signature: SemanticBitField::<8>::zeroed().expect("value"),
            }],
            SemanticElasticPolicy {
                max_selected_pages: 1,
                min_combined_similarity_ppm: 0,
            },
        )
        .expect("bridge");
        let state = SemanticState::<8>::zeroed().expect("state");

        assert_eq!(
            incomplete.plan(&state, &planner, 1),
            Err(SemanticElasticError::MissingRegion(PageId(1)))
        );

        let complete = bridge();
        assert!(matches!(
            complete.plan(&state, &planner, 1),
            Err(SemanticElasticError::Metric(_))
        ));
    }
}
