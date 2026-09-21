//! First integrated SML-1 task candidate pilot.
//!
//! Event routing is learned and selected hard micro-operations can be injected
//! from task-loss training. Associative-memory semantics remain fixed reference
//! code. Held-out execution therefore validates bounded learned mechanisms, not
//! general language-model capability.

use core::fmt;

use crate::{
    audit::{validate_non_attention_candidate, CandidateAuditError},
    memory::{BoundedAssociativeMemory, MemoryError, ReadOutcome, ReplacementPolicy, WriteOutcome},
    routing_train::{
        LatentDirectRouter, RouteLearningAccounting, RouteLearningBudget, RouteLearningError,
        RouteTrainingConfig, RouteTrainingReport,
    },
    tasks::{TaskAnswer, TaskEvent, TaskInstance, TaskKind},
    train::TruthTable2,
    BooleanWorkspace, DirectRouteTable, ExecutionCounters, PageId, RouteError,
};

const EVENT_ADDRESS_WIDTH: u8 = 4;
const MICRO_OP_PAGE_COUNT: u32 = 10;
const ROUTE_ENTRY_COUNT: usize = 1 << EVENT_ADDRESS_WIDTH;

const PAGE_MARKER: u32 = 0;
const PAGE_QUERY_KEY: u32 = 1;
const PAGE_FACT: u32 = 2;
const PAGE_QUERY_COPY: u32 = 3;
const PAGE_QUERY_RECALL: u32 = 4;
const PAGE_STORE_BIT: u32 = 5;
const PAGE_QUERY_CONJUNCTION: u32 = 6;
const PAGE_PAYLOAD_BIT: u32 = 7;
const PAGE_DISTRACTOR: u32 = 8;
const PAGE_NOOP: u32 = 9;

/// Structural and optimization limits for the first integrated task candidate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TaskCandidateConfig {
    pub memory_capacity: usize,
    pub max_payload_bits: usize,
    pub route_budget: RouteLearningBudget,
    pub route_training: RouteTrainingConfig,
}

impl Default for TaskCandidateConfig {
    fn default() -> Self {
        Self {
            memory_capacity: 65_536,
            max_payload_bits: 4_096,
            route_budget: RouteLearningBudget {
                max_address_width: EVENT_ADDRESS_WIDTH,
                max_pages: MICRO_OP_PAGE_COUNT,
                max_learned_bits: 64,
            },
            route_training: RouteTrainingConfig::default(),
        }
    }
}

/// Training evidence for the learned event router.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TaskCandidateTrainingReport {
    pub route: RouteTrainingReport,
    pub route_accounting: RouteLearningAccounting,
}

/// Immutable per-event evidence from one task execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskTraceRecord {
    pub event_index: usize,
    pub event: TaskEvent,
    pub route_address: u8,
    pub selected_page: PageId,
    pub emitted_answer: Option<TaskAnswer>,
    pub counters: ExecutionCounters,
}

/// Complete evidence for one deterministic task instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskExecutionEvidence {
    pub kind: TaskKind,
    pub case_id: u64,
    pub expected: TaskAnswer,
    pub predicted: TaskAnswer,
    pub exact: bool,
    pub semantic_route_bits_touched: u64,
    pub trace: Vec<TaskTraceRecord>,
    pub counters: ExecutionCounters,
}

/// Frozen candidate with learned direct event routing and fixed bounded micro-operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmlTaskCandidate {
    router: DirectRouteTable,
    route_accounting: RouteLearningAccounting,
    memory_capacity: usize,
    max_payload_bits: usize,
    conjunction_table: TruthTable2,
    state_update_table: TruthTable2,
}

impl SmlTaskCandidate {
    /// Trains the non-trivial event-to-page permutation and freezes it to hard routing.
    pub fn train(
        config: TaskCandidateConfig,
    ) -> Result<(Self, TaskCandidateTrainingReport), TaskCandidateError> {
        Self::train_with_micro_ops(config, TruthTable2::AND, TruthTable2::RIGHT)
    }

    /// Trains event routing while injecting an explicit hard conjunction micro-operation.
    ///
    /// This constructor exists so later task-loss experiments and fixed-gate
    /// ablations can reuse the identical router, memory, and evidence path.
    pub fn train_with_conjunction(
        config: TaskCandidateConfig,
        conjunction_table: TruthTable2,
    ) -> Result<(Self, TaskCandidateTrainingReport), TaskCandidateError> {
        Self::train_with_micro_ops(config, conjunction_table, TruthTable2::RIGHT)
    }

    /// Trains event routing while injecting both currently learnable hard micro-operations.
    ///
    /// The state-update table receives (previous_state, incoming_value). The
    /// reference overwrite semantics are therefore the RIGHT projection.
    pub fn train_with_micro_ops(
        config: TaskCandidateConfig,
        conjunction_table: TruthTable2,
        state_update_table: TruthTable2,
    ) -> Result<(Self, TaskCandidateTrainingReport), TaskCandidateError> {
        if config.memory_capacity == 0 {
            return Err(TaskCandidateError::ZeroMemoryCapacity);
        }
        if config.max_payload_bits == 0 {
            return Err(TaskCandidateError::ZeroPayloadBudget);
        }

        let mut router = LatentDirectRouter::new(
            EVENT_ADDRESS_WIDTH,
            MICRO_OP_PAGE_COUNT,
            config.route_budget,
        )?;
        let oracle = routing_oracle();
        let report = router.train_complete(&oracle, config.route_training)?;
        if report.hard_correct != ROUTE_ENTRY_COUNT as u64 {
            return Err(TaskCandidateError::RouteNotConverged {
                correct: report.hard_correct,
                total: ROUTE_ENTRY_COUNT as u64,
            });
        }
        let route_accounting = router.accounting();
        let frozen = router.freeze()?;

        Ok((
            Self {
                router: frozen,
                route_accounting,
                memory_capacity: config.memory_capacity,
                max_payload_bits: config.max_payload_bits,
                conjunction_table,
                state_update_table,
            },
            TaskCandidateTrainingReport {
                route: report,
                route_accounting,
            },
        ))
    }

    /// Hard conjunction table used by the current frozen candidate.
    #[must_use]
    pub const fn conjunction_table(&self) -> TruthTable2 {
        self.conjunction_table
    }

    /// Hard state-update table used by the current frozen candidate.
    #[must_use]
    pub const fn state_update_table(&self) -> TruthTable2 {
        self.state_update_table
    }

    /// Executes one task through the frozen learned router and retains every event record.
    pub fn execute(
        &self,
        instance: &TaskInstance,
    ) -> Result<TaskExecutionEvidence, TaskCandidateError> {
        let mut memory = BoundedAssociativeMemory::new(
            self.memory_capacity,
            1,
            ReplacementPolicy::RejectCollision,
        )?;
        let mut counters = ExecutionCounters::default();
        let mut stored_bit = None;
        let mut copy_active = false;
        let mut payload = Vec::new();
        let mut predicted = None;
        let mut trace = Vec::with_capacity(instance.events.len());

        for (event_index, event) in instance.events.iter().copied().enumerate() {
            let address = event_address(event);
            let address_bits = [
                address & 1 != 0,
                address & 2 != 0,
                address & 4 != 0,
                address & 8 != 0,
            ];
            let workspace = BooleanWorkspace::from_bits(&address_bits);
            let page = self.router.route(&workspace, &mut counters)?;
            let emitted = self.dispatch(
                page,
                event,
                &mut memory,
                &mut counters,
                &mut stored_bit,
                &mut copy_active,
                &mut payload,
            )?;

            if let Some(answer) = emitted.clone() {
                if predicted.replace(answer).is_some() {
                    return Err(TaskCandidateError::MultiplePredictions);
                }
            }

            trace.push(TaskTraceRecord {
                event_index,
                event,
                route_address: address,
                selected_page: page,
                emitted_answer: emitted,
                counters,
            });
        }

        validate_non_attention_candidate(&counters)?;
        let predicted = predicted.ok_or(TaskCandidateError::MissingPrediction)?;
        let semantic_route_bits_touched =
            (trace.len() as u64).saturating_mul(self.route_accounting.bits_per_route as u64);

        Ok(TaskExecutionEvidence {
            kind: instance.kind,
            case_id: instance.case_id,
            expected: instance.expected.clone(),
            exact: predicted == instance.expected,
            predicted,
            semantic_route_bits_touched,
            trace,
            counters,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn dispatch(
        &self,
        page: PageId,
        event: TaskEvent,
        memory: &mut BoundedAssociativeMemory,
        counters: &mut ExecutionCounters,
        stored_bit: &mut Option<bool>,
        copy_active: &mut bool,
        payload: &mut Vec<bool>,
    ) -> Result<Option<TaskAnswer>, TaskCandidateError> {
        match (page.0, event) {
            (PAGE_STORE_BIT, TaskEvent::StoreBit(value)) => {
                let previous = stored_bit.unwrap_or(false);
                *stored_bit = Some(self.state_update_table.evaluate(previous, value));
                Ok(None)
            }
            (PAGE_FACT, TaskEvent::Fact { key, value }) => {
                let outcome =
                    memory.write(key as u64, BooleanWorkspace::from_bits(&[value]), counters)?;
                match outcome {
                    WriteOutcome::Inserted { .. } | WriteOutcome::Updated { .. } => Ok(None),
                    WriteOutcome::CollisionRejected { slot, resident_key } => {
                        Err(TaskCandidateError::MemoryCollision {
                            key,
                            slot,
                            resident_key,
                        })
                    }
                    WriteOutcome::Replaced { .. } => Err(TaskCandidateError::UnexpectedReplacement),
                }
            }
            (PAGE_DISTRACTOR, TaskEvent::Distractor(_)) => Ok(None),
            (PAGE_MARKER, TaskEvent::Marker) => {
                payload.clear();
                *copy_active = true;
                Ok(None)
            }
            (PAGE_PAYLOAD_BIT, TaskEvent::PayloadBit(value)) => {
                if !*copy_active {
                    return Err(TaskCandidateError::PayloadBeforeMarker);
                }
                if payload.len() >= self.max_payload_bits {
                    return Err(TaskCandidateError::PayloadBudget {
                        maximum: self.max_payload_bits,
                    });
                }
                payload.push(value);
                Ok(None)
            }
            (PAGE_QUERY_RECALL, TaskEvent::QueryRecall) => Ok(Some(TaskAnswer::Bit(
                stored_bit.ok_or(TaskCandidateError::MissingStoredBit)?,
            ))),
            (PAGE_QUERY_KEY, TaskEvent::QueryKey(key)) => {
                Ok(Some(TaskAnswer::Bit(read_fact(memory, key, counters)?)))
            }
            (PAGE_QUERY_CONJUNCTION, TaskEvent::QueryConjunction { left, right }) => {
                let left_value = read_fact(memory, left, counters)?;
                let right_value = read_fact(memory, right, counters)?;
                Ok(Some(TaskAnswer::Bit(
                    self.conjunction_table.evaluate(left_value, right_value),
                )))
            }
            (PAGE_QUERY_COPY, TaskEvent::QueryCopy) => {
                if !*copy_active || payload.is_empty() {
                    return Err(TaskCandidateError::MissingPayload);
                }
                Ok(Some(TaskAnswer::Bits(payload.clone())))
            }
            _ => Err(TaskCandidateError::EventPageMismatch { event, page }),
        }
    }
}

fn read_fact(
    memory: &BoundedAssociativeMemory,
    key: u16,
    counters: &mut ExecutionCounters,
) -> Result<bool, TaskCandidateError> {
    match memory.read(key as u64, counters) {
        ReadOutcome::Hit { value, .. } => {
            value.get(0).ok_or(TaskCandidateError::CorruptMemoryValue)
        }
        ReadOutcome::EmptyMiss { .. } | ReadOutcome::KeyMismatch { .. } => {
            Err(TaskCandidateError::MissingFact(key))
        }
    }
}

const fn event_address(event: TaskEvent) -> u8 {
    match event {
        TaskEvent::StoreBit(_) => 0,
        TaskEvent::Fact { .. } => 1,
        TaskEvent::Distractor(_) => 2,
        TaskEvent::Marker => 3,
        TaskEvent::PayloadBit(_) => 4,
        TaskEvent::QueryRecall => 5,
        TaskEvent::QueryKey(_) => 6,
        TaskEvent::QueryConjunction { .. } => 7,
        TaskEvent::QueryCopy => 8,
    }
}

fn routing_oracle() -> [PageId; ROUTE_ENTRY_COUNT] {
    [
        PageId(PAGE_STORE_BIT),
        PageId(PAGE_FACT),
        PageId(PAGE_DISTRACTOR),
        PageId(PAGE_MARKER),
        PageId(PAGE_PAYLOAD_BIT),
        PageId(PAGE_QUERY_RECALL),
        PageId(PAGE_QUERY_KEY),
        PageId(PAGE_QUERY_CONJUNCTION),
        PageId(PAGE_QUERY_COPY),
        PageId(PAGE_NOOP),
        PageId(PAGE_NOOP),
        PageId(PAGE_NOOP),
        PageId(PAGE_NOOP),
        PageId(PAGE_NOOP),
        PageId(PAGE_NOOP),
        PageId(PAGE_NOOP),
    ]
}

/// Candidate configuration, routing, memory, or execution failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskCandidateError {
    ZeroMemoryCapacity,
    ZeroPayloadBudget,
    RouteNotConverged {
        correct: u64,
        total: u64,
    },
    MultiplePredictions,
    MissingPrediction,
    MissingStoredBit,
    MissingFact(u16),
    MissingPayload,
    PayloadBeforeMarker,
    PayloadBudget {
        maximum: usize,
    },
    CorruptMemoryValue,
    MemoryCollision {
        key: u16,
        slot: usize,
        resident_key: u64,
    },
    UnexpectedReplacement,
    EventPageMismatch {
        event: TaskEvent,
        page: PageId,
    },
    RouteLearning(RouteLearningError),
    Route(RouteError),
    Memory(MemoryError),
    Audit(CandidateAuditError),
}

impl fmt::Display for TaskCandidateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for TaskCandidateError {}

impl From<RouteLearningError> for TaskCandidateError {
    fn from(value: RouteLearningError) -> Self {
        Self::RouteLearning(value)
    }
}

impl From<RouteError> for TaskCandidateError {
    fn from(value: RouteError) -> Self {
        Self::Route(value)
    }
}

impl From<MemoryError> for TaskCandidateError {
    fn from(value: MemoryError) -> Self {
        Self::Memory(value)
    }
}

impl From<CandidateAuditError> for TaskCandidateError {
    fn from(value: CandidateAuditError) -> Self {
        Self::Audit(value)
    }
}
