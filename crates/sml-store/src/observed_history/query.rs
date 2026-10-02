use super::{ObservationEvent, ObservationLog, SubjectKey};

/// Agreement and absence are relative to the admitted log, not world truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservedState {
    Missing,
    Agreement,
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistorySummary {
    pub key: SubjectKey,
    pub generation: usize,
    pub matched_records: usize,
    pub active_records: usize,
    pub state: ObservedState,
    /// Present only when all active observed values agree; not a model answer.
    pub value: Option<[bool; 2]>,
}

/// Immutable owner-bound query; neither numeric magnitude nor hashes are used.
///
/// ```compile_fail
/// use sml_store::observed_history::{HistoryLimits, ObservationEvent, ObservationLog, SubjectKey};
/// let mut log = ObservationLog::new(HistoryLimits {
///     max_events: 8, max_keys: 8, max_element_bytes: 4096,
/// }).unwrap();
/// let key = SubjectKey::new(1, 0).unwrap();
/// let query = log.query(key);
/// log.append(ObservationEvent::new(1, key, [false; 2], None).unwrap()).unwrap();
/// assert_eq!(query.summary().generation, 0);
/// ```
#[derive(Debug)]
pub struct HistoryQuery<'a> {
    owner: &'a ObservationLog,
    indices: &'a [usize],
    key: SubjectKey,
}

impl ObservationLog {
    /// O(log(events)) selection plus O(matching events) summary; no allocation.
    pub fn query(&self, key: SubjectKey) -> HistoryQuery<'_> {
        let first = self
            .index
            .partition_point(|&i| self.records[i].event.key < key);
        let last = self
            .index
            .partition_point(|&i| self.records[i].event.key <= key);
        HistoryQuery {
            owner: self,
            indices: &self.index[first..last],
            key,
        }
    }
}

impl HistoryQuery<'_> {
    pub fn key(&self) -> SubjectKey {
        self.key
    }

    /// Every version, including superseded versions, remains available in order.
    pub fn events(&self) -> impl ExactSizeIterator<Item = &ObservationEvent> {
        self.indices.iter().map(|&i| &self.owner.records[i].event)
    }

    pub fn summary(&self) -> HistorySummary {
        let mut active_records = 0;
        let mut value = None;
        let mut conflict = false;
        for &i in self.indices {
            let record = &self.owner.records[i];
            if !record.active {
                continue;
            }
            active_records += 1;
            match value {
                Some(previous) => conflict |= previous != record.event.bits,
                None => value = Some(record.event.bits),
            }
        }
        let state = if active_records == 0 {
            ObservedState::Missing
        } else if conflict {
            ObservedState::Conflict
        } else {
            ObservedState::Agreement
        };
        HistorySummary {
            key: self.key,
            generation: self.owner.len(),
            matched_records: self.indices.len(),
            active_records,
            state,
            value: if conflict { None } else { value },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observed_history::HistoryLimits;

    fn log() -> ObservationLog {
        ObservationLog::new(HistoryLimits {
            max_events: 16,
            max_keys: 8,
            max_element_bytes: 8192,
        })
        .unwrap()
    }

    fn key(subject: u64, group: u32) -> SubjectKey {
        SubjectKey::new(subject, group).unwrap()
    }

    fn add(log: &mut ObservationLog, id: u64, k: SubjectKey, bits: [bool; 2]) {
        let event = ObservationEvent::new(id, k, bits, None).unwrap();
        log.append(event).unwrap();
    }

    #[test]
    fn missing_is_not_false_and_is_scoped_to_exact_group() {
        let mut history = log();
        add(&mut history, 1, key(1, 0), [false; 2]);
        let present = history.query(key(1, 0)).summary();
        let absent = history.query(key(1, 1)).summary();
        assert_eq!(present.state, ObservedState::Agreement);
        assert_eq!(present.value, Some([false; 2]));
        assert_eq!(absent.state, ObservedState::Missing);
        assert_eq!(absent.value, None);
        assert_eq!(absent.generation, 1);
    }

    #[test]
    fn every_two_bit_pair_has_an_independent_conflict_or_agreement() {
        let values = [[false, false], [false, true], [true, false], [true, true]];
        for a in values {
            for b in values {
                let mut history = log();
                add(&mut history, 1, key(1, 0), a);
                add(&mut history, 2, key(1, 0), b);
                let summary = history.query(key(1, 0)).summary();
                let state = if a == b {
                    ObservedState::Agreement
                } else {
                    ObservedState::Conflict
                };
                assert_eq!(summary.state, state);
                assert_eq!(summary.value, if a == b { Some(a) } else { None });
                assert_eq!(summary.active_records, 2);
            }
        }
    }

    #[test]
    fn interleaved_keys_retain_only_matching_ordered_versions() {
        let mut history = log();
        add(&mut history, 2, key(2, 0), [false; 2]);
        add(&mut history, 4, key(1, 0), [true; 2]);
        add(&mut history, 6, key(1, 1), [false; 2]);
        add(&mut history, 8, key(1, 0), [true; 2]);
        let query = history.query(key(1, 0));
        let ids = query.events().map(|e| e.id()).collect::<Vec<_>>();
        assert_eq!(ids, [4, 8]);
        assert_eq!(query.key(), key(1, 0));
        assert_eq!(query.summary().matched_records, 2);
        assert_eq!(query.summary().generation, 4);
    }

    #[test]
    fn supersession_resolves_only_the_named_active_predecessor() {
        let mut history = log();
        let k = key(1, 0);
        add(&mut history, 1, k, [false; 2]);
        add(&mut history, 2, k, [true; 2]);
        assert_eq!(history.query(k).summary().state, ObservedState::Conflict);
        let replacement = ObservationEvent::new(3, k, [false; 2], Some(2)).unwrap();
        history.append(replacement).unwrap();
        let summary = history.query(k).summary();
        assert_eq!(summary.state, ObservedState::Agreement);
        assert_eq!(summary.active_records, 2);
        assert_eq!(summary.matched_records, 3);
        assert_eq!(summary.value, Some([false; 2]));
    }

    #[test]
    fn downstream_coincidence_cannot_hide_source_conflict() {
        let mut history = log();
        // Both pairs have the same XOR; they remain different observations.
        add(&mut history, 1, key(1, 0), [false, true]);
        add(&mut history, 2, key(1, 0), [true, false]);
        let summary = history.query(key(1, 0)).summary();
        assert_eq!(summary.state, ObservedState::Conflict);
        assert_eq!(summary.value, None);
    }
}
