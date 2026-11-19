//! Model-owned transactional SBG R ingress into a causal event agenda.
//!
//! This opt-in bridge owns the native dirty-word producer. It is not the
//! authoritative SBG/MOR runtime mutator and cannot skip recurrent, expert
//! or memory work until a complete upstream mutation contract is proved.

use crate::semantic::{
    rae_dirty_journal::{
        SbgDirtyJournalError, SbgJournalEvidence, SbgRoutingDirtyJournal,
    },
    SemanticBitField, SemanticBitFieldError,
};

use super::{
    versioned::{
        SbgVersionedIngressError, SparseGenerationError, SparseSourceStamp,
        VersionedSparseAgenda,
    },
    SbgSparseEventIngress, SparseAgendaError, SparseAgendaEvidence, SparseDispatchError,
    SparseEvent, SparseEventKind, SparseEventRequest, SparseIngressEvidence,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SbgCausalBridgeError {
    NotAnInvalidation,
    Source(SparseGenerationError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SbgCausalFlushEvidence {
    pub journal: SbgJournalEvidence,
    pub ingress: SparseIngressEvidence,
}

/// Opt-in R mutation source, exact dirty-word ingress and generation-aware
/// bounded agenda sharing a single transactional acknowledgement boundary.
///
/// Writes through \`set\` are sparse. \`replace\` explicitly scans every
/// packed R word. No untracked external SBG writes are observed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SbgCausalEventBridge<const BITS: usize> {
    journal: SbgRoutingDirtyJournal<BITS>,
    ingress: SbgSparseEventIngress<BITS>,
    agenda: VersionedSparseAgenda,
    source_id: u64,
    generation: u64,
}

impl<const BITS: usize> SbgCausalEventBridge<BITS> {
    pub fn new(
        baseline: SemanticBitField<BITS>,
        baseline_tick: u64,
        capacity: usize,
        source_id: u64,
    ) -> Result<Self, SparseAgendaError> {
        let ingress = SbgSparseEventIngress::new(&baseline, baseline_tick);
        Ok(Self {
            journal: SbgRoutingDirtyJournal::new(baseline, baseline_tick),
            ingress,
            agenda: VersionedSparseAgenda::with_source(capacity, source_id, 0)?,
            source_id,
            generation: 0,
        })
    }

    #[must_use]
    pub fn routing(&self) -> &SemanticBitField<BITS> {
        self.journal.routing()
    }

    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub fn pending_words(&self) -> usize {
        self.journal.pending_word_count()
    }

    #[must_use]
    pub fn pending_events(&self) -> usize {
        self.agenda.pending_count()
    }

    #[must_use]
    pub const fn stale_retired(&self) -> u64 {
        self.agenda.stale_retired()
    }

    #[must_use]
    pub const fn agenda_evidence(&self) -> SparseAgendaEvidence {
        self.agenda.agenda_evidence()
    }

    /// Mutate exactly one R coordinate and record its packed word at write
    /// time. No full network/field scan occurs for point writes.
    pub fn set(&mut self, index: usize, enabled: bool) -> Result<bool, SemanticBitFieldError> {
        self.journal.set(index, enabled)
    }

    /// A full R replacement requires a visible packed-word scan.
    pub fn replace(&mut self, next: SemanticBitField<BITS>) -> usize {
        self.journal.replace(next)
    }

    /// Only the model owner may advance a source generation. It must reemit
    /// all causally required work, including held-state and non-R memory
    /// invalidations, before using stale-event retirement as an optimization.
    pub fn advance_generation(
        &mut self,
        next: u64,
    ) -> Result<(), SparseGenerationError> {
        self.agenda.set_source_generation(self.source_id, next)?;
        self.generation = next;
        Ok(())
    }

    /// Preserve context, memory and wake invalidations as explicit events;
    /// R rising/falling notifications must use the tracked dirty-word writer.
    pub fn enqueue_invalidation(
        &mut self,
        request: SparseEventRequest,
    ) -> Result<(), SbgCausalBridgeError> {
        if !matches!(
            request.kind,
            SparseEventKind::ContextInvalidation
                | SparseEventKind::MemoryInvalidation
                | SparseEventKind::Wake
        ) {
            return Err(SbgCausalBridgeError::NotAnInvalidation);
        }
        self.agenda
            .submit_from(
                SparseSourceStamp {
                    source_id: self.source_id,
                    generation: self.generation,
                },
                &[request],
            )
            .map_err(SbgCausalBridgeError::Source)
    }

    /// Journal acknowledgement, ingress baseline advancement and stamped
    /// event admission commit atomically. On failure, all remain retriable.
    pub fn flush(
        &mut self,
        tick: u64,
    ) -> Result<SbgCausalFlushEvidence, SbgDirtyJournalError<SbgVersionedIngressError>> {
        let stamp = SparseSourceStamp {
            source_id: self.source_id,
            generation: self.generation,
        };
        let mut emitted = None;
        let journal = self.journal.flush(tick, |deltas| {
            let evidence = self
                .ingress
                .submit_dirty_words_versioned(tick, deltas, stamp, &mut self.agenda)?;
            emitted = Some(evidence);
            Ok::<(), SbgVersionedIngressError>(())
        })?;
        Ok(SbgCausalFlushEvidence {
            journal,
            ingress: emitted.expect("successful journal sink must report ingress evidence"),
        })
    }

    /// Dispatch at most one causally current event, retiring all older
    /// leading generations. The caller's handler must be transactional.
    pub fn poll_next<E, F>(
        &mut self,
        handler: F,
    ) -> Result<Option<SparseEvent>, SparseDispatchError<E>>
    where
        F: FnOnce(SparseEvent) -> Result<(), E>,
    {
        self.agenda.poll_next(handler)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::async_sparse::SparseEventTarget;

    #[test]
    fn native_point_writes_reach_bounded_agenda_in_exact_order() {
        let baseline = SemanticBitField::<65>::zeroed().unwrap();
        let mut bridge = SbgCausalEventBridge::new(baseline, 0, 3, 11).unwrap();
        assert!(bridge.set(64, true).unwrap());
        assert!(bridge.set(0, true).unwrap());
        assert!(!bridge.set(0, true).unwrap());
        let record = bridge.flush(1).unwrap();
        assert_eq!(record.journal.dirty_words, 2);
        assert_eq!(record.ingress.rising, 2);
        assert_eq!(record.ingress.falling, 0);
        assert_eq!(bridge.pending_words(), 0);
        assert_eq!(bridge.pending_events(), 2);
        for target in [0, 64] {
            let event = bridge.poll_next(|_| Ok::<(), ()>(())).unwrap().unwrap();
            assert_eq!(event.target, SparseEventTarget::Feature(target));
            assert_eq!(event.kind, SparseEventKind::Rising);
        }
        assert_eq!(bridge.agenda_evidence().committed, 2);
    }

    #[test]
    fn newer_generation_retires_previous_edge_without_losing_new_edges() {
        let mut bridge =
            SbgCausalEventBridge::new(SemanticBitField::<65>::zeroed().unwrap(), 0, 4, 42)
                .unwrap();
        bridge.set(0, true).unwrap();
        bridge.flush(1).unwrap();
        bridge.advance_generation(1).unwrap();
        bridge.set(0, false).unwrap();
        bridge.set(64, true).unwrap();
        let record = bridge.flush(2).unwrap();
        assert_eq!((record.ingress.rising, record.ingress.falling), (1, 1));

        let mut observed = Vec::new();
        for _ in 0..2 {
            bridge
                .poll_next(|event| {
                    observed.push((event.target, event.kind));
                    Ok::<(), ()>(())
                })
                .unwrap();
        }
        assert_eq!(
            observed,
            vec![
                (SparseEventTarget::Feature(0), SparseEventKind::Falling),
                (SparseEventTarget::Feature(64), SparseEventKind::Rising),
            ]
        );
        assert_eq!(bridge.stale_retired(), 1);
        assert_eq!(bridge.agenda_evidence().committed, 2);
        assert_eq!(bridge.pending_events(), 0);
    }

    #[test]
    fn backpressure_preserves_journal_and_ingress_for_retry() {
        let mut bridge =
            SbgCausalEventBridge::new(SemanticBitField::<65>::zeroed().unwrap(), 0, 1, 2)
                .unwrap();
        bridge.set(0, true).unwrap();
        bridge.set(64, true).unwrap();
        let before = bridge.clone();
        assert!(matches!(
            bridge.flush(1),
            Err(SbgDirtyJournalError::Sink(SbgVersionedIngressError::Source(
                SparseGenerationError::Agenda(SparseAgendaError::QueueFull { .. })
            )))
        ));
        assert_eq!(bridge, before);
        // Revert one pending mutation, leaving one legitimate rising front.
        bridge.set(64, false).unwrap();
        let record = bridge.flush(1).unwrap();
        assert_eq!(record.ingress.rising, 1);
        assert_eq!(bridge.pending_events(), 1);
        assert_eq!(bridge.pending_words(), 0);
        assert_eq!(
            bridge.poll_next(|_| Ok::<(), ()>(())).unwrap().unwrap().target,
            SparseEventTarget::Feature(0)
        );
    }

    #[test]
    fn non_r_memory_invalidation_is_retained_and_spurious_edges_rejected() {
        let mut bridge =
            SbgCausalEventBridge::new(SemanticBitField::<1>::zeroed().unwrap(), 0, 1, 4)
                .unwrap();
        bridge.advance_generation(1).unwrap();
        let invalidation = SparseEventRequest {
            logical_time: 2,
            target: SparseEventTarget::Memory(7),
            kind: SparseEventKind::MemoryInvalidation,
        };
        bridge.enqueue_invalidation(invalidation).unwrap();
        let before = bridge.clone();
        assert_eq!(
            bridge.enqueue_invalidation(SparseEventRequest {
                kind: SparseEventKind::Rising,
                ..invalidation
            }),
            Err(SbgCausalBridgeError::NotAnInvalidation)
        );
        assert_eq!(bridge, before);
        assert_eq!(
            bridge.poll_next(|_| Ok::<(), ()>(())).unwrap().unwrap().kind,
            SparseEventKind::MemoryInvalidation
        );
        assert_eq!(bridge.agenda_evidence().committed, 1);
    }

    #[test]
    fn handler_failure_retains_authoritative_current_event() {
        let mut bridge =
            SbgCausalEventBridge::new(SemanticBitField::<1>::zeroed().unwrap(), 0, 1, 4)
                .unwrap();
        bridge.set(0, true).unwrap();
        bridge.flush(1).unwrap();
        let before = bridge.clone();
        assert_eq!(
            bridge.poll_next(|_| Err::<(), _>("reject")),
            Err(SparseDispatchError::Handler("reject"))
        );
        assert_eq!(bridge, before);
        assert!(bridge.poll_next(|_| Ok::<(), ()>(())).unwrap().is_some());
        assert_eq!(bridge.agenda_evidence().committed, 1);
    }
}
