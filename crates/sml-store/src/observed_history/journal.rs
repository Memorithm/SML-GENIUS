use core::{fmt, mem::size_of};

/// Exact observed address. A compressed projection is not a subject identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SubjectKey {
    subject: u64,
    group: u32,
}

impl SubjectKey {
    pub fn new(subject: u64, group: u32) -> Result<Self, HistoryError> {
        if subject == 0 {
            return Err(HistoryError::InvalidIdentity);
        }
        Ok(Self { subject, group })
    }

    pub const fn subject(self) -> u64 {
        self.subject
    }

    pub const fn group(self) -> u32 {
        self.group
    }
}

/// An observation is a claim in an input record, not authenticated world truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservationEvent {
    pub(super) id: u64,
    pub(super) key: SubjectKey,
    pub(super) bits: [bool; 2],
    pub(super) predecessor: Option<u64>,
}

impl ObservationEvent {
    pub fn new(
        id: u64,
        key: SubjectKey,
        bits: [bool; 2],
        predecessor: Option<u64>,
    ) -> Result<Self, HistoryError> {
        if id == 0 || predecessor.is_some_and(|prior| prior == 0 || prior >= id) {
            return Err(HistoryError::InvalidIdentity);
        }
        Ok(Self {
            id,
            key,
            bits,
            predecessor,
        })
    }

    pub const fn id(self) -> u64 {
        self.id
    }

    pub const fn key(self) -> SubjectKey {
        self.key
    }

    pub const fn bits(self) -> [bool; 2] {
        self.bits
    }

    pub const fn predecessor(self) -> Option<u64> {
        self.predecessor
    }
}

/// Logical element limits, excluding allocation capacity, allocator and RSS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryLimits {
    pub max_events: usize,
    pub max_keys: usize,
    pub max_element_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StoredEvent {
    pub(super) event: ObservationEvent,
    pub(super) active: bool,
}

/// Append-only bounded reference log. Supersession deactivates but never erases.
///
/// Returned errors preserve logical events, active states and index. Allocation
/// capacity may change on allocation failure. Process-crash durability is absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationLog {
    pub(super) records: Vec<StoredEvent>,
    pub(super) index: Vec<usize>,
    limits: HistoryLimits,
    key_count: usize,
    element_plan: usize,
}

impl ObservationLog {
    pub fn new(limits: HistoryLimits) -> Result<Self, HistoryError> {
        if limits.max_events == 0 || limits.max_keys == 0 {
            return Err(HistoryError::InvalidLimits);
        }
        let per_event = size_of::<StoredEvent>()
            .checked_add(size_of::<usize>())
            .ok_or(HistoryError::Overflow)?;
        let plan = limits
            .max_events
            .checked_mul(per_event)
            .and_then(|n| n.checked_add(size_of::<Self>()))
            .ok_or(HistoryError::Overflow)?;
        if plan > limits.max_element_bytes {
            return Err(HistoryError::BudgetExceeded);
        }
        Ok(Self {
            records: Vec::new(),
            index: Vec::new(),
            limits,
            key_count: 0,
            element_plan: plan,
        })
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub const fn key_count(&self) -> usize {
        self.key_count
    }

    pub const fn max_element_bytes(&self) -> usize {
        self.element_plan
    }

    pub fn events(&self) -> impl ExactSizeIterator<Item = &ObservationEvent> {
        self.records.iter().map(|record| &record.event)
    }

    pub fn append(&mut self, event: ObservationEvent) -> Result<(), HistoryError> {
        if self.records.len() >= self.limits.max_events {
            return Err(HistoryError::BudgetExceeded);
        }
        if self.records.last().is_some_and(|r| r.event.id >= event.id) {
            return Err(HistoryError::NonIncreasingIdentity);
        }
        let new_key = !self.records.iter().any(|r| r.event.key == event.key);
        if new_key && self.key_count >= self.limits.max_keys {
            return Err(HistoryError::BudgetExceeded);
        }
        let predecessor = match event.predecessor {
            Some(id) => {
                let i = self
                    .records
                    .binary_search_by_key(&id, |r| r.event.id)
                    .map_err(|_| HistoryError::InvalidPredecessor)?;
                let previous = &self.records[i];
                if previous.event.key != event.key || !previous.active {
                    return Err(HistoryError::InvalidPredecessor);
                }
                Some(i)
            }
            None => None,
        };
        let position = self
            .index
            .partition_point(|&i| self.records[i].event.key <= event.key);
        self.records
            .try_reserve_exact(1)
            .map_err(|_| HistoryError::Allocation)?;
        self.index
            .try_reserve_exact(1)
            .map_err(|_| HistoryError::Allocation)?;
        // All fallible checks/allocation precede the logical commit.
        if let Some(i) = predecessor {
            self.records[i].active = false;
        }
        self.index.insert(position, self.records.len());
        self.records.push(StoredEvent {
            event,
            active: true,
        });
        self.key_count += usize::from(new_key);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryError {
    InvalidIdentity,
    InvalidLimits,
    NonIncreasingIdentity,
    InvalidPredecessor,
    BudgetExceeded,
    Overflow,
    Allocation,
}

impl fmt::Display for HistoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for HistoryError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(events: usize, keys: usize) -> ObservationLog {
        ObservationLog::new(HistoryLimits {
            max_events: events,
            max_keys: keys,
            max_element_bytes: 1 << 20,
        })
        .unwrap()
    }

    fn event(id: u64, subject: u64, prior: Option<u64>) -> ObservationEvent {
        ObservationEvent::new(id, SubjectKey::new(subject, 0).unwrap(), [true, false], prior)
            .unwrap()
    }

    #[test]
    fn identities_reject_zero_future_and_self_predecessors() {
        assert_eq!(SubjectKey::new(0, 0), Err(HistoryError::InvalidIdentity));
        let key = SubjectKey::new(1, 0).unwrap();
        for (id, prior) in [(0, None), (1, Some(0)), (1, Some(1)), (1, Some(2))] {
            assert!(ObservationEvent::new(id, key, [false; 2], prior).is_err());
        }
        assert_ne!(key, SubjectKey::new(1, 1).unwrap());
    }

    #[test]
    fn append_preserves_every_event_and_exact_index_order() {
        let mut history = log(4, 2);
        history.append(event(1, 2, None)).unwrap();
        history.append(event(3, 1, None)).unwrap();
        history.append(event(5, 2, Some(1))).unwrap();
        assert_eq!(history.len(), 3);
        assert_eq!(history.key_count(), 2);
        assert_eq!(history.index, [1, 0, 2]);
        assert!(!history.records[0].active);
        assert!(history.records[1].active && history.records[2].active);
        assert_eq!(history.events().map(|e| e.id()).collect::<Vec<_>>(), [1, 3, 5]);
    }

    #[test]
    fn invalid_predecessors_and_duplicates_leave_history_unchanged() {
        let mut history = log(10, 10);
        history.append(event(1, 1, None)).unwrap();
        history.append(event(2, 1, Some(1))).unwrap();
        for e in [event(2, 1, None), event(3, 2, Some(2)), event(3, 1, Some(1))] {
            let before = history.clone();
            assert!(history.append(e).is_err());
            assert_eq!(history, before);
        }
        let before = history.clone();
        assert!(history.append(event(9, 1, Some(8))).is_err());
        assert_eq!(history, before);
    }

    #[test]
    fn limits_reject_before_logical_mutation() {
        let mut history = log(2, 1);
        history.append(event(1, 1, None)).unwrap();
        let before = history.clone();
        assert_eq!(history.append(event(2, 2, None)), Err(HistoryError::BudgetExceeded));
        assert_eq!(history, before);
        history.append(event(2, 1, Some(1))).unwrap();
        let before = history.clone();
        assert!(history.append(event(3, 1, Some(2))).is_err());
        assert_eq!(history, before);
    }

    #[test]
    fn zero_overflow_and_element_budgets_reject() {
        for (events, keys, bytes) in [(0, 1, 9999), (1, 0, 9999), (1, 1, 0)] {
            assert!(ObservationLog::new(HistoryLimits {
                max_events: events,
                max_keys: keys,
                max_element_bytes: bytes,
            })
            .is_err());
        }
        assert!(ObservationLog::new(HistoryLimits {
            max_events: usize::MAX,
            max_keys: 1,
            max_element_bytes: usize::MAX,
        })
        .is_err());
        assert!(log(3, 3).max_element_bytes() >= 3 * size_of::<ObservationEvent>());
    }
}
