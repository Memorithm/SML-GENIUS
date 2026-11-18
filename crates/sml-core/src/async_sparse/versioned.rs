//! Optional causal source-generation guard for the bounded SML event agenda.
//!
//! Source generations are model-owned: a caller must advance a generation
//! whenever a previously queued event can no longer be causally authoritative.
//! This wrapper never authorizes skipping native MOR, recurrent work or memory
//! invalidations. Unlike a wall-clock TTL, a generation comparison is exact.

use std::collections::BTreeMap;

use super::{
    AsyncSparseAgenda, SparseAgendaError, SparseAgendaEvidence, SparseDispatchError, SparseEvent,
    SparseEventRequest,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SparseSourceStamp {
    pub source_id: u64,
    pub generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SparseGenerationError {
    UnknownSource {
        source_id: u64,
    },
    GenerationRegression {
        source_id: u64,
        current: u64,
        proposed: u64,
    },
    StaleSubmission {
        source_id: u64,
        current: u64,
        submitted: u64,
    },
    Agenda(SparseAgendaError),
}

/// Bounded, single-event logical-time agenda with explicit causal generations.
///
/// A submission is all-or-nothing. When a source advances, previously queued
/// events are retained in deterministic order but retired without invoking the
/// handler on the next poll; these retirements are counted separately from
/// committed work. An unrelated source is never invalidated by this advance.
///
/// No event is merged, and this is not proof of an asynchronous model runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionedSparseAgenda {
    agenda: AsyncSparseAgenda,
    generations: BTreeMap<u64, u64>,
    stamps: BTreeMap<u64, SparseSourceStamp>,
    stale_retired: u64,
}

impl VersionedSparseAgenda {
    pub fn new(capacity: usize) -> Result<Self, SparseAgendaError> {
        Ok(Self {
            agenda: AsyncSparseAgenda::new(capacity)?,
            generations: BTreeMap::new(),
            stamps: BTreeMap::new(),
            stale_retired: 0,
        })
    }

    /// Register or advance the model-owned generation of one event source.
    /// An advance does not modify other sources and never rewinds a generation.
    pub fn set_source_generation(
        &mut self,
        source_id: u64,
        proposed: u64,
    ) -> Result<(), SparseGenerationError> {
        if let Some(&current) = self.generations.get(&source_id) {
            if proposed < current {
                return Err(SparseGenerationError::GenerationRegression {
                    source_id,
                    current,
                    proposed,
                });
            }
        }
        self.generations.insert(source_id, proposed);
        Ok(())
    }

    /// Admit only events stamped with the currently registered generation.
    /// Neither stamps nor sequence numbers are changed if admission fails.
    pub fn submit_from(
        &mut self,
        stamp: SparseSourceStamp,
        requests: &[SparseEventRequest],
    ) -> Result<(), SparseGenerationError> {
        let current = *self.generations.get(&stamp.source_id).ok_or(
            SparseGenerationError::UnknownSource {
                source_id: stamp.source_id,
            },
        )?;
        if stamp.generation != current {
            return Err(SparseGenerationError::StaleSubmission {
                source_id: stamp.source_id,
                current,
                submitted: stamp.generation,
            });
        }

        let previous_sequence = self.agenda.next_sequence;
        self.agenda
            .submit_batch(requests)
            .map_err(SparseGenerationError::Agenda)?;
        if !requests.is_empty() {
            // submit_batch has already checked the u64 sequence range, so the
            // inclusive range cannot overflow.
            for sequence in previous_sequence + 1..=self.agenda.next_sequence {
                self.stamps.insert(sequence, stamp);
            }
        }
        Ok(())
    }

    /// Retire stale events (with explicit accounting), then dispatch at most
    /// one current event. A failed handler preserves that current event and
    /// its stamp for retry. The handler itself must be transactional.
    pub fn poll_next<E, F>(
        &mut self,
        handler: F,
    ) -> Result<Option<SparseEvent>, SparseDispatchError<E>>
    where
        F: FnOnce(SparseEvent) -> Result<(), E>,
    {
        let mut handler = Some(handler);
        loop {
            let Some(event) = self.agenda.peek() else {
                return Ok(None);
            };
            let stamp = *self
                .stamps
                .get(&event.sequence)
                .expect("every versioned event must retain its source stamp");
            let &current = self
                .generations
                .get(&stamp.source_id)
                .expect("registered source cannot disappear");
            if current != stamp.generation {
                let (_, retired) = self
                    .agenda
                    .pending
                    .pop_first()
                    .expect("peeked stale event must remain queued");
                debug_assert_eq!(retired, event);
                self.stamps.remove(&event.sequence);
                self.agenda.last_committed_time = Some(event.logical_time);
                self.stale_retired = self
                    .stale_retired
                    .checked_add(1)
                    .expect("stale retirement counter overflow");
                continue;
            }

            let committed = self
                .agenda
                .poll_next(handler.take().expect("handler is invoked at most once"))?;
            self.stamps.remove(&event.sequence);
            return Ok(committed);
        }
    }

    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.agenda.pending_count()
    }

    #[must_use]
    pub const fn stale_retired(&self) -> u64 {
        self.stale_retired
    }

    #[must_use]
    pub const fn agenda_evidence(&self) -> SparseAgendaEvidence {
        self.agenda.evidence()
    }

    #[must_use]
    pub const fn last_committed_time(&self) -> Option<u64> {
        self.agenda.last_committed_time()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::async_sparse::{SparseEventKind, SparseEventTarget};

    fn rising(time: u64, id: usize) -> SparseEventRequest {
        SparseEventRequest {
            logical_time: time,
            target: SparseEventTarget::Feature(id),
            kind: SparseEventKind::Rising,
        }
    }

    fn stamp(source_id: u64, generation: u64) -> SparseSourceStamp {
        SparseSourceStamp {
            source_id,
            generation,
        }
    }

    #[test]
    fn generation_advance_retires_stale_work_without_handler_execution() {
        let mut agenda = VersionedSparseAgenda::new(4).unwrap();
        agenda.set_source_generation(17, 1).unwrap();
        agenda
            .submit_from(stamp(17, 1), &[rising(4, 1), rising(4, 2)])
            .unwrap();
        agenda.set_source_generation(17, 2).unwrap();
        agenda.submit_from(stamp(17, 2), &[rising(5, 3)]).unwrap();

        let mut executed = Vec::new();
        let event = agenda
            .poll_next(|event| {
                executed.push(event.target);
                Ok::<(), ()>(())
            })
            .unwrap()
            .unwrap();
        assert_eq!(event.target, SparseEventTarget::Feature(3));
        assert_eq!(executed, [SparseEventTarget::Feature(3)]);
        assert_eq!(agenda.stale_retired(), 2);
        assert_eq!(agenda.agenda_evidence().submitted, 3);
        assert_eq!(agenda.agenda_evidence().committed, 1);
        assert_eq!(agenda.pending_count(), 0);
        assert_eq!(agenda.last_committed_time(), Some(5));
    }

    #[test]
    fn unrelated_source_survives_replacement_and_failed_handler_can_retry() {
        let mut agenda = VersionedSparseAgenda::new(3).unwrap();
        agenda.set_source_generation(1, 1).unwrap();
        agenda.set_source_generation(2, 1).unwrap();
        agenda.submit_from(stamp(1, 1), &[rising(7, 1)]).unwrap();
        agenda.submit_from(stamp(2, 1), &[rising(7, 2)]).unwrap();
        agenda.set_source_generation(1, 2).unwrap();

        assert_eq!(
            agenda.poll_next(|_| Err::<(), _>("reject")),
            Err(SparseDispatchError::Handler("reject"))
        );
        assert_eq!(agenda.stale_retired(), 1);
        assert_eq!(agenda.pending_count(), 1);
        assert_eq!(agenda.agenda_evidence().committed, 0);
        let retained = agenda.clone();
        assert_eq!(
            agenda.poll_next(|_| Err::<(), _>("retry")),
            Err(SparseDispatchError::Handler("retry"))
        );
        assert_eq!(agenda, retained);
        assert_eq!(
            agenda
                .poll_next(|event| {
                    assert_eq!(event.target, SparseEventTarget::Feature(2));
                    Ok::<(), ()>(())
                })
                .unwrap()
                .unwrap()
                .logical_time,
            7
        );
        assert_eq!(agenda.agenda_evidence().committed, 1);
    }

    #[test]
    fn unknown_stale_and_regressed_source_generation_fail_closed() {
        let mut agenda = VersionedSparseAgenda::new(2).unwrap();
        assert_eq!(
            agenda.submit_from(stamp(9, 1), &[rising(1, 1)]),
            Err(SparseGenerationError::UnknownSource { source_id: 9 })
        );
        agenda.set_source_generation(9, 2).unwrap();
        let before = agenda.clone();
        assert_eq!(
            agenda.set_source_generation(9, 1),
            Err(SparseGenerationError::GenerationRegression {
                source_id: 9,
                current: 2,
                proposed: 1,
            })
        );
        assert_eq!(
            agenda.submit_from(stamp(9, 1), &[rising(1, 1)]),
            Err(SparseGenerationError::StaleSubmission {
                source_id: 9,
                current: 2,
                submitted: 1,
            })
        );
        assert_eq!(agenda, before);
    }

    #[test]
    fn queue_full_never_leaves_partial_generation_stamps() {
        let mut agenda = VersionedSparseAgenda::new(1).unwrap();
        agenda.set_source_generation(4, 7).unwrap();
        let before = agenda.clone();
        assert_eq!(
            agenda.submit_from(stamp(4, 7), &[rising(1, 1), rising(1, 2)]),
            Err(SparseGenerationError::Agenda(SparseAgendaError::QueueFull {
                capacity: 1,
                pending: 0,
                incoming: 2,
            }))
        );
        assert_eq!(agenda, before);
        agenda.submit_from(stamp(4, 7), &[rising(1, 3)]).unwrap();
        assert_eq!(
            agenda
                .poll_next(|_| Ok::<(), ()>(()))
                .unwrap()
                .unwrap()
                .sequence,
            1
        );
        assert_eq!(agenda.stale_retired(), 0);
    }

    #[test]
    fn empty_batch_does_not_overflow_sequence_or_invoke_callback() {
        let mut agenda = VersionedSparseAgenda::new(1).unwrap();
        agenda.set_source_generation(8, u64::MAX).unwrap();
        agenda.submit_from(stamp(8, u64::MAX), &[]).unwrap();
        assert_eq!(agenda.poll_next::<(), _>(|_| panic!("no events")), Ok(None));
        assert_eq!(agenda.agenda_evidence().submitted, 0);
    }
}
