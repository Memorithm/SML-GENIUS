//! Immutable evidence records for recurrent-depth execution.

use core::fmt;

use crate::{
    BooleanWorkspace, ExecutionCounters, HaltingPolicy, PageId, ReasoningKernel, RecurrentOutcome,
    RunError,
};

/// Immutable record for one internal reasoning cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CycleTraceRecord {
    pub token_index: u64,
    pub depth: u32,
    pub selected_pages: Vec<PageId>,
    pub active_parameters: u64,
    pub active_storage_bits: u128,
    pub workspace_hamming: u64,
    pub contradiction: bool,
    pub pairwise_comparisons: u64,
    pub attention_score_ops: u64,
}

/// All recurrent cycles executed for one emitted-token position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecurrentTrace {
    token_index: u64,
    records: Vec<CycleTraceRecord>,
}

impl RecurrentTrace {
    /// Constructs a trace from immutable records and validates its sequence.
    pub fn new(
        token_index: u64,
        records: Vec<CycleTraceRecord>,
    ) -> Result<Self, TraceValidationError> {
        let trace = Self {
            token_index,
            records,
        };
        trace.validate_structure()?;
        Ok(trace)
    }

    /// Token position represented by this trace.
    #[must_use]
    pub const fn token_index(&self) -> u64 {
        self.token_index
    }

    /// Ordered cycle records.
    #[must_use]
    pub fn records(&self) -> &[CycleTraceRecord] {
        &self.records
    }

    /// Number of internal cycles.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Returns true when no cycle is recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Validates token identity and contiguous 1-based depth.
    pub fn validate_structure(&self) -> Result<(), TraceValidationError> {
        if self.records.is_empty() {
            return Err(TraceValidationError::EmptyTrace);
        }

        for (index, record) in self.records.iter().enumerate() {
            if record.token_index != self.token_index {
                return Err(TraceValidationError::TokenIndex {
                    expected: self.token_index,
                    actual: record.token_index,
                });
            }
            let expected_depth = index as u32 + 1;
            if record.depth != expected_depth {
                return Err(TraceValidationError::Depth {
                    expected: expected_depth,
                    actual: record.depth,
                });
            }
        }
        Ok(())
    }

    /// Fails closed if a candidate trace reports prohibited attention-like work.
    pub fn validate_non_attention(&self) -> Result<(), TraceValidationError> {
        self.validate_structure()?;
        for record in &self.records {
            if record.pairwise_comparisons != 0 {
                return Err(TraceValidationError::PairwiseComparisons {
                    depth: record.depth,
                    count: record.pairwise_comparisons,
                });
            }
            if record.attention_score_ops != 0 {
                return Err(TraceValidationError::AttentionScoreOperations {
                    depth: record.depth,
                    count: record.attention_score_ops,
                });
            }
        }
        Ok(())
    }
}

/// Recurrent result plus its complete per-cycle trace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TracedRecurrentOutcome {
    pub outcome: RecurrentOutcome,
    pub trace: RecurrentTrace,
}

/// Executes adaptive recurrent depth while retaining one immutable record per cycle.
pub fn run_recurrent_traced<K: ReasoningKernel>(
    initial: BooleanWorkspace,
    kernel: &mut K,
    policy: HaltingPolicy,
    token_index: u64,
) -> Result<TracedRecurrentOutcome, RunError<K::Error>> {
    let policy = policy.validate().map_err(RunError::Runner)?;
    let mut previous = initial;
    let mut counters = ExecutionCounters::default();
    let mut records = Vec::with_capacity(policy.max_depth as usize);

    for depth in 1..=policy.max_depth {
        let step = kernel.step(&previous, depth).map_err(RunError::Kernel)?;
        let distance = previous
            .hamming_distance(&step.workspace)
            .map_err(|error| RunError::Runner(error.into()))?;

        counters.recurrent_cycles = depth;
        counters.active_parameters = counters
            .active_parameters
            .saturating_add(step.active_parameters);
        counters.active_storage_bits = counters
            .active_storage_bits
            .saturating_add(step.active_storage_bits);
        counters.pairwise_comparisons = counters
            .pairwise_comparisons
            .saturating_add(step.pairwise_comparisons);
        counters.attention_score_ops = counters
            .attention_score_ops
            .saturating_add(step.attention_score_ops);

        records.push(CycleTraceRecord {
            token_index,
            depth,
            selected_pages: step.selected_pages.clone(),
            active_parameters: step.active_parameters,
            active_storage_bits: step.active_storage_bits,
            workspace_hamming: distance,
            contradiction: step.contradiction,
            pairwise_comparisons: step.pairwise_comparisons,
            attention_score_ops: step.attention_score_ops,
        });

        let stable = distance <= policy.stable_hamming_threshold;
        let can_halt = depth >= policy.min_depth && stable && !step.contradiction;
        let must_halt = depth == policy.max_depth;

        previous = step.workspace;
        if can_halt || must_halt {
            let trace = RecurrentTrace {
                token_index,
                records,
            };
            return Ok(TracedRecurrentOutcome {
                outcome: RecurrentOutcome {
                    workspace: previous,
                    depth,
                    counters,
                },
                trace,
            });
        }
    }

    unreachable!("validated max_depth is non-zero")
}

/// Invalid recurrent-trace evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceValidationError {
    EmptyTrace,
    TokenIndex { expected: u64, actual: u64 },
    Depth { expected: u32, actual: u32 },
    PairwiseComparisons { depth: u32, count: u64 },
    AttentionScoreOperations { depth: u32, count: u64 },
}

impl fmt::Display for TraceValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for TraceValidationError {}
