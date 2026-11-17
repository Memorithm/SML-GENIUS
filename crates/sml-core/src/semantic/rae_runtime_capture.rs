//! P0: opt-in RAE observation at the *actual* native SBG→MOR route boundary.
//!
//! The live routed kernel still executes original MOR on every cycle. This
//! collector consumes the *already computed* native selected expert IDs:
//! unlike the standalone probe, it does not route a second time.
//! Whole-state adaptation scans/copies R; zero end-to-end memory-read
//! savings are claimed. The journal can later be owned by a real R writer.

use core::{convert::Infallible, fmt};

use crate::PageId;

use super::{
    rae_dirty_journal::{SbgDirtyJournalError, SbgRoutingDirtyJournal},
    rae_native_trace::{SbgRaeNativeFrame, SBG_RAE_MAX_EXPERTS, SBG_RAE_MAX_FRAMES},
    SemanticState,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RaeNativeCaptureError {
    InvalidExpertCapacity,
    TooManyFrames,
    NonIncreasingCycle { previous: u64, next: u64 },
    NoNativeExperts,
    ExpertOutOfRange(PageId),
    DuplicateNativeExpert(PageId),
    JournalCycle,
    ReconstructedMismatch,
    Allocation,
}

impl fmt::Display for RaeNativeCaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for RaeNativeCaptureError {}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RaeNativeCaptureAccounting {
    pub native_routes_observed: u64,
    pub dirty_words_delivered: u64,
    /// The fallback `replace` scans all R words each tick. This count
    /// prevents calling the opt-in collector a zero-scan implementation.
    pub whole_state_words_scanned: u64,
    pub full_reconciliation_words: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaeNativeRuntimeCapture<const BITS: usize> {
    journal: SbgRoutingDirtyJournal<BITS>,
    reconstructed: Vec<u64>,
    previous_state: SemanticState<BITS>,
    frames: Vec<SbgRaeNativeFrame>,
    expert_count: usize,
    last_tick: u64,
    counters: RaeNativeCaptureAccounting,
}

impl<const BITS: usize> RaeNativeRuntimeCapture<BITS> {
    pub fn new(
        initial_state: &SemanticState<BITS>,
        baseline_tick: u64,
        expert_count: usize,
    ) -> Result<Self, RaeNativeCaptureError> {
        if expert_count == 0 || expert_count > SBG_RAE_MAX_EXPERTS {
            return Err(RaeNativeCaptureError::InvalidExpertCapacity);
        }
        Ok(Self {
            journal: SbgRoutingDirtyJournal::new(initial_state.routing.clone(), baseline_tick),
            reconstructed: initial_state.routing.words().to_vec(),
            previous_state: initial_state.clone(),
            frames: Vec::new(),
            expert_count,
            last_tick: baseline_tick,
            counters: RaeNativeCaptureAccounting::default(),
        })
    }

    #[must_use]
    pub fn frames(&self) -> &[SbgRaeNativeFrame] {
        &self.frames
    }

    #[must_use]
    pub const fn accounting(&self) -> RaeNativeCaptureAccounting {
        self.counters
    }

    /// Receives a genuine native MOR decision, never a learned prediction.
    ///
    /// Model state may be built by arbitrary existing SBG kernels, which is
    /// why this conservative adapter currently calls whole-field
    /// `replace` before emitting exact dirty notifications. It is not an
    /// independently proven write-time dirty source. This method is
    /// transactional: every error leaves the recorder unchanged.
    pub fn observe_native(
        &mut self,
        tick: u64,
        state: &SemanticState<BITS>,
        native_selected: &[PageId],
        external_context_invalidated: bool,
    ) -> Result<(), RaeNativeCaptureError> {
        if tick <= self.last_tick {
            return Err(RaeNativeCaptureError::NonIncreasingCycle {
                previous: self.last_tick,
                next: tick,
            });
        }
        if self.frames.len() >= SBG_RAE_MAX_FRAMES {
            return Err(RaeNativeCaptureError::TooManyFrames);
        }
        if native_selected.is_empty() {
            return Err(RaeNativeCaptureError::NoNativeExperts);
        }
        let mut required = vec![0_u64; self.expert_count.div_ceil(64)];
        for &page in native_selected {
            let index = page.0 as usize;
            if index >= self.expert_count {
                return Err(RaeNativeCaptureError::ExpertOutOfRange(page));
            }
            let word = &mut required[index / 64];
            let bit = 1_u64 << (index % 64);
            if *word & bit != 0 {
                return Err(RaeNativeCaptureError::DuplicateNativeExpert(page));
            }
            *word |= bit;
        }

        let mut journal = self.journal.clone();
        let total_words = journal.routing().words().len() as u64;
        journal.replace(state.routing.clone());
        let mut reconstructed = self.reconstructed.clone();
        let mut rising = vec![0_u64; reconstructed.len()];
        let mut falling = vec![0_u64; reconstructed.len()];
        let mut dirty_words = 0_u64;
        journal
            .flush(tick, |notices| {
                dirty_words = notices.len() as u64;
                for notice in notices {
                    let previous = reconstructed[notice.index];
                    rising[notice.index] = notice.next & !previous;
                    falling[notice.index] = previous & !notice.next;
                    reconstructed[notice.index] = notice.next;
                }
                Ok::<(), Infallible>(())
            })
            .map_err(|error| match error {
                SbgDirtyJournalError::NonIncreasingTick { .. } => {
                    RaeNativeCaptureError::JournalCycle
                }
                SbgDirtyJournalError::Sink(never) => match never {},
            })?;
        if reconstructed != state.routing.words() {
            return Err(RaeNativeCaptureError::ReconstructedMismatch);
        }
        let rising_bits = rising.iter().map(|word| u64::from(word.count_ones())).sum();
        let falling_bits = falling
            .iter()
            .map(|word| u64::from(word.count_ones()))
            .sum();
        let active_bits = reconstructed
            .iter()
            .map(|word| u64::from(word.count_ones()))
            .sum();
        let vma_changed = state.value != self.previous_state.value
            || state.meaning_mask != self.previous_state.meaning_mask
            || state.ambiguity != self.previous_state.ambiguity;
        let frame = SbgRaeNativeFrame {
            cycle: tick,
            routing_words: reconstructed.clone(),
            rising_words: rising,
            falling_words: falling,
            required_pages_words: required,
            selected_experts: native_selected.to_vec(),
            rising_routing_bits: rising_bits,
            falling_routing_bits: falling_bits,
            active_routing_bits: active_bits,
            context_invalidated: external_context_invalidated || vma_changed,
        };
        let mut frames = self.frames.clone();
        frames
            .try_reserve(1)
            .map_err(|_| RaeNativeCaptureError::Allocation)?;
        frames.push(frame);
        self.journal = journal;
        self.reconstructed = reconstructed;
        self.previous_state = state.clone();
        self.frames = frames;
        self.last_tick = tick;
        self.counters.native_routes_observed += 1;
        self.counters.dirty_words_delivered += dirty_words;
        self.counters.whole_state_words_scanned += total_words;
        self.counters.full_reconciliation_words += total_words;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_native_labels_and_zero_fronts_are_recorded() {
        let first = SemanticState::<65>::zeroed().unwrap();
        let mut recorder = RaeNativeRuntimeCapture::new(&first, 0, 2).unwrap();
        let mut high = first.clone();
        high.routing.set(64, true).unwrap();
        recorder
            .observe_native(1, &high, &[PageId(1)], false)
            .unwrap();
        recorder
            .observe_native(2, &high, &[PageId(1)], false)
            .unwrap();
        assert_eq!(recorder.frames()[0].rising_words, vec![0, 1]);
        assert_eq!(recorder.frames()[1].rising_routing_bits, 0);
        assert_eq!(recorder.frames()[1].selected_experts, vec![PageId(1)]);
        assert_eq!(recorder.accounting().native_routes_observed, 2);
        assert_eq!(recorder.accounting().dirty_words_delivered, 1);
        assert_eq!(recorder.accounting().whole_state_words_scanned, 4);
    }

    #[test]
    fn invalid_native_label_and_clock_rollback_every_field() {
        let state = SemanticState::<8>::zeroed().unwrap();
        let mut recorder = RaeNativeRuntimeCapture::new(&state, 0, 2).unwrap();
        let before = recorder.clone();
        assert_eq!(
            recorder.observe_native(1, &state, &[PageId(0), PageId(0)], false),
            Err(RaeNativeCaptureError::DuplicateNativeExpert(PageId(0)))
        );
        assert_eq!(recorder, before);
        assert_eq!(
            recorder.observe_native(1, &state, &[PageId(2)], false),
            Err(RaeNativeCaptureError::ExpertOutOfRange(PageId(2)))
        );
        assert_eq!(recorder, before);
        recorder
            .observe_native(1, &state, &[PageId(0)], false)
            .unwrap();
        let saved = recorder.clone();
        assert_eq!(
            recorder.observe_native(1, &state, &[PageId(0)], false),
            Err(RaeNativeCaptureError::NonIncreasingCycle {
                previous: 1,
                next: 1,
            })
        );
        assert_eq!(recorder, saved);
    }

    #[test]
    fn non_routing_context_invalidation_does_not_require_a_front() {
        let initial = SemanticState::<8>::zeroed().unwrap();
        let mut second = initial.clone();
        second.value.set(3, true).unwrap();
        let mut observer = RaeNativeRuntimeCapture::new(&initial, 0, 2).unwrap();
        observer
            .observe_native(1, &initial, &[PageId(0)], false)
            .unwrap();
        observer
            .observe_native(2, &second, &[PageId(0)], false)
            .unwrap();
        assert_eq!(observer.frames()[1].rising_routing_bits, 0);
        assert!(observer.frames()[1].context_invalidated);
    }
}
