//! Native SBG R-field dirty-word producer for optional event-driven routing.
//!
//! Unlike repeated full-snapshot comparison, point mutations here identify
//! changed packed words *at write time*. This adapter is opt-in: it does not
//! replace SemanticState routing or the authoritative native MOR decision.
//!
//! Only writes through this wrapper are tracked. Replacing a whole R field
//! must scan its words once to identify differences; callers must not infer
//! that every SML state change already produces a dirty-word notice.

use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use core::fmt;

use super::{SemanticBitField, SemanticBitFieldError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SbgDirtyRoutingWord {
    pub index: usize,
    pub next: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SbgDirtyJournalError<E> {
    NonIncreasingTick { previous: u64, next: u64 },
    Sink(E),
}

impl<E: fmt::Debug> fmt::Display for SbgDirtyJournalError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl<E: std::error::Error + 'static> std::error::Error for SbgDirtyJournalError<E> {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SbgJournalEvidence {
    pub tick: u64,
    /// Changed words delivered, distinct from bits/fronts produced.
    pub dirty_words: usize,
    /// Total packed words in the field, provided only for comparison.
    pub total_words: usize,
}

/// A self-contained model-owned source of exact delta notices for SBG R.
///
/// The ordered map stores each changed word's value at the *last successful
/// acknowledgement*. If subsequent writes undo a change, that word is removed
/// from the pending set. It does not copy or scan the whole bitfield for
/// point mutations. The no-delta case visits no packed field words at flush.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SbgRoutingDirtyJournal<const BITS: usize> {
    routing: SemanticBitField<BITS>,
    /// Original packed values before the first modification since ack.
    original_words: BTreeMap<usize, u64>,
    last_tick: u64,
}

impl<const BITS: usize> SbgRoutingDirtyJournal<BITS> {
    #[must_use]
    pub fn new(routing: SemanticBitField<BITS>, baseline_tick: u64) -> Self {
        Self {
            routing,
            original_words: BTreeMap::new(),
            last_tick: baseline_tick,
        }
    }

    #[must_use]
    pub fn routing(&self) -> &SemanticBitField<BITS> {
        &self.routing
    }

    #[must_use]
    pub const fn last_tick(&self) -> u64 {
        self.last_tick
    }

    #[must_use]
    pub fn pending_word_count(&self) -> usize {
        self.original_words.len()
    }

    /// Write through the native single-coordinate bit mutation path.
    ///
    /// A successful no-op is not a dirty event. Invalid coordinates do not
    /// mutate the field, journal or clock. Returning true means the packed
    /// field actually changed at this operation, not that a 0->1 edge
    /// has necessarily occurred relative to the prior acknowledged cycle.
    pub fn set(&mut self, index: usize, enabled: bool) -> Result<bool, SemanticBitFieldError> {
        let word_index = index / 64;
        let old = self.routing.words().get(word_index).copied().unwrap_or(0);
        self.routing.set(index, enabled)?;
        let next = self.routing.words()[word_index];
        self.record_change(word_index, old, next);
        Ok(old != next)
    }

    /// Replace the entire routing field. This explicitly scans all packed
    /// words because whole-field replacement cannot enumerate changed words
    /// without reading both versions. No false O(number-of-events) claim.
    pub fn replace(&mut self, next: SemanticBitField<BITS>) -> usize {
        let mut changed = 0;
        for (index, (&old, &new)) in self
            .routing
            .words()
            .iter()
            .zip(next.words())
            .enumerate()
        {
            if old != new {
                changed += 1;
            }
            // Recording is deferred until after iteration; see below.
            let _ = index;
        }
        // Inspect once for exact pending baseline handling. The point
        // mutation path is the intended sparse path; replacement is dense.
        for index in 0..self.routing.words().len() {
            self.record_change(index, self.routing.words()[index], next.words()[index]);
        }
        self.routing = next;
        changed
    }

    /// Deliver a sorted, unique, owned delta slice to a caller-provided sink.
    /// Only a successful sink acknowledgement clears pending old values.
    ///
    /// The sink may convert each value to FLAT's neutral DirtyFeatureWord
    /// schema, or record model-native evidence without a FLAT dependency.
    /// If it fails, the journal remains available for a retried flush.
    pub fn flush<E, F>(
        &mut self,
        tick: u64,
        sink: F,
    ) -> Result<SbgJournalEvidence, SbgDirtyJournalError<E>>
    where
        F: FnOnce(&[SbgDirtyRoutingWord]) -> Result<(), E>,
    {
        if tick <= self.last_tick {
            return Err(SbgDirtyJournalError::NonIncreasingTick {
                previous: self.last_tick,
                next: tick,
            });
        }
        let mut delta = Vec::with_capacity(self.original_words.len());
        for &index in self.original_words.keys() {
            delta.push(SbgDirtyRoutingWord {
                index,
                next: self.routing.words()[index],
            });
        }
        sink(&delta).map_err(SbgDirtyJournalError::Sink)?;
        self.original_words.clear();
        self.last_tick = tick;
        Ok(SbgJournalEvidence {
            tick,
            dirty_words: delta.len(),
            total_words: self.routing.words().len(),
        })
    }

    fn record_change(&mut self, index: usize, old: u64, next: u64) {
        match self.original_words.entry(index) {
            Entry::Vacant(entry) => {
                if old != next {
                    entry.insert(old);
                }
            }
            Entry::Occupied(entry) => {
                if *entry.get() == next {
                    entry.remove();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_writes_emit_only_changed_words_and_collapse_reversions() {
        let initial = SemanticBitField::<130>::zeroed().unwrap();
        let mut journal = SbgRoutingDirtyJournal::new(initial, 0);
        journal.set(0, true).unwrap();
        journal.set(1, true).unwrap();
        journal.set(64, true).unwrap();
        journal.set(129, true).unwrap();
        journal.set(1, false).unwrap();
        assert_eq!(journal.pending_word_count(), 3);
        let mut captured = Vec::new();
        let counts = journal
            .flush(1, |words| {
                captured.extend_from_slice(words);
                Ok::<(), &'static str>(())
            })
            .unwrap();
        assert_eq!(captured, vec![
            SbgDirtyRoutingWord { index: 0, next: 1 },
            SbgDirtyRoutingWord { index: 1, next: 1 },
            SbgDirtyRoutingWord { index: 2, next: 2 },
        ]);
        assert_eq!(counts.dirty_words, 3);
        assert_eq!(counts.total_words, 3);
        assert_eq!(journal.pending_word_count(), 0);
    }

    #[test]
    fn undo_before_flush_produces_no_delta() {
        let mut journal = SbgRoutingDirtyJournal::new(SemanticBitField::<65>::zeroed().unwrap(), 0);
        assert!(journal.set(64, true).unwrap());
        assert!(journal.set(64, false).unwrap());
        assert_eq!(journal.pending_word_count(), 0);
        let record = journal.flush(1, |_| Ok::<(), ()>(())).unwrap();
        assert_eq!(record.dirty_words, 0);
        assert_eq!(record.total_words, 2);
    }

    #[test]
    fn sink_failure_keeps_pending_deltas_and_tick_unchanged() {
        let mut journal = SbgRoutingDirtyJournal::new(SemanticBitField::<1>::zeroed().unwrap(), 7);
        journal.set(0, true).unwrap();
        let snapshot = journal.clone();
        assert_eq!(
            journal.flush(8, |_| Err::<(), _>("unavailable")),
            Err(SbgDirtyJournalError::Sink("unavailable"))
        );
        assert_eq!(journal, snapshot);
        assert_eq!(
            journal.flush(7, |_| Ok::<(), ()>(())),
            Err(SbgDirtyJournalError::NonIncreasingTick {
                previous: 7,
                next: 7,
            })
        );
        let mut value = 0;
        journal.flush(8, |words| {
            value = words[0].next;
            Ok::<(), ()>(())
        }).unwrap();
        assert_eq!(value, 1);
        assert_eq!(journal.pending_word_count(), 0);
    }

    #[test]
    fn replacing_field_has_explicit_dense_scan_semantics() {
        let mut journal = SbgRoutingDirtyJournal::new(SemanticBitField::<70>::zeroed().unwrap(), 0);
        assert_eq!(journal.replace(SemanticBitField::<70>::ones().unwrap()), 2);
        let mut output = Vec::new();
        journal.flush(1, |words| {
            output.extend_from_slice(words);
            Ok::<(), ()>(())
        }).unwrap();
        assert_eq!(output, vec![
            SbgDirtyRoutingWord { index: 0, next: u64::MAX },
            SbgDirtyRoutingWord { index: 1, next: 0b11_1111 },
        ]);
        assert_eq!(journal.replace(SemanticBitField::<70>::zeroed().unwrap()), 2);
        journal.flush(2, |words| {
            assert_eq!(words.len(), 2);
            Ok::<(), ()>(())
        }).unwrap();
    }

    #[test]
    fn invalid_index_does_not_change_journal() {
        let mut journal = SbgRoutingDirtyJournal::new(SemanticBitField::<1>::zeroed().unwrap(), 0);
        let before = journal.clone();
        assert_eq!(
            journal.set(1, true),
            Err(SemanticBitFieldError::IndexOutOfRange {
                index: 1,
                width: 1,
            })
        );
        assert_eq!(journal, before);
    }
}
