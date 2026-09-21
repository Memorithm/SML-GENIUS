//! Deterministic bounded tasks for the SML-1 development programme.
//!
//! These tasks provide complete symbolic answers. They are development
//! fixtures, not language-model benchmarks and not confirmatory evidence.

use core::fmt;

/// Initial bounded task families.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    DelayedBitRecall,
    KeyedFactRecall,
    CopyAfterMarker,
    TwoFactConjunction,
    DistractorRejection,
}

/// Typed sequence event consumed by a future SML-1 evaluator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskEvent {
    StoreBit(bool),
    Fact { key: u16, value: bool },
    Distractor(u16),
    Marker,
    PayloadBit(bool),
    QueryRecall,
    QueryKey(u16),
    QueryConjunction { left: u16, right: u16 },
    QueryCopy,
}

/// Complete deterministic answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskAnswer {
    Bit(bool),
    Bits(Vec<bool>),
}

/// One generated task instance with its complete expected answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskInstance {
    pub kind: TaskKind,
    pub case_id: u64,
    pub events: Vec<TaskEvent>,
    pub expected: TaskAnswer,
}

impl TaskInstance {
    /// Recomputes the answer from the event stream.
    pub fn solve(&self) -> Result<TaskAnswer, TaskError> {
        solve_task(self)
    }

    /// Returns whether the embedded answer matches the independent oracle.
    pub fn oracle_matches(&self) -> Result<bool, TaskError> {
        Ok(self.solve()? == self.expected)
    }
}

/// Delayed recall of one stored bit through unrelated events.
#[must_use]
pub fn delayed_bit_recall(case_id: u64, delay: usize) -> TaskInstance {
    let value = deterministic_bit(case_id, 0);
    let mut events = Vec::with_capacity(delay + 2);
    events.push(TaskEvent::StoreBit(value));
    for index in 0..delay {
        events.push(TaskEvent::Distractor(index as u16));
    }
    events.push(TaskEvent::QueryRecall);

    TaskInstance {
        kind: TaskKind::DelayedBitRecall,
        case_id,
        events,
        expected: TaskAnswer::Bit(value),
    }
}

/// Direct keyed Boolean fact recall with unrelated keyed facts.
pub fn keyed_fact_recall(case_id: u64, distractors: usize) -> Result<TaskInstance, TaskError> {
    validate_distractor_count(distractors)?;
    let target_key = (case_id % 16_384) as u16;
    let target_value = deterministic_bit(case_id, 1);

    let mut events = Vec::with_capacity(distractors + 2);
    events.push(TaskEvent::Fact {
        key: target_key,
        value: target_value,
    });
    for index in 0..distractors {
        events.push(TaskEvent::Fact {
            key: 16_384 + index as u16,
            value: deterministic_bit(case_id, index as u64 + 2),
        });
    }
    events.push(TaskEvent::QueryKey(target_key));

    Ok(TaskInstance {
        kind: TaskKind::KeyedFactRecall,
        case_id,
        events,
        expected: TaskAnswer::Bit(target_value),
    })
}

/// Copies the exact payload observed after a marker.
pub fn copy_after_marker(case_id: u64, payload_len: usize) -> Result<TaskInstance, TaskError> {
    if payload_len == 0 || payload_len > 4_096 {
        return Err(TaskError::InvalidPayloadLength(payload_len));
    }

    let payload: Vec<bool> = (0..payload_len)
        .map(|index| deterministic_bit(case_id, index as u64 + 17))
        .collect();

    let mut events = Vec::with_capacity(payload_len + 3);
    events.push(TaskEvent::Distractor(0));
    events.push(TaskEvent::Marker);
    for bit in &payload {
        events.push(TaskEvent::PayloadBit(*bit));
    }
    events.push(TaskEvent::QueryCopy);

    Ok(TaskInstance {
        kind: TaskKind::CopyAfterMarker,
        case_id,
        events,
        expected: TaskAnswer::Bits(payload),
    })
}

/// Retrieves two facts and returns their Boolean conjunction.
pub fn two_fact_conjunction(case_id: u64, distractors: usize) -> Result<TaskInstance, TaskError> {
    validate_distractor_count(distractors)?;
    let left_key = (case_id % 8_192) as u16;
    let right_key = 8_192 + (case_id % 8_192) as u16;
    let left_value = deterministic_bit(case_id, 31);
    let right_value = deterministic_bit(case_id, 32);

    let mut events = Vec::with_capacity(distractors + 3);
    events.push(TaskEvent::Fact {
        key: left_key,
        value: left_value,
    });
    events.push(TaskEvent::Fact {
        key: right_key,
        value: right_value,
    });
    for index in 0..distractors {
        events.push(TaskEvent::Fact {
            key: 16_384 + index as u16,
            value: deterministic_bit(case_id, index as u64 + 33),
        });
    }
    events.push(TaskEvent::QueryConjunction {
        left: left_key,
        right: right_key,
    });

    Ok(TaskInstance {
        kind: TaskKind::TwoFactConjunction,
        case_id,
        events,
        expected: TaskAnswer::Bit(left_value & right_value),
    })
}

/// Recalls a target fact while irrelevant facts and non-keyed distractors intervene.
pub fn distractor_rejection(case_id: u64, distractors: usize) -> Result<TaskInstance, TaskError> {
    validate_distractor_count(distractors)?;
    let target_key = (case_id % 8_192) as u16;
    let target_value = deterministic_bit(case_id, 97);

    let mut events = Vec::with_capacity(distractors.saturating_mul(2) + 2);
    events.push(TaskEvent::Fact {
        key: target_key,
        value: target_value,
    });
    for index in 0..distractors {
        events.push(TaskEvent::Distractor(index as u16));
        events.push(TaskEvent::Fact {
            key: 16_384 + index as u16,
            value: deterministic_bit(case_id, index as u64 + 98),
        });
    }
    events.push(TaskEvent::QueryKey(target_key));

    Ok(TaskInstance {
        kind: TaskKind::DistractorRejection,
        case_id,
        events,
        expected: TaskAnswer::Bit(target_value),
    })
}

/// Builds a deterministic panel spanning every initial task family from a declared case ID.
pub fn suite_from_case_id(
    start_case_id: u64,
    cases_per_kind: u32,
) -> Result<Vec<TaskInstance>, TaskError> {
    let end_case_id = start_case_id
        .checked_add(cases_per_kind as u64)
        .ok_or(TaskError::CaseIdOverflow)?;
    let mut suite = Vec::with_capacity((cases_per_kind as usize).saturating_mul(5));
    for case_id in start_case_id..end_case_id {
        let span = (case_id % 16) as usize;
        suite.push(delayed_bit_recall(case_id, span));
        suite.push(keyed_fact_recall(case_id, span)?);
        suite.push(copy_after_marker(case_id, 1 + (case_id % 8) as usize)?);
        suite.push(two_fact_conjunction(case_id, span)?);
        suite.push(distractor_rejection(case_id, span)?);
    }
    Ok(suite)
}

/// Builds the canonical deterministic development panel from case ID zero.
pub fn development_suite(cases_per_kind: u32) -> Result<Vec<TaskInstance>, TaskError> {
    suite_from_case_id(0, cases_per_kind)
}

/// Independent complete-answer oracle.
pub fn solve_task(instance: &TaskInstance) -> Result<TaskAnswer, TaskError> {
    match instance.kind {
        TaskKind::DelayedBitRecall => {
            let query = find_event(&instance.events, |event| {
                matches!(event, TaskEvent::QueryRecall)
            })
            .ok_or(TaskError::MissingQuery)?;
            let value = instance.events[..query]
                .iter()
                .rev()
                .find_map(|event| match event {
                    TaskEvent::StoreBit(value) => Some(*value),
                    _ => None,
                })
                .ok_or(TaskError::MissingStoredBit)?;
            Ok(TaskAnswer::Bit(value))
        }
        TaskKind::KeyedFactRecall | TaskKind::DistractorRejection => {
            let query = find_query_key(&instance.events)?;
            let value = find_last_fact(&instance.events, query.0, query.1)?;
            Ok(TaskAnswer::Bit(value))
        }
        TaskKind::CopyAfterMarker => {
            let query = find_event(&instance.events, |event| {
                matches!(event, TaskEvent::QueryCopy)
            })
            .ok_or(TaskError::MissingQuery)?;
            let marker = instance.events[..query]
                .iter()
                .rposition(|event| matches!(event, TaskEvent::Marker))
                .ok_or(TaskError::MissingMarker)?;
            let payload: Vec<bool> = instance.events[marker + 1..query]
                .iter()
                .filter_map(|event| match event {
                    TaskEvent::PayloadBit(value) => Some(*value),
                    _ => None,
                })
                .collect();
            if payload.is_empty() {
                return Err(TaskError::MissingPayload);
            }
            Ok(TaskAnswer::Bits(payload))
        }
        TaskKind::TwoFactConjunction => {
            let (left, right, query_index) = find_conjunction_query(&instance.events)?;
            let left_value = find_last_fact(&instance.events, left, query_index)?;
            let right_value = find_last_fact(&instance.events, right, query_index)?;
            Ok(TaskAnswer::Bit(left_value & right_value))
        }
    }
}

fn find_event(events: &[TaskEvent], predicate: impl Fn(&TaskEvent) -> bool) -> Option<usize> {
    events.iter().position(predicate)
}

fn find_query_key(events: &[TaskEvent]) -> Result<(u16, usize), TaskError> {
    events
        .iter()
        .enumerate()
        .find_map(|(index, event)| match event {
            TaskEvent::QueryKey(key) => Some((*key, index)),
            _ => None,
        })
        .ok_or(TaskError::MissingQuery)
}

fn find_conjunction_query(events: &[TaskEvent]) -> Result<(u16, u16, usize), TaskError> {
    events
        .iter()
        .enumerate()
        .find_map(|(index, event)| match event {
            TaskEvent::QueryConjunction { left, right } => Some((*left, *right, index)),
            _ => None,
        })
        .ok_or(TaskError::MissingQuery)
}

fn find_last_fact(events: &[TaskEvent], key: u16, before: usize) -> Result<bool, TaskError> {
    events[..before]
        .iter()
        .rev()
        .find_map(|event| match event {
            TaskEvent::Fact {
                key: resident_key,
                value,
            } if *resident_key == key => Some(*value),
            _ => None,
        })
        .ok_or(TaskError::MissingFact(key))
}

fn validate_distractor_count(distractors: usize) -> Result<(), TaskError> {
    if distractors > 16_383 {
        return Err(TaskError::TooManyDistractors(distractors));
    }
    Ok(())
}

fn deterministic_bit(case_id: u64, salt: u64) -> bool {
    mix64(case_id ^ salt.wrapping_mul(0x9e37_79b9_7f4a_7c15)) & 1 != 0
}

fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

/// Invalid generated or hand-authored task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskError {
    CaseIdOverflow,
    InvalidPayloadLength(usize),
    TooManyDistractors(usize),
    MissingQuery,
    MissingStoredBit,
    MissingMarker,
    MissingPayload,
    MissingFact(u16),
}

impl fmt::Display for TaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for TaskError {}
