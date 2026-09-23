//! SML-D1d: MOR-gated sparse decision expert bank.
//!
//! Every record is routed through the existing native SBG->MOR bridge first.
//! This slice requires exactly one selected expert per record. All routes are
//! validated before any decision expert executes; records are then grouped by
//! page so each selected expert retains the existing bit-sliced batch path.

use core::fmt;
use std::collections::BTreeMap;

use sml_core::{
    mor::{ExpertRuntimeHint, MixtureOfRouters, PreviousRoute},
    mor_registry::MorRouterRegistry,
    semantic::{SemanticMorBridge, SemanticMorError, SemanticMorTopK},
    PageId,
};

use crate::{
    semantic::{
        ProjectionBudget, SemanticDecisionError, SemanticDecisionProgram, SemanticInput,
    },
    BatchBudget, DecisionRecord,
};

#[derive(Debug, Clone)]
pub struct DecisionExpert<const BITS: usize> {
    pub page_id: PageId,
    pub program: SemanticDecisionProgram<BITS>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SparseDecisionBudget {
    pub max_records: usize,
    pub max_expert_batches: usize,
    pub max_total_results: usize,
    pub max_total_working_payload_bytes: usize,
}

impl Default for SparseDecisionBudget {
    fn default() -> Self {
        Self {
            max_records: 4096,
            max_expert_batches: 64,
            max_total_results: 65_536,
            max_total_working_payload_bytes: 16 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectMorRouteEvidence {
    pub record_id: u64,
    pub selected_page: PageId,
    pub active_routers: usize,
    pub ranked_experts: usize,
    pub noise_ppm: u32,
    pub structural_disagreement_ppm: u32,
    pub request_precision_escalation: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpertBatchEvidence {
    pub page_id: PageId,
    pub records: usize,
    pub decisions: usize,
    pub logical_gate_evaluations: u64,
    pub word_gate_evaluations: u64,
    pub feature_queries: u64,
    pub observation_mask_reads: u64,
    pub semantic_bit_reads: u64,
    pub working_payload_bytes_bound: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutedDecisionRecord {
    pub expert_page: PageId,
    pub decision: DecisionRecord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SparseDecisionBatchResult {
    /// Input order, then the selected expert's declared head order.
    pub decisions: Vec<RoutedDecisionRecord>,
    /// One compact routing record per input, in input order.
    pub routes: Vec<DirectMorRouteEvidence>,
    /// One entry per expert that actually executed.
    pub expert_batches: Vec<ExpertBatchEvidence>,
    pub routing_calls: u64,
    pub total_working_payload_bytes_bound: usize,
    pub text_tokens_generated: u64,
}

#[derive(Debug, Clone)]
pub struct MorDecisionExpertBank<const BITS: usize> {
    bridge: SemanticMorBridge<BITS>,
    registry: MorRouterRegistry,
    engine: MixtureOfRouters,
    experts: BTreeMap<u32, SemanticDecisionProgram<BITS>>,
    schema_id: u64,
    projection_version: u64,
}

impl<const BITS: usize> MorDecisionExpertBank<BITS> {
    pub fn new(
        bridge: SemanticMorBridge<BITS>,
        registry: MorRouterRegistry,
        engine: MixtureOfRouters,
        experts: Vec<DecisionExpert<BITS>>,
    ) -> Result<Self, SparseDecisionError> {
        if bridge.proposal_top_k() != SemanticMorTopK::One {
            return Err(SparseDecisionError::NonSingletonProposalFanout);
        }

        let expert_count = registry.policy().expert_count as usize;
        if experts.len() != expert_count || experts.is_empty() {
            return Err(SparseDecisionError::ExpertCoverage);
        }
        if bridge.expert_signature_count() != expert_count
            || bridge.router_signature_count() != registry.specializations().len()
        {
            return Err(SparseDecisionError::BridgeCoverage);
        }

        let first = experts
            .first()
            .ok_or(SparseDecisionError::ExpertCoverage)?;
        let schema_id = first.program.schema_id();
        let projection_version = first.program.projection_version();

        let mut indexed = BTreeMap::new();
        for expert in experts {
            if expert.page_id.0 >= registry.policy().expert_count {
                return Err(SparseDecisionError::ExpertOutOfRange(expert.page_id));
            }
            if expert.program.schema_id() != schema_id
                || expert.program.projection_version() != projection_version
            {
                return Err(SparseDecisionError::ProgramBindingMismatch(expert.page_id));
            }
            if indexed.insert(expert.page_id.0, expert.program).is_some() {
                return Err(SparseDecisionError::DuplicateExpert(expert.page_id));
            }
        }

        for page in 0..registry.policy().expert_count {
            if !indexed.contains_key(&page) {
                return Err(SparseDecisionError::MissingExpert(PageId(page)));
            }
        }

        Ok(Self {
            bridge,
            registry,
            engine,
            experts: indexed,
            schema_id,
            projection_version,
        })
    }

    #[must_use]
    pub fn expert_count(&self) -> usize {
        self.experts.len()
    }

    #[must_use]
    pub const fn schema_id(&self) -> u64 {
        self.schema_id
    }

    #[must_use]
    pub const fn projection_version(&self) -> u64 {
        self.projection_version
    }

    pub fn evaluate(
        &self,
        inputs: &[SemanticInput<'_, BITS>],
        runtime_hints: &[ExpertRuntimeHint],
        decision_budget: BatchBudget,
        projection_budget: ProjectionBudget,
        sparse_budget: SparseDecisionBudget,
    ) -> Result<SparseDecisionBatchResult, SparseDecisionError> {
        if inputs.len() > sparse_budget.max_records {
            return Err(SparseDecisionError::BatchBudget);
        }
        if inputs.is_empty() {
            return Ok(SparseDecisionBatchResult {
                decisions: Vec::new(),
                routes: Vec::new(),
                expert_batches: Vec::new(),
                routing_calls: 0,
                total_working_payload_bytes_bound: 0,
                text_tokens_generated: 0,
            });
        }

        for input in inputs {
            if input.schema_id != self.schema_id
                || input.projection_version != self.projection_version
            {
                return Err(SparseDecisionError::InputBindingMismatch(input.record_id));
            }
            if input.observed.routing.count_ones() != BITS as u64 {
                return Err(SparseDecisionError::RoutingNotFullyObserved(
                    input.record_id,
                ));
            }
        }

        // Route every record first. No decision program executes before this loop
        // has established one valid page for every input.
        let mut routes = Vec::new();
        routes
            .try_reserve_exact(inputs.len())
            .map_err(|_| SparseDecisionError::Allocation)?;
        let mut groups = BTreeMap::<u32, Vec<usize>>::new();
        let mut head_counts = Vec::new();
        head_counts
            .try_reserve_exact(inputs.len())
            .map_err(|_| SparseDecisionError::Allocation)?;

        for (index, input) in inputs.iter().enumerate() {
            let routed = self.bridge.route(
                input.state,
                &self.registry,
                &self.engine,
                runtime_hints,
                &PreviousRoute::default(),
            )?;
            if routed.decision.selected_experts.len() != 1 {
                return Err(SparseDecisionError::NonSingletonSelection {
                    record_id: input.record_id,
                    selected: routed.decision.selected_experts.len(),
                });
            }
            let selected_page = routed.decision.selected_experts[0];
            let program = self
                .experts
                .get(&selected_page.0)
                .ok_or(SparseDecisionError::MissingExpert(selected_page))?;
            groups.entry(selected_page.0).or_default().push(index);
            head_counts.push(program.head_count());
            routes.push(DirectMorRouteEvidence {
                record_id: input.record_id,
                selected_page,
                active_routers: routed.decision.active_routers.len(),
                ranked_experts: routed.decision.ranked_experts.len(),
                noise_ppm: routed.decision.noise_ppm,
                structural_disagreement_ppm: routed.decision.structural_disagreement_ppm,
                request_precision_escalation: routed.decision.request_precision_escalation,
            });
        }

        if groups.len() > sparse_budget.max_expert_batches {
            return Err(SparseDecisionError::BatchBudget);
        }

        let mut offsets = Vec::new();
        offsets
            .try_reserve_exact(inputs.len() + 1)
            .map_err(|_| SparseDecisionError::Allocation)?;
        offsets.push(0_usize);
        for heads in &head_counts {
            let next = offsets
                .last()
                .copied()
                .ok_or(SparseDecisionError::InternalInvariant)?
                .checked_add(*heads)
                .ok_or(SparseDecisionError::Overflow)?;
            offsets.push(next);
        }
        let total_results = *offsets
            .last()
            .ok_or(SparseDecisionError::InternalInvariant)?;
        if total_results > sparse_budget.max_total_results {
            return Err(SparseDecisionError::BatchBudget);
        }

        // Pre-plan every selected expert batch and aggregate the conservative
        // payload bounds before the first expert executes.
        let mut total_working_payload_bytes_bound = 0_usize;
        for (page, indexes) in &groups {
            let program = self
                .experts
                .get(page)
                .ok_or(SparseDecisionError::MissingExpert(PageId(*page)))?;
            let plan = program.plan(indexes.len(), decision_budget, projection_budget)?;
            total_working_payload_bytes_bound = total_working_payload_bytes_bound
                .checked_add(plan.working_payload_bytes_bound)
                .ok_or(SparseDecisionError::Overflow)?;
        }
        if total_working_payload_bytes_bound > sparse_budget.max_total_working_payload_bytes {
            return Err(SparseDecisionError::BatchBudget);
        }

        let mut ordered = vec![None; total_results];
        let mut expert_batches = Vec::new();
        expert_batches
            .try_reserve_exact(groups.len())
            .map_err(|_| SparseDecisionError::Allocation)?;
        let mut text_tokens_generated = 0_u64;

        for (page, indexes) in groups {
            let page_id = PageId(page);
            let program = self
                .experts
                .get(&page)
                .ok_or(SparseDecisionError::MissingExpert(page_id))?;
            let mut batch_inputs = Vec::new();
            batch_inputs
                .try_reserve_exact(indexes.len())
                .map_err(|_| SparseDecisionError::Allocation)?;
            for index in &indexes {
                batch_inputs.push(inputs[*index]);
            }

            let output = program.evaluate(&batch_inputs, decision_budget, projection_budget)?;
            let heads = program.head_count();
            if output.decisions.decisions.len()
                != indexes
                    .len()
                    .checked_mul(heads)
                    .ok_or(SparseDecisionError::Overflow)?
            {
                return Err(SparseDecisionError::InternalInvariant);
            }

            for (group_position, input_index) in indexes.iter().copied().enumerate() {
                let start = group_position
                    .checked_mul(heads)
                    .ok_or(SparseDecisionError::Overflow)?;
                for head in 0..heads {
                    let decision = output
                        .decisions
                        .decisions
                        .get(start + head)
                        .copied()
                        .ok_or(SparseDecisionError::InternalInvariant)?;
                    let destination = offsets[input_index]
                        .checked_add(head)
                        .ok_or(SparseDecisionError::Overflow)?;
                    let slot = ordered
                        .get_mut(destination)
                        .ok_or(SparseDecisionError::InternalInvariant)?;
                    if slot
                        .replace(RoutedDecisionRecord {
                            expert_page: page_id,
                            decision,
                        })
                        .is_some()
                    {
                        return Err(SparseDecisionError::InternalInvariant);
                    }
                }
            }

            text_tokens_generated = text_tokens_generated
                .checked_add(output.decisions.text_tokens_generated)
                .ok_or(SparseDecisionError::Overflow)?;
            expert_batches.push(ExpertBatchEvidence {
                page_id,
                records: indexes.len(),
                decisions: output.decisions.decisions.len(),
                logical_gate_evaluations: output.decisions.plan.logical_gate_evaluations,
                word_gate_evaluations: output.decisions.plan.word_gate_evaluations,
                feature_queries: output.projection.feature_queries,
                observation_mask_reads: output.projection.observation_mask_reads,
                semantic_bit_reads: output.projection.semantic_bit_reads,
                working_payload_bytes_bound: output.working_payload_bytes_bound,
            });
        }

        let decisions = ordered
            .into_iter()
            .map(|item| item.ok_or(SparseDecisionError::InternalInvariant))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(SparseDecisionBatchResult {
            decisions,
            routes,
            expert_batches,
            routing_calls: u64::try_from(inputs.len()).map_err(|_| SparseDecisionError::Overflow)?,
            total_working_payload_bytes_bound,
            text_tokens_generated,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SparseDecisionError {
    ExpertCoverage,
    BridgeCoverage,
    NonSingletonProposalFanout,
    ExpertOutOfRange(PageId),
    DuplicateExpert(PageId),
    MissingExpert(PageId),
    ProgramBindingMismatch(PageId),
    InputBindingMismatch(u64),
    RoutingNotFullyObserved(u64),
    NonSingletonSelection { record_id: u64, selected: usize },
    BatchBudget,
    Overflow,
    Allocation,
    InternalInvariant,
    Mor(SemanticMorError),
    Decision(SemanticDecisionError),
}

impl From<SemanticMorError> for SparseDecisionError {
    fn from(error: SemanticMorError) -> Self {
        Self::Mor(error)
    }
}

impl From<SemanticDecisionError> for SparseDecisionError {
    fn from(error: SemanticDecisionError) -> Self {
        Self::Decision(error)
    }
}

impl fmt::Display for SparseDecisionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SparseDecisionError {}
