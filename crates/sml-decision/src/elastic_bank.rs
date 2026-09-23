//! SML-D1e: native Elastic Weight admission for MOR-selected decision pages.
//!
//! MOR remains the sole expert selector. The Elastic Weight planner receives the
//! union of pages already selected by MOR and validates precision, residency,
//! active-parameter, RAM and VRAM budgets before any decision expert executes.
//! This is control-plane PLAN/VALIDATE only: no physical weight materialization
//! and no implicit planner commit occur in this slice.

use core::fmt;
use std::collections::BTreeSet;

use sml_core::{
    elastic_weights::{ElasticWeightError, ElasticWeightPlan, ElasticWeightPlanner},
    mor::ExpertRuntimeHint,
    PageId,
};

use crate::{
    mor_bank::{
        MorDecisionExpertBank, SparseDecisionBatchResult, SparseDecisionBudget, SparseDecisionError,
    },
    semantic::{ProjectionBudget, SemanticInput},
    BatchBudget,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElasticAdmissionEvidence {
    pub planner_generation: u64,
    pub selected_pages: usize,
    pub active_parameters: u64,
    pub active_storage_bits: u128,
    pub ram_bytes: u64,
    pub vram_bytes: u64,
    pub bytes_moved_to_ram: u64,
    pub bytes_moved_to_vram: u64,
    pub precision_changes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElasticSparseDecisionBatchResult {
    pub decisions: SparseDecisionBatchResult,
    /// None only for an empty input batch, for which no expert working set exists.
    pub elastic_plan: Option<ElasticWeightPlan>,
    pub admission: Option<ElasticAdmissionEvidence>,
}

#[derive(Debug, Clone)]
pub struct ElasticMorDecisionBank<const BITS: usize> {
    decision_bank: MorDecisionExpertBank<BITS>,
    planner: ElasticWeightPlanner,
}

impl<const BITS: usize> ElasticMorDecisionBank<BITS> {
    pub fn new(
        decision_bank: MorDecisionExpertBank<BITS>,
        planner: ElasticWeightPlanner,
    ) -> Result<Self, ElasticAdmissionError> {
        let expert_pages = decision_bank
            .expert_pages()
            .into_iter()
            .map(|page| page.0)
            .collect::<BTreeSet<_>>();
        let planner_pages = planner
            .pages()
            .iter()
            .map(|page| page.page_id.0)
            .collect::<BTreeSet<_>>();
        if expert_pages != planner_pages {
            return Err(ElasticAdmissionError::PageInventoryMismatch);
        }
        Ok(Self {
            decision_bank,
            planner,
        })
    }

    #[must_use]
    pub fn planner(&self) -> &ElasticWeightPlanner {
        &self.planner
    }

    #[must_use]
    pub fn decision_bank(&self) -> &MorDecisionExpertBank<BITS> {
        &self.decision_bank
    }

    pub fn evaluate(
        &self,
        inputs: &[SemanticInput<'_, BITS>],
        runtime_hints: &[ExpertRuntimeHint],
        epoch: u64,
        decision_budget: BatchBudget,
        projection_budget: ProjectionBudget,
        sparse_budget: SparseDecisionBudget,
    ) -> Result<ElasticSparseDecisionBatchResult, ElasticAdmissionError> {
        let route_plan = self.decision_bank.plan_sparse(
            inputs,
            runtime_hints,
            decision_budget,
            projection_budget,
            sparse_budget,
        )?;

        if route_plan.is_empty() {
            let decisions = self.decision_bank.execute_sparse(
                inputs,
                &route_plan,
                decision_budget,
                projection_budget,
            )?;
            return Ok(ElasticSparseDecisionBatchResult {
                decisions,
                elastic_plan: None,
                admission: None,
            });
        }

        // MOR is authoritative for expert selection. Elastic receives exactly the
        // unique MOR-selected page set and may reject it on resource/policy grounds.
        let selected_pages = route_plan.selected_pages();
        let elastic_plan = self.planner.plan_cycle(&selected_pages, epoch)?;
        elastic_plan.validate_contract_v1()?;
        validate_active_set(&selected_pages, &elastic_plan)?;

        // Expert execution happens only after the complete Elastic plan validates.
        let decisions = self.decision_bank.execute_sparse(
            inputs,
            &route_plan,
            decision_budget,
            projection_budget,
        )?;

        let admission = ElasticAdmissionEvidence {
            planner_generation: elastic_plan.base_generation,
            selected_pages: selected_pages.len(),
            active_parameters: elastic_plan.active_parameters,
            active_storage_bits: elastic_plan.active_storage_bits,
            ram_bytes: elastic_plan.ram_bytes,
            vram_bytes: elastic_plan.vram_bytes,
            bytes_moved_to_ram: elastic_plan.bytes_moved_to_ram,
            bytes_moved_to_vram: elastic_plan.bytes_moved_to_vram,
            precision_changes: elastic_plan.precision_changes,
        };

        Ok(ElasticSparseDecisionBatchResult {
            decisions,
            elastic_plan: Some(elastic_plan),
            admission: Some(admission),
        })
    }
}

fn validate_active_set(
    selected_pages: &[PageId],
    plan: &ElasticWeightPlan,
) -> Result<(), ElasticAdmissionError> {
    let expected = selected_pages
        .iter()
        .map(|page| page.0)
        .collect::<BTreeSet<_>>();
    let actual = plan
        .transitions
        .iter()
        .filter(|transition| transition.to_active)
        .map(|transition| transition.page_id.0)
        .collect::<BTreeSet<_>>();
    if expected != actual {
        return Err(ElasticAdmissionError::PlanSelectionMismatch);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElasticAdmissionError {
    PageInventoryMismatch,
    PlanSelectionMismatch,
    Sparse(SparseDecisionError),
    Elastic(ElasticWeightError),
}

impl From<SparseDecisionError> for ElasticAdmissionError {
    fn from(error: SparseDecisionError) -> Self {
        Self::Sparse(error)
    }
}

impl From<ElasticWeightError> for ElasticAdmissionError {
    fn from(error: ElasticWeightError) -> Self {
        Self::Elastic(error)
    }
}

impl fmt::Display for ElasticAdmissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ElasticAdmissionError {}
