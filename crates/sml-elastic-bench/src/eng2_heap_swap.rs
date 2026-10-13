//! ENG-2b/H1: physically replace one heap-resident byte N-gram model
//! with an exact, equal-depth representation and independently verify
//! its predictions AFTER the swap, with fail-closed rollback.
//!
//! Research-only, single-threaded, in-process Rust heap model. This module
//! does NOT manage mmap, GPU, multiple readers, hard RSS budgets, crash
//! durability, or the SML production inference route.

use std::fmt;

use elastic_core::{
    CapabilitySet, RepresentationEpoch, RepresentationId, RepresentationState, TargetContract,
    TransitionAttestations, TransitionMechanism, VersionFrontier,
};
use sml_core::{
    anf_char::{ChunkLayout, LANES},
    char_baselines::ByteScore,
    elastic_ngram_long::{ElasticNGramLong, LongNGramLimits},
    elastic_ngram_trie::{ElasticNGramTrie, TrieLimits},
};

use crate::eng2_trie_gate::{NGramFormat, NGramProfile, ENG2_MAX_VERIFY_EVENTS};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HeapSwapError {
    InvalidCorpus,
    InvalidProbe,
    SameProbeAsTrain,
    WrongTrainingSource,
    TooManyEvents,
    InvalidDepth,
    ModelDepthChanged,
    StaleSourceEpoch,
    EpochOverflow,
    UnqualifiedReencoder,
    UnsupportedMechanism,
    ModelBuild(String),
    ClassMismatch { depth: usize, byte: usize },
    NonProbability,
    ScoreMismatch,
    Frontier(String),
    InjectedFailure,
}

impl fmt::Display for HeapSwapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for HeapSwapError {}

#[derive(Clone, Debug)]
enum ResidentNGram {
    Full(ElasticNGramLong),
    Trie(ElasticNGramTrie),
}

impl ResidentNGram {
    fn fit(source: &[u8], profile: NGramProfile) -> Result<Self, HeapSwapError> {
        match profile.format {
            NGramFormat::FullSuffix => ElasticNGramLong::train(
                source,
                LongNGramLimits {
                    max_depth: profile.depth,
                    ..LongNGramLimits::default()
                },
            )
            .map(Self::Full)
            .map_err(|e| HeapSwapError::ModelBuild(format!("full: {e}"))),
            NGramFormat::SharedTrie => ElasticNGramTrie::train(
                source,
                TrieLimits {
                    max_depth: profile.depth,
                    ..TrieLimits::default()
                },
            )
            .map(Self::Trie)
            .map_err(|e| HeapSwapError::ModelBuild(format!("trie: {e}"))),
        }
    }

    fn distribution(
        &self,
        history: &[u8],
        depth: usize,
    ) -> Result<[f64; 256], HeapSwapError> {
        match self {
            Self::Full(model) => model
                .distribution(history, depth)
                .map_err(|e| HeapSwapError::ModelBuild(format!("full: {e}"))),
            Self::Trie(model) => model
                .distribution(history, depth)
                .map_err(|e| HeapSwapError::ModelBuild(format!("trie: {e}"))),
        }
    }

    fn score(&self, bytes: &[u8], depth: usize) -> Result<ByteScore, HeapSwapError> {
        match self {
            Self::Full(model) => model
                .score(bytes, depth)
                .map_err(|e| HeapSwapError::ModelBuild(format!("full: {e}"))),
            Self::Trie(model) => model
                .score(bytes, depth)
                .map_err(|e| HeapSwapError::ModelBuild(format!("trie: {e}"))),
        }
    }

    fn accounting(&self) -> (usize, usize) {
        match self {
            Self::Full(model) => {
                let a = model.accounting();
                (a.contexts, a.follower_pairs)
            }
            Self::Trie(model) => {
                let a = model.accounting();
                (a.nodes, a.follower_pairs)
            }
        }
    }
}

fn events(bytes: &[u8]) -> Result<usize, HeapSwapError> {
    let layout = ChunkLayout::new(bytes.len()).map_err(|_| HeapSwapError::InvalidCorpus)?;
    let n = LANES
        .checked_mul(layout.chunk - 1)
        .ok_or(HeapSwapError::TooManyEvents)?;
    if n > ENG2_MAX_VERIFY_EVENTS {
        return Err(HeapSwapError::TooManyEvents);
    }
    Ok(n)
}

fn representation(profile: NGramProfile) -> Result<RepresentationId, HeapSwapError> {
    let name = match profile.format {
        NGramFormat::FullSuffix => "full",
        NGramFormat::SharedTrie => "trie",
    };
    RepresentationId::new(format!("sml.eng2b.heap.{name}.d{}", profile.depth))
        .map_err(|e| HeapSwapError::Frontier(e.to_string()))
}

fn structural_error(error: impl fmt::Display) -> HeapSwapError {
    HeapSwapError::Frontier(error.to_string())
}

/// Compare every 256-class prediction across every lane-bound context,
/// not only aggregate bpb. This is a bounded empirical equivalence oracle.
fn verify_all(
    old: &ResidentNGram,
    new: &ResidentNGram,
    corpus: &[u8],
    depth: usize,
) -> Result<usize, HeapSwapError> {
    let layout = ChunkLayout::new(corpus.len()).map_err(|_| HeapSwapError::InvalidCorpus)?;
    let mut checked = 0usize;
    for lane in 0..LANES {
        let chunk = &corpus[lane * layout.chunk..(lane + 1) * layout.chunk];
        for t in 1..chunk.len() {
            let history = &chunk[..t];
            let previous = old.distribution(history, depth)?;
            let next = new.distribution(history, depth)?;
            let mut sum = 0.0;
            for y in 0..256 {
                if !previous[y].is_finite()
                    || !next[y].is_finite()
                    || previous[y] <= 0.0
                    || next[y] <= 0.0
                {
                    return Err(HeapSwapError::NonProbability);
                }
                if previous[y].to_bits() != next[y].to_bits() {
                    return Err(HeapSwapError::ClassMismatch { depth, byte: y });
                }
                sum += next[y];
            }
            if (sum - 1.0).abs() > 1e-10 {
                return Err(HeapSwapError::NonProbability);
            }
            checked += 1;
        }
    }
    Ok(checked)
}

fn verify_two_corpora(
    old: &ResidentNGram,
    new: &ResidentNGram,
    train: &[u8],
    probe: &[u8],
    depth: usize,
) -> Result<usize, HeapSwapError> {
    let train_rows = verify_all(old, new, train, depth)?;
    let probe_rows = verify_all(old, new, probe, depth)?;
    let previous = old.score(probe, depth)?;
    let next = new.score(probe, depth)?;
    if previous.tokens != next.tokens
        || previous.bits_per_byte.to_bits() != next.bits_per_byte.to_bits()
        || previous.accuracy.to_bits() != next.accuracy.to_bits()
    {
        return Err(HeapSwapError::ScoreMismatch);
    }
    train_rows
        .checked_add(probe_rows)
        .and_then(|rows| rows.checked_mul(256))
        .ok_or(HeapSwapError::TooManyEvents)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeapOutcome {
    Noop,
    SwappedAndCommitted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeapSwitchEvidence {
    pub from: NGramProfile,
    pub to: NGramProfile,
    pub outcome: HeapOutcome,
    pub epoch_before: u64,
    pub epoch_after: u64,
    pub source_bytes: usize,
    pub train_events: usize,
    pub independent_probe_events: usize,
    pub pre_swap_probability_cells: usize,
    pub post_swap_probability_cells: usize,
    pub source_contexts: usize,
    pub destination_contexts: usize,
    pub source_follower_pairs: usize,
    pub destination_follower_pairs: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Inject {
    None,
    PoisonShadow,
    AfterHeapSwap,
    AfterPostVerify,
    AfterFrontierCommit,
}

#[derive(Debug)]
pub struct HeapNGramRuntime {
    train: Vec<u8>,
    active: Box<ResidentNGram>,
    profile: NGramProfile,
    frontier: VersionFrontier,
    train_events: usize,
}

impl HeapNGramRuntime {
    /// Exactly ONE representation is resident in this model owner on creation.
    pub fn fit(source: &[u8], depth: usize, format: NGramFormat) -> Result<Self, HeapSwapError> {
        if depth > 32 {
            return Err(HeapSwapError::InvalidDepth);
        }
        let train_events = events(source)?;
        let profile = NGramProfile::new(format, depth);
        let active = Box::new(ResidentNGram::fit(source, profile)?);
        let initial = RepresentationState::new(
            representation(profile)?,
            1,
            RepresentationEpoch::new(0),
        );
        Ok(Self {
            train: source.to_vec(),
            active,
            profile,
            frontier: VersionFrontier::new(initial),
            train_events,
        })
    }

    #[must_use]
    pub fn active_profile(&self) -> NGramProfile {
        self.profile
    }

    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.frontier.committed().epoch.get()
    }

    #[must_use]
    pub fn has_pending_transition(&self) -> bool {
        self.frontier.pending().is_some()
    }

    #[must_use]
    pub fn source_len(&self) -> usize {
        self.train.len()
    }

    pub fn distribution(&self, history: &[u8]) -> Result<[f64; 256], HeapSwapError> {
        self.active.distribution(history, self.profile.depth)
    }

    pub fn score(&self, probe: &[u8]) -> Result<ByteScore, HeapSwapError> {
        self.active.score(probe, self.profile.depth)
    }

    /// Actually swap heap-resident model, verify its predictions *after* the
    /// swap, then commit ElasticXxx's structural frontier. A failure restores
    /// the ORIGINAL heap Box and frontier, without guessing reconstructed data.
    pub fn switch(
        &mut self,
        supplied_train: &[u8],
        independent_probe: &[u8],
        target: NGramProfile,
        requested_mechanism: TransitionMechanism,
        expected_epoch: u64,
    ) -> Result<HeapSwitchEvidence, HeapSwapError> {
        self.switch_with_injection(
            supplied_train,
            independent_probe,
            target,
            requested_mechanism,
            expected_epoch,
            Inject::None,
        )
    }

    fn switch_with_injection(
        &mut self,
        supplied_train: &[u8],
        independent_probe: &[u8],
        target: NGramProfile,
        requested_mechanism: TransitionMechanism,
        expected_epoch: u64,
        injection: Inject,
    ) -> Result<HeapSwitchEvidence, HeapSwapError> {
        // Admission is entirely SML-owned. ElasticXxx's structural validator
        // is reached only AFTER these domain and byte-identity checks.
        if supplied_train != self.train.as_slice() {
            return Err(HeapSwapError::WrongTrainingSource);
        }
        if independent_probe == self.train.as_slice() {
            return Err(HeapSwapError::SameProbeAsTrain);
        }
        let probe_events = events(independent_probe).map_err(|e| match e {
            HeapSwapError::InvalidCorpus => HeapSwapError::InvalidProbe,
            other => other,
        })?;
        let before_epoch = self.epoch();
        if expected_epoch != before_epoch {
            return Err(HeapSwapError::StaleSourceEpoch);
        }
        if before_epoch == u64::MAX {
            return Err(HeapSwapError::EpochOverflow);
        }
        if target.depth > 32 {
            return Err(HeapSwapError::InvalidDepth);
        }
        if target.depth != self.profile.depth {
            return Err(HeapSwapError::ModelDepthChanged);
        }
        if requested_mechanism != TransitionMechanism::Recompute {
            return Err(if requested_mechanism == TransitionMechanism::Reencode {
                HeapSwapError::UnqualifiedReencoder
            } else {
                HeapSwapError::UnsupportedMechanism
            });
        }
        let (before_contexts, before_pairs) = self.active.accounting();
        let source_profile = self.profile;
        if source_profile == target {
            return Ok(HeapSwitchEvidence {
                from: source_profile,
                to: target,
                outcome: HeapOutcome::Noop,
                epoch_before: before_epoch,
                epoch_after: before_epoch,
                source_bytes: self.train.len(),
                train_events: self.train_events,
                independent_probe_events: probe_events,
                pre_swap_probability_cells: 0,
                post_swap_probability_cells: 0,
                source_contexts: before_contexts,
                destination_contexts: before_contexts,
                source_follower_pairs: before_pairs,
                destination_follower_pairs: before_pairs,
            });
        }

        // A different model is really materialized on the heap. No code path
        // accepts an unqualified physical codec or depth downgrade.
        let poisoned = (injection == Inject::PoisonShadow).then(|| vec![200; self.train.len()]);
        let source = poisoned.as_deref().unwrap_or(&self.train);
        let new_resident = Box::new(ResidentNGram::fit(source, target)?);
        let (target_contexts, target_pairs) = new_resident.accounting();
        if (before_contexts, before_pairs) != (target_contexts, target_pairs) {
            return Err(HeapSwapError::ModelBuild(
                "source/target context and follower cardinalities differ".into(),
            ));
        }
        let prechecked = verify_two_corpora(
            self.active.as_ref(),
            new_resident.as_ref(),
            &self.train,
            independent_probe,
            target.depth,
        )?;

        // Stage against the source epoch. Structural attestation describes a
        // Recompute source, not an equivalence/physical-resource certificate.
        let saved_frontier = self.frontier.clone();
        let mut caps = CapabilitySet::default();
        caps.insert(representation(source_profile)?, 1);
        caps.insert(representation(target)?, 1);
        let next = self
            .frontier
            .committed()
            .derive_target(
                TargetContract::New {
                    id: representation(target)?,
                    schema_version: 1,
                },
                TransitionMechanism::Recompute,
            )
            .map_err(structural_error)?;
        self.frontier
            .propose(next, TransitionMechanism::Recompute)
            .map_err(structural_error)?;
        let attest = TransitionAttestations::none().attest_recompute_source_available();
        if let Err(error) = self.frontier.validate_pending(&caps, attest) {
            self.frontier = saved_frontier;
            return Err(structural_error(error));
        }

        // ACT: this is an actual heap allocation replacement, not merely a
        // planned state or a guessed logical resource representation.
        let original_box = std::mem::replace(&mut self.active, new_resident);

        // VERIFY after ACT with an independent pointer to the previous
        // resident model and all 256 probabilities, not only score summary.
        let attempted = (|| -> Result<usize, HeapSwapError> {
            if injection == Inject::AfterHeapSwap {
                return Err(HeapSwapError::InjectedFailure);
            }
            let postchecked = verify_two_corpora(
                original_box.as_ref(),
                self.active.as_ref(),
                &self.train,
                independent_probe,
                target.depth,
            )?;
            if injection == Inject::AfterPostVerify {
                return Err(HeapSwapError::InjectedFailure);
            }

            self.frontier
                .commit(
                    &caps,
                    TransitionAttestations::none().attest_recompute_source_available(),
                )
                .map_err(structural_error)?;
            if injection == Inject::AfterFrontierCommit {
                return Err(HeapSwapError::InjectedFailure);
            }
            Ok(postchecked)
        })();

        match attempted {
            Ok(postchecked) => {
                // Old allocations are dropped only AFTER verified commit.
                self.profile = target;
                drop(original_box);
                Ok(HeapSwitchEvidence {
                    from: source_profile,
                    to: target,
                    outcome: HeapOutcome::SwappedAndCommitted,
                    epoch_before: before_epoch,
                    epoch_after: self.epoch(),
                    source_bytes: self.train.len(),
                    train_events: self.train_events,
                    independent_probe_events: probe_events,
                    pre_swap_probability_cells: prechecked,
                    post_swap_probability_cells: postchecked,
                    source_contexts: before_contexts,
                    destination_contexts: target_contexts,
                    source_follower_pairs: before_pairs,
                    destination_follower_pairs: target_pairs,
                })
            }
            Err(problem) => {
                // ROLLBACK: restore the original resident heap object, not an
                // inferred recreation. Discard the speculative new allocation.
                drop(std::mem::replace(&mut self.active, original_box));
                self.frontier = saved_frontier;
                Err(problem)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(seed: usize) -> Vec<u8> {
        (0..LANES)
            .flat_map(|lane| {
                (0..40usize)
                    .map(move |t| ((lane * 17 + t * 7 + (t % 3) * 11 + seed) % 23) as u8)
            })
            .collect()
    }

    fn same_depth(format: NGramFormat, depth: usize) -> NGramProfile {
        NGramProfile::new(format, depth)
    }

    fn bits_at(r: &HeapNGramRuntime, history: &[u8]) -> Vec<u64> {
        r.distribution(history)
            .unwrap()
            .iter()
            .map(|p| p.to_bits())
            .collect()
    }

    #[test]
    fn eng2b_real_heap_switch_and_reverse_commit_keep_fixed_depth_probabilities() {
        let train = fixture(0);
        let probe = fixture(13);
        let mut runtime = HeapNGramRuntime::fit(&train, 12, NGramFormat::FullSuffix).unwrap();
        let history = &probe[..39];
        let initial = bits_at(&runtime, history);
        let before_pointer = runtime.active.as_ref() as *const ResidentNGram;
        let forward = runtime
            .switch(
                &train,
                &probe,
                same_depth(NGramFormat::SharedTrie, 12),
                TransitionMechanism::Recompute,
                0,
            )
            .unwrap();
        assert_eq!(forward.outcome, HeapOutcome::SwappedAndCommitted);
        assert_eq!((forward.epoch_before, forward.epoch_after), (0, 1));
        assert_eq!(forward.pre_swap_probability_cells, 2 * LANES * 39 * 256);
        assert_eq!(forward.post_swap_probability_cells, 2 * LANES * 39 * 256);
        assert_eq!(runtime.active_profile().format, NGramFormat::SharedTrie);
        assert_ne!(runtime.active.as_ref() as *const ResidentNGram, before_pointer);
        assert_eq!(bits_at(&runtime, history), initial);
        assert!(!runtime.has_pending_transition());

        let new_pointer = runtime.active.as_ref() as *const ResidentNGram;
        let noop = runtime
            .switch(
                &train,
                &probe,
                same_depth(NGramFormat::SharedTrie, 12),
                TransitionMechanism::Recompute,
                1,
            )
            .unwrap();
        assert_eq!(noop.outcome, HeapOutcome::Noop);
        assert_eq!(runtime.active.as_ref() as *const ResidentNGram, new_pointer);
        assert_eq!(runtime.epoch(), 1);

        let reverse = runtime
            .switch(
                &train,
                &probe,
                same_depth(NGramFormat::FullSuffix, 12),
                TransitionMechanism::Recompute,
                1,
            )
            .unwrap();
        assert_eq!(reverse.outcome, HeapOutcome::SwappedAndCommitted);
        assert_eq!(reverse.epoch_after, 2);
        assert_eq!(runtime.active_profile().format, NGramFormat::FullSuffix);
        assert_eq!(bits_at(&runtime, history), initial);
        assert!(!runtime.has_pending_transition());
    }

    #[test]
    fn eng2b_fail_closed_prechecks_do_not_touch_live_heap_or_frontier() {
        let train = fixture(0);
        let probe = fixture(5);
        let mut runtime = HeapNGramRuntime::fit(&train, 12, NGramFormat::FullSuffix).unwrap();
        let target = same_depth(NGramFormat::SharedTrie, 12);
        let prior = bits_at(&runtime, &probe[..39]);
        let pointer = runtime.active.as_ref() as *const ResidentNGram;
        let mut wrong = train.clone();
        wrong[0] ^= 1;

        let cases = [
            runtime.switch(&wrong, &probe, target, TransitionMechanism::Recompute, 0),
            runtime.switch(&train, &train, target, TransitionMechanism::Recompute, 0),
            runtime.switch(&train, &[0; 64], target, TransitionMechanism::Recompute, 0),
            runtime.switch(&train, &probe, target, TransitionMechanism::Recompute, 1),
            runtime.switch(&train, &probe, target, TransitionMechanism::Reencode, 0),
            runtime.switch(&train, &probe, target, TransitionMechanism::Reinterpret, 0),
            runtime.switch(
                &train,
                &probe,
                same_depth(NGramFormat::SharedTrie, 7),
                TransitionMechanism::Recompute,
                0,
            ),
            runtime.switch(
                &train,
                &probe,
                same_depth(NGramFormat::SharedTrie, 33),
                TransitionMechanism::Recompute,
                0,
            ),
        ];
        let errors = [
            HeapSwapError::WrongTrainingSource,
            HeapSwapError::SameProbeAsTrain,
            HeapSwapError::InvalidProbe,
            HeapSwapError::StaleSourceEpoch,
            HeapSwapError::UnqualifiedReencoder,
            HeapSwapError::UnsupportedMechanism,
            HeapSwapError::ModelDepthChanged,
            HeapSwapError::InvalidDepth,
        ];
        for (actual, expected) in cases.into_iter().zip(errors) {
            assert_eq!(actual, Err(expected));
        }
        assert_eq!(runtime.active.as_ref() as *const ResidentNGram, pointer);
        assert_eq!(runtime.epoch(), 0);
        assert!(!runtime.has_pending_transition());
        assert_eq!(bits_at(&runtime, &probe[..39]), prior);
    }

    #[test]
    fn eng2b_injected_post_swap_post_verify_and_post_commit_failures_restore_original_box() {
        let train = fixture(0);
        let probe = fixture(13);
        let target = same_depth(NGramFormat::SharedTrie, 12);
        let mut runtime = HeapNGramRuntime::fit(&train, 12, NGramFormat::FullSuffix).unwrap();
        let ptr = runtime.active.as_ref() as *const ResidentNGram;
        let before = bits_at(&runtime, &probe[..39]);
        for fault in [
            Inject::AfterHeapSwap,
            Inject::AfterPostVerify,
            Inject::AfterFrontierCommit,
        ] {
            assert_eq!(
                runtime.switch_with_injection(
                    &train,
                    &probe,
                    target,
                    TransitionMechanism::Recompute,
                    0,
                    fault,
                ),
                Err(HeapSwapError::InjectedFailure)
            );
            assert_eq!(runtime.active.as_ref() as *const ResidentNGram, ptr);
            assert_eq!(runtime.active_profile().format, NGramFormat::FullSuffix);
            assert_eq!(runtime.epoch(), 0);
            assert!(!runtime.has_pending_transition());
            assert_eq!(bits_at(&runtime, &probe[..39]), before);
        }
        // A failed physical attempt leaves the original source available
        // for a subsequent actual successful switch.
        assert_eq!(
            runtime
                .switch(
                    &train,
                    &probe,
                    target,
                    TransitionMechanism::Recompute,
                    0
                )
                .unwrap()
                .epoch_after,
            1
        );
    }

    #[test]
    fn eng2b_tampered_materialization_rejects_before_resident_swap() {
        let train = fixture(0);
        let probe = fixture(13);
        let mut runtime = HeapNGramRuntime::fit(&train, 12, NGramFormat::FullSuffix).unwrap();
        let ptr = runtime.active.as_ref() as *const ResidentNGram;
        assert!(matches!(
            runtime.switch_with_injection(
                &train,
                &probe,
                same_depth(NGramFormat::SharedTrie, 12),
                TransitionMechanism::Recompute,
                0,
                Inject::PoisonShadow
            ),
            Err(HeapSwapError::ClassMismatch { .. })
                | Err(HeapSwapError::ModelBuild(_))
        ));
        assert_eq!(runtime.active.as_ref() as *const ResidentNGram, ptr);
        assert_eq!(runtime.epoch(), 0);
    }

    #[test]
    fn eng2b_zero_to_32_exact_depth_and_8192_event_limit() {
        let train = fixture(0);
        let probe = fixture(7);
        for depth in [0, 4, 7, 12, 32] {
            let mut runtime = HeapNGramRuntime::fit(&train, depth, NGramFormat::FullSuffix)
                .unwrap();
            let before = bits_at(&runtime, &probe[..39]);
            runtime
                .switch(
                    &train,
                    &probe,
                    same_depth(NGramFormat::SharedTrie, depth),
                    TransitionMechanism::Recompute,
                    0,
                )
                .unwrap();
            assert_eq!(bits_at(&runtime, &probe[..39]), before);
        }
        assert!(matches!(
            HeapNGramRuntime::fit(&vec![1; LANES * 130], 32, NGramFormat::SharedTrie),
            Err(HeapSwapError::TooManyEvents)
        ));
        assert!(matches!(
            HeapNGramRuntime::fit(&[1; LANES], 4, NGramFormat::FullSuffix),
            Err(HeapSwapError::InvalidCorpus)
        ));
        assert!(matches!(
            HeapNGramRuntime::fit(&train, 33, NGramFormat::FullSuffix),
            Err(HeapSwapError::InvalidDepth)
        ));
    }

    #[test]
    fn eng2b_real_heap_twelve_byte_witness_cannot_truncate_to_legacy_seven() {
        let train = (0..LANES)
            .flat_map(|lane| {
                let tag = (lane % 2) as u8;
                std::iter::once(tag)
                    .chain(std::iter::repeat_n(0, 11))
                    .chain(std::iter::once(200 + tag))
                    .chain(std::iter::repeat_n(77, 7))
            })
            .collect::<Vec<_>>();
        let probe = fixture(13);
        let mut runtime = HeapNGramRuntime::fit(&train, 12, NGramFormat::FullSuffix).unwrap();
        let a = std::iter::once(0u8)
            .chain(std::iter::repeat_n(0, 11))
            .collect::<Vec<_>>();
        let b = std::iter::once(1u8)
            .chain(std::iter::repeat_n(0, 11))
            .collect::<Vec<_>>();
        assert!(runtime.distribution(&a).unwrap()[200] > 0.9);
        assert!(runtime.distribution(&b).unwrap()[201] > 0.9);
        runtime
            .switch(
                &train,
                &probe,
                same_depth(NGramFormat::SharedTrie, 12),
                TransitionMechanism::Recompute,
                0,
            )
            .unwrap();
        assert!(runtime.distribution(&a).unwrap()[200] > 0.9);
        assert!(runtime.distribution(&b).unwrap()[201] > 0.9);
        assert_eq!(runtime.epoch(), 1);
    }
}
