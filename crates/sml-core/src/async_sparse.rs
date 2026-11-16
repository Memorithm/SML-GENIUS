//! Model-owned, bounded event agenda and sparse SBG dirty-word ingress.
//!
//! Research-only reference semantics. Polling a logical event is NOT proof of
//! parallel/asynchronous hardware execution or permission to skip native MOR.
//! The caller must preserve non-R invalidations and authoritative model work.

use std::collections::BTreeMap;

use crate::{
    semantic::{rae_dirty_journal::SbgDirtyRoutingWord, SemanticBitField},
    PageId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SparseEventKind {
    Rising,
    Falling,
    ContextInvalidation,
    MemoryInvalidation,
    Wake,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SparseEventTarget {
    Feature(usize),
    Expert(PageId),
    Memory(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SparseEventRequest {
    pub logical_time: u64,
    pub target: SparseEventTarget,
    pub kind: SparseEventKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SparseEvent {
    pub logical_time: u64,
    pub sequence: u64,
    pub target: SparseEventTarget,
    pub kind: SparseEventKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SparseAgendaError {
    ZeroCapacity,
    QueueFull {
        capacity: usize,
        pending: usize,
        incoming: usize,
    },
    RetrogradeTime { last_committed: u64, proposed: u64 },
    SequenceExhausted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SparseDispatchError<E> {
    Handler(E),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SparseAgendaEvidence {
    pub submitted: u64,
    pub committed: u64,
    pub peak_pending: usize,
}

/// Logical-time agenda: one locally ready event is processed per poll.
/// No fixed full-network tick, dense scan, external executor or global barrier.
///
/// Ordered (time, submission-sequence) keys give a deterministic single-thread
/// reference, not a claim that model inference is already asynchronous.
/// A failed handler leaves the event available for retry. Handlers themselves
/// must guarantee rollback/no side effects on failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncSparseAgenda {
    pending: BTreeMap<(u64, u64), SparseEvent>,
    capacity: usize,
    next_sequence: u64,
    last_committed_time: Option<u64>,
    evidence: SparseAgendaEvidence,
}

impl AsyncSparseAgenda {
    pub fn new(capacity: usize) -> Result<Self, SparseAgendaError> {
        if capacity == 0 {
            return Err(SparseAgendaError::ZeroCapacity);
        }
        Ok(Self {
            pending: BTreeMap::new(),
            capacity,
            next_sequence: 0,
            last_committed_time: None,
            evidence: SparseAgendaEvidence::default(),
        })
    }

    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    #[must_use]
    pub const fn evidence(&self) -> SparseAgendaEvidence {
        self.evidence
    }

    #[must_use]
    pub const fn last_committed_time(&self) -> Option<u64> {
        self.last_committed_time
    }

    #[must_use]
    pub fn peek(&self) -> Option<SparseEvent> {
        self.pending.first_key_value().map(|(_, event)| *event)
    }

    /// Admit a complete local burst or none of it. The source can retry after
    /// bounded backpressure; no event is silently dropped or truncated.
    pub fn submit_batch(
        &mut self,
        requests: &[SparseEventRequest],
    ) -> Result<(), SparseAgendaError> {
        if self
            .pending
            .len()
            .checked_add(requests.len())
            .is_none_or(|sum| sum > self.capacity)
        {
            return Err(SparseAgendaError::QueueFull {
                capacity: self.capacity,
                pending: self.pending.len(),
                incoming: requests.len(),
            });
        }
        let incoming =
            u64::try_from(requests.len()).map_err(|_| SparseAgendaError::SequenceExhausted)?;
        self.next_sequence
            .checked_add(incoming)
            .ok_or(SparseAgendaError::SequenceExhausted)?;
        if let Some(last) = self.last_committed_time {
            for request in requests {
                if request.logical_time < last {
                    return Err(SparseAgendaError::RetrogradeTime {
                        last_committed: last,
                        proposed: request.logical_time,
                    });
                }
            }
        }

        for request in requests {
            self.next_sequence += 1;
            let event = SparseEvent {
                logical_time: request.logical_time,
                sequence: self.next_sequence,
                target: request.target,
                kind: request.kind,
            };
            self.pending
                .insert((event.logical_time, event.sequence), event);
        }
        self.evidence.submitted += incoming;
        self.evidence.peak_pending = self.evidence.peak_pending.max(self.pending.len());
        Ok(())
    }

    /// Process exactly one event; an empty agenda does no model work.
    ///
    /// Application errors preserve queue and agenda time. The model's handler
    /// must itself be transactional, because this agenda cannot undo changes
    /// to arbitrary external memory or expert state.
    pub fn poll_next<E, F>(
        &mut self,
        handler: F,
    ) -> Result<Option<SparseEvent>, SparseDispatchError<E>>
    where
        F: FnOnce(SparseEvent) -> Result<(), E>,
    {
        let Some(event) = self.peek() else {
            return Ok(None);
        };
        handler(event).map_err(SparseDispatchError::Handler)?;
        self.pending.pop_first();
        self.last_committed_time = Some(event.logical_time);
        self.evidence.committed += 1;
        Ok(Some(event))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SparseIngressError {
    NonIncreasingTick { previous: u64, proposed: u64 },
    UnorderedWord { previous: usize, proposed: usize },
    WordOutOfRange { index: usize, words: usize },
    NonZeroPadding { index: usize },
    Agenda(SparseAgendaError),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SparseIngressEvidence {
    pub tick: u64,
    pub dirty_words: usize,
    pub rising: usize,
    pub falling: usize,
}

/// Exact R-field edges derived from only the words reported by REA-4.
/// The owned previous-word array is allocated once and never scanned on an
/// empty delta. The source assumes exclusive ownership of all R mutations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SbgSparseEventIngress<const BITS: usize> {
    previous_words: Vec<u64>,
    last_tick: u64,
}

impl<const BITS: usize> SbgSparseEventIngress<BITS> {
    #[must_use]
    pub fn new(baseline: &SemanticBitField<BITS>, baseline_tick: u64) -> Self {
        Self {
            previous_words: baseline.words().to_vec(),
            last_tick: baseline_tick,
        }
    }

    #[must_use]
    pub const fn last_tick(&self) -> u64 {
        self.last_tick
    }

    /// Compatible with SbgRoutingDirtyJournal::flush(tick, |deltas| ...).
    /// Invalid metadata, a non-monotonic tick or a full agenda leaves both
    /// prior feature state and queued events untouched.
    pub fn submit_dirty_words(
        &mut self,
        tick: u64,
        deltas: &[SbgDirtyRoutingWord],
        agenda: &mut AsyncSparseAgenda,
    ) -> Result<SparseIngressEvidence, SparseIngressError> {
        if tick <= self.last_tick {
            return Err(SparseIngressError::NonIncreasingTick {
                previous: self.last_tick,
                proposed: tick,
            });
        }
        let mut last_index = None;
        for delta in deltas {
            if delta.index >= self.previous_words.len() {
                return Err(SparseIngressError::WordOutOfRange {
                    index: delta.index,
                    words: self.previous_words.len(),
                });
            }
            if let Some(previous) = last_index {
                if delta.index <= previous {
                    return Err(SparseIngressError::UnorderedWord {
                        previous,
                        proposed: delta.index,
                    });
                }
            }
            if BITS % 64 != 0 && delta.index == self.previous_words.len() - 1 {
                let tail_mask = (1_u64 << (BITS % 64)) - 1;
                if delta.next & !tail_mask != 0 {
                    return Err(SparseIngressError::NonZeroPadding { index: delta.index });
                }
            }
            last_index = Some(delta.index);
        }

        let mut requests = Vec::new();
        let mut rising = 0;
        let mut falling = 0;
        for delta in deltas {
            let old = self.previous_words[delta.index];
            let mut rises = delta.next & !old;
            let mut falls = old & !delta.next;
            rising += rises.count_ones() as usize;
            falling += falls.count_ones() as usize;
            while rises != 0 {
                let bit = rises.trailing_zeros() as usize;
                requests.push(SparseEventRequest {
                    logical_time: tick,
                    target: SparseEventTarget::Feature(delta.index * 64 + bit),
                    kind: SparseEventKind::Rising,
                });
                rises &= rises - 1;
            }
            while falls != 0 {
                let bit = falls.trailing_zeros() as usize;
                requests.push(SparseEventRequest {
                    logical_time: tick,
                    target: SparseEventTarget::Feature(delta.index * 64 + bit),
                    kind: SparseEventKind::Falling,
                });
                falls &= falls - 1;
            }
        }
        agenda
            .submit_batch(&requests)
            .map_err(SparseIngressError::Agenda)?;
        for delta in deltas {
            self.previous_words[delta.index] = delta.next;
        }
        self.last_tick = tick;
        Ok(SparseIngressEvidence {
            tick,
            dirty_words: deltas.len(),
            rising,
            falling,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::rae_dirty_journal::{
        SbgDirtyJournalError, SbgRoutingDirtyJournal,
    };

    fn request(time: u64, feature: usize) -> SparseEventRequest {
        SparseEventRequest {
            logical_time: time,
            target: SparseEventTarget::Feature(feature),
            kind: SparseEventKind::Rising,
        }
    }

    #[test]
    fn idle_poll_does_not_dispatch_or_scan() {
        let mut agenda = AsyncSparseAgenda::new(2).unwrap();
        assert_eq!(agenda.poll_next::<(), _>(|_| panic!("no event")), Ok(None));
        assert_eq!(agenda.evidence(), SparseAgendaEvidence::default());
    }

    #[test]
    fn time_order_and_fifo_within_same_logical_time() {
        let mut agenda = AsyncSparseAgenda::new(4).unwrap();
        agenda
            .submit_batch(&[request(8, 1), request(5, 2), request(8, 3)])
            .unwrap();
        let mut order = Vec::new();
        for _ in 0..3 {
            agenda
                .poll_next(|event| {
                    order.push((event.logical_time, event.target));
                    Ok::<(), ()>(())
                })
                .unwrap();
        }
        assert_eq!(
            order,
            vec![
                (5, SparseEventTarget::Feature(2)),
                (8, SparseEventTarget::Feature(1)),
                (8, SparseEventTarget::Feature(3))
            ]
        );
        assert_eq!(agenda.evidence().committed, 3);
    }

    #[test]
    fn bounded_batch_backpressure_has_no_partial_admission() {
        let mut agenda = AsyncSparseAgenda::new(1).unwrap();
        let before = agenda.clone();
        assert_eq!(
            agenda.submit_batch(&[request(1, 0), request(1, 1)]),
            Err(SparseAgendaError::QueueFull {
                capacity: 1,
                pending: 0,
                incoming: 2
            })
        );
        assert_eq!(agenda, before);
    }

    #[test]
    fn callback_failure_retries_same_event_and_rejects_past_input() {
        let mut agenda = AsyncSparseAgenda::new(2).unwrap();
        agenda.submit_batch(&[request(7, 4)]).unwrap();
        let before = agenda.clone();
        assert_eq!(
            agenda.poll_next(|_| Err::<(), _>("failed")),
            Err(SparseDispatchError::Handler("failed"))
        );
        assert_eq!(agenda, before);
        agenda.poll_next(|_| Ok::<(), ()>(())).unwrap();
        assert_eq!(
            agenda.submit_batch(&[request(6, 5)]),
            Err(SparseAgendaError::RetrogradeTime {
                last_committed: 7,
                proposed: 6
            })
        );
    }

    #[test]
    fn journal_to_ingress_emits_only_net_edges_with_original_coordinates() {
        let baseline = SemanticBitField::<130>::zeroed().unwrap();
        let mut journal = SbgRoutingDirtyJournal::new(baseline.clone(), 0);
        let mut ingress = SbgSparseEventIngress::new(&baseline, 0);
        let mut agenda = AsyncSparseAgenda::new(4).unwrap();
        journal.set(64, true).unwrap();
        journal.set(129, true).unwrap();
        journal.set(0, true).unwrap();
        journal.set(0, false).unwrap();
        let mut evidence = SparseIngressEvidence::default();
        journal
            .flush(1, |deltas| {
                evidence = ingress.submit_dirty_words(1, deltas, &mut agenda)?;
                Ok::<(), SparseIngressError>(())
            })
            .unwrap();
        assert_eq!(evidence.rising, 2);
        assert_eq!(evidence.falling, 0);
        assert_eq!(evidence.dirty_words, 2);
        assert_eq!(agenda.pending_count(), 2);
        assert_eq!(
            agenda.poll_next(|_| Ok::<(), ()>(())).unwrap().unwrap().target,
            SparseEventTarget::Feature(64)
        );
        assert_eq!(
            agenda.poll_next(|_| Ok::<(), ()>(())).unwrap().unwrap().target,
            SparseEventTarget::Feature(129)
        );
        journal
            .flush(2, |deltas| ingress.submit_dirty_words(2, deltas, &mut agenda).map(|_| ()))
            .unwrap();
        assert_eq!(agenda.pending_count(), 0);
    }

    #[test]
    fn full_queue_preserves_journal_and_ingress_for_retry() {
        let baseline = SemanticBitField::<65>::zeroed().unwrap();
        let mut journal = SbgRoutingDirtyJournal::new(baseline.clone(), 0);
        let mut ingress = SbgSparseEventIngress::new(&baseline, 0);
        journal.set(0, true).unwrap();
        journal.set(64, true).unwrap();
        let mut small = AsyncSparseAgenda::new(1).unwrap();
        let old_ingress = ingress.clone();
        let old_journal = journal.clone();
        assert!(matches!(
            journal.flush(1, |deltas| ingress
                .submit_dirty_words(1, deltas, &mut small)
                .map(|_| ())),
            Err(SbgDirtyJournalError::Sink(SparseIngressError::Agenda(
                SparseAgendaError::QueueFull { .. }
            )))
        ));
        assert_eq!(ingress, old_ingress);
        assert_eq!(journal, old_journal);
        assert_eq!(small.pending_count(), 0);
        let mut enough = AsyncSparseAgenda::new(2).unwrap();
        journal
            .flush(1, |deltas| ingress.submit_dirty_words(1, deltas, &mut enough).map(|_| ()))
            .unwrap();
        assert_eq!(enough.pending_count(), 2);
    }

    #[test]
    fn invalid_padding_and_duplicate_words_fail_without_state_change() {
        let baseline = SemanticBitField::<65>::zeroed().unwrap();
        let mut ingress = SbgSparseEventIngress::new(&baseline, 0);
        let mut agenda = AsyncSparseAgenda::new(3).unwrap();
        let before = ingress.clone();
        assert_eq!(
            ingress.submit_dirty_words(
                1,
                &[SbgDirtyRoutingWord { index: 1, next: 2 }],
                &mut agenda
            ),
            Err(SparseIngressError::NonZeroPadding { index: 1 })
        );
        assert_eq!(
            ingress.submit_dirty_words(
                1,
                &[
                    SbgDirtyRoutingWord { index: 0, next: 1 },
                    SbgDirtyRoutingWord { index: 0, next: 1 }
                ],
                &mut agenda
            ),
            Err(SparseIngressError::UnorderedWord {
                previous: 0,
                proposed: 0
            })
        );
        assert_eq!(ingress, before);
        assert_eq!(agenda.pending_count(), 0);
    }

    #[test]
    fn explicit_non_r_invalidation_is_not_dropped_on_no_front() {
        let mut agenda = AsyncSparseAgenda::new(1).unwrap();
        agenda
            .submit_batch(&[SparseEventRequest {
                logical_time: 3,
                target: SparseEventTarget::Expert(PageId(7)),
                kind: SparseEventKind::MemoryInvalidation,
            }])
            .unwrap();
        let observed = agenda.poll_next(|_| Ok::<(), ()>(())).unwrap().unwrap();
        assert_eq!(observed.kind, SparseEventKind::MemoryInvalidation);
        assert_eq!(observed.target, SparseEventTarget::Expert(PageId(7)));
    }
}
