//! ENG-2a: non-actuating, model-owned transition feasibility for exact byte N-grams.
//!
//! Existing ENG-1a full-suffix and ENG-1b reverse-trie models are fitted from
//! ONE retained exact TRAIN byte stream; any proposed same-depth format switch
//! undergoes complete 256-class TRAIN and independent-probe parity before
//! generic ElasticXxx structural admission. The proposal is ALWAYS rolled back.
//!
//! This is a bounded research verifier, not an executable physical allocator,
//! codec, production model router, or an authorization for depth adaptation.

use std::fmt;

use elastic_core::{
    CapabilitySet, RepresentationEpoch, RepresentationId, RepresentationState, TargetContract,
    TransitionAttestations, TransitionMechanism, VersionFrontier,
};
use sml_core::{
    anf_char::{ChunkLayout, LANES},
    elastic_ngram_long::{ElasticNGramLong, LongNGramLimits},
    elastic_ngram_trie::{ElasticNGramTrie, TrieLimits},
};

pub const ENG2_MAX_VERIFY_EVENTS: usize = 8192;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NGramFormat {
    FullSuffix,
    SharedTrie,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NGramProfile {
    pub format: NGramFormat,
    pub depth: usize,
}

impl NGramProfile {
    pub const fn new(format: NGramFormat, depth: usize) -> Self {
        Self { format, depth }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionError {
    InvalidCorpus,
    InvalidProbe,
    SameDataNotIndependent,
    WrongTrainSource,
    TooManyVerificationEvents,
    InvalidDepth,
    ModelSemanticsChanged,
    UnprovenPhysicalReencoder,
    MissingNewState,
    EpochOverflow,
    ProbabilitiesDiffer { depth: usize, target: usize },
    DistributionInvalid,
    ProbeScoreDiffer,
    ModelBuild(String),
    GenericFrontier(String),
    PhysicalActuationUnavailable,
}

impl fmt::Display for AdmissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for AdmissionError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InspectionOutcome {
    Noop,
    ValidatedThenRolledBack,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InspectionEvidence {
    pub from: NGramProfile,
    pub to: NGramProfile,
    pub train_events: usize,
    pub independent_probe_events: usize,
    pub verified_probability_cells: usize,
    pub trie_nodes: usize,
    pub trie_follower_pairs: usize,
    pub full_suffix_contexts: usize,
    pub outcome: InspectionOutcome,
}

/// Both views and their complete TRAIN bytes are retained together. This
/// deliberately pays an extra RAM cost rather than asserting that a hash or
/// a caller-supplied string cryptographically binds the source.
#[derive(Clone, Debug)]
pub struct TrainingSnapshot {
    train: Vec<u8>,
    trained_depth: usize,
    full: ElasticNGramLong,
    trie: ElasticNGramTrie,
    train_events: usize,
}

fn counted_events(bytes: &[u8]) -> Result<usize, AdmissionError> {
    let layout = ChunkLayout::new(bytes.len()).map_err(|_| AdmissionError::InvalidCorpus)?;
    LANES
        .checked_mul(layout.chunk - 1)
        .ok_or(AdmissionError::TooManyVerificationEvents)
}

fn representation(profile: NGramProfile) -> Result<RepresentationId, AdmissionError> {
    let format = match profile.format {
        NGramFormat::FullSuffix => "full",
        NGramFormat::SharedTrie => "trie",
    };
    RepresentationId::new(format!("sml.eng2a.ngram.{format}.d{}", profile.depth))
        .map_err(|e| AdmissionError::GenericFrontier(e.to_string()))
}

fn as_generic_error(error: impl std::fmt::Display) -> AdmissionError {
    AdmissionError::GenericFrontier(error.to_string())
}

impl TrainingSnapshot {
    /// Nothing is published on a failed fit. Both implementations use the
    /// same TRAIN argument, immutable after fitting.
    pub fn fit(train: &[u8], max_depth: usize) -> Result<Self, AdmissionError> {
        if max_depth > 32 {
            return Err(AdmissionError::InvalidDepth);
        }
        let train_events = counted_events(train)?;
        if train_events > ENG2_MAX_VERIFY_EVENTS {
            return Err(AdmissionError::TooManyVerificationEvents);
        }
        let full = ElasticNGramLong::train(
            train,
            LongNGramLimits {
                max_depth,
                ..LongNGramLimits::default()
            },
        )
        .map_err(|e| AdmissionError::ModelBuild(format!("full: {e}")))?;
        let trie = ElasticNGramTrie::train(
            train,
            TrieLimits {
                max_depth,
                ..TrieLimits::default()
            },
        )
        .map_err(|e| AdmissionError::ModelBuild(format!("trie: {e}")))?;
        if full.accounting().contexts != trie.accounting().nodes
            || full.accounting().follower_pairs != trie.accounting().follower_pairs
            || full.accounting().training_events != train_events
            || trie.accounting().training_events != train_events
        {
            return Err(AdmissionError::ModelBuild(
                "different fitted context/follower accounting".into(),
            ));
        }
        Ok(Self {
            train: train.to_vec(),
            trained_depth: max_depth,
            full,
            trie,
            train_events,
        })
    }

    /// Real actuation is intentionally impossible in this phase.
    pub fn request_physical_actuation(&self) -> Result<(), AdmissionError> {
        Err(AdmissionError::PhysicalActuationUnavailable)
    }

    #[must_use]
    pub fn trained_depth(&self) -> usize {
        self.trained_depth
    }

    /// Compare EVERY evaluated row in a 64-lane dataset at the exact
    /// requested depth. The complete class vector is checked bit-for-bit.
    fn verify_corpus(&self, data: &[u8], depth: usize) -> Result<usize, AdmissionError> {
        let layout = ChunkLayout::new(data.len()).map_err(|_| AdmissionError::InvalidCorpus)?;
        let mut verified = 0usize;
        for lane in 0..LANES {
            let chunk = &data[lane * layout.chunk..(lane + 1) * layout.chunk];
            for t in 1..chunk.len() {
                let history = &chunk[..t];
                let long = self
                    .full
                    .distribution(history, depth)
                    .map_err(|e| AdmissionError::ModelBuild(e.to_string()))?;
                let trie = self
                    .trie
                    .distribution(history, depth)
                    .map_err(|e| AdmissionError::ModelBuild(e.to_string()))?;
                for y in 0..256 {
                    if long[y].to_bits() != trie[y].to_bits() {
                        return Err(AdmissionError::ProbabilitiesDiffer { depth, target: y });
                    }
                    if !long[y].is_finite() || long[y] <= 0.0 {
                        return Err(AdmissionError::DistributionInvalid);
                    }
                }
                if (long.iter().sum::<f64>() - 1.0).abs() > 1e-10 {
                    return Err(AdmissionError::DistributionInvalid);
                }
                verified += 1;
            }
        }
        Ok(verified)
    }

    /// Inspect a model-owned same-depth representation plan.
    ///
    /// SML rejects semantic or physical-feasibility gaps BEFORE generic
    /// frontier validation. Generic validation then stages only an
    /// externally unattested Recompute *proposal*; it is always rolled
    /// back and has NO physical side effects.
    pub fn inspect(
        &self,
        source_bytes: &[u8],
        probe: &[u8],
        from: NGramProfile,
        to: NGramProfile,
        requested_mechanism: TransitionMechanism,
        epoch: u64,
    ) -> Result<InspectionEvidence, AdmissionError> {
        if source_bytes != self.train.as_slice() {
            return Err(AdmissionError::WrongTrainSource);
        }
        if probe == self.train.as_slice() {
            return Err(AdmissionError::SameDataNotIndependent);
        }
        let probe_events = counted_events(probe).map_err(|_| AdmissionError::InvalidProbe)?;
        if probe_events > ENG2_MAX_VERIFY_EVENTS {
            return Err(AdmissionError::TooManyVerificationEvents);
        }
        if from.depth > self.trained_depth || to.depth > self.trained_depth {
            return Err(AdmissionError::InvalidDepth);
        }
        if from.depth != to.depth {
            return Err(AdmissionError::ModelSemanticsChanged);
        }
        if epoch == u64::MAX {
            return Err(AdmissionError::EpochOverflow);
        }
        if from.format != to.format && requested_mechanism != TransitionMechanism::Recompute {
            return Err(AdmissionError::UnprovenPhysicalReencoder);
        }

        let train_checked = self.verify_corpus(&self.train, from.depth)?;
        let probe_checked = self.verify_corpus(probe, from.depth)?;
        if train_checked != self.train_events || probe_checked != probe_events {
            return Err(AdmissionError::InvalidProbe);
        }

        let left_score = self
            .full
            .score(probe, from.depth)
            .map_err(|e| AdmissionError::ModelBuild(e.to_string()))?;
        let right_score = self
            .trie
            .score(probe, from.depth)
            .map_err(|e| AdmissionError::ModelBuild(e.to_string()))?;
        if left_score.tokens != right_score.tokens
            || left_score.bits_per_byte.to_bits() != right_score.bits_per_byte.to_bits()
            || left_score.accuracy.to_bits() != right_score.accuracy.to_bits()
        {
            return Err(AdmissionError::ProbeScoreDiffer);
        }
        let outcome = if from == to {
            InspectionOutcome::Noop
        } else {
            let current =
                RepresentationState::new(representation(from)?, 1, RepresentationEpoch::new(epoch));
            let mut frontier = VersionFrontier::new(current.clone());
            let mut caps = CapabilitySet::default();
            caps.insert(representation(from)?, 1);
            caps.insert(representation(to)?, 1);

            let target = frontier
                .committed()
                .derive_target(
                    TargetContract::New {
                        id: representation(to)?,
                        schema_version: 1,
                    },
                    TransitionMechanism::Recompute,
                )
                .map_err(as_generic_error)?;
            frontier
                .propose(target, TransitionMechanism::Recompute)
                .map_err(as_generic_error)?;
            frontier
                .validate_pending(
                    &caps,
                    TransitionAttestations::none().attest_recompute_source_available(),
                )
                .map_err(as_generic_error)?;

            if frontier.rollback().is_none()
                || frontier.committed() != &current
                || frontier.pending().is_some()
            {
                return Err(AdmissionError::MissingNewState);
            }
            InspectionOutcome::ValidatedThenRolledBack
        };

        let cells = train_checked
            .checked_add(probe_checked)
            .and_then(|rows| rows.checked_mul(256))
            .ok_or(AdmissionError::TooManyVerificationEvents)?;
        Ok(InspectionEvidence {
            from,
            to,
            train_events: train_checked,
            independent_probe_events: probe_checked,
            verified_probability_cells: cells,
            trie_nodes: self.trie.accounting().nodes,
            trie_follower_pairs: self.trie.accounting().follower_pairs,
            full_suffix_contexts: self.full.accounting().contexts,
            outcome,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(offset: usize) -> Vec<u8> {
        (0..LANES)
            .flat_map(|lane| {
                (0..40usize).map(move |t| ((lane * 17 + t * 7 + (t % 3) * 11 + offset) % 23) as u8)
            })
            .collect()
    }

    fn profiles(depth: usize) -> (NGramProfile, NGramProfile) {
        (
            NGramProfile::new(NGramFormat::FullSuffix, depth),
            NGramProfile::new(NGramFormat::SharedTrie, depth),
        )
    }

    #[test]
    fn eng2_trie_gate_parity_and_rollback_both_directions_at_four_depths() {
        let train = fixture(0);
        let probe = fixture(13);
        let snapshot = TrainingSnapshot::fit(&train, 12).unwrap();
        assert_eq!(snapshot.trained_depth(), 12);
        for depth in [0, 4, 7, 12] {
            let (long, trie) = profiles(depth);
            for (from, to) in [(long, trie), (trie, long)] {
                let result = snapshot
                    .inspect(&train, &probe, from, to, TransitionMechanism::Recompute, 3)
                    .unwrap();
                assert_eq!(result.outcome, InspectionOutcome::ValidatedThenRolledBack);
                assert_eq!(result.train_events, LANES * 39);
                assert_eq!(result.independent_probe_events, LANES * 39);
                assert_eq!(result.verified_probability_cells, LANES * 39 * 2 * 256);
                assert_eq!(result.trie_nodes, result.full_suffix_contexts);
            }
        }
        assert_eq!(
            snapshot.request_physical_actuation(),
            Err(AdmissionError::PhysicalActuationUnavailable)
        );
    }

    #[test]
    fn eng2_trie_gate_strict_source_depth_codec_and_epoch_rejections() {
        let train = fixture(0);
        let probe = fixture(5);
        let snapshot = TrainingSnapshot::fit(&train, 12).unwrap();
        let (long, trie) = profiles(12);
        let mut wrong = train.clone();
        wrong[1] ^= 0xFF;
        assert_eq!(
            snapshot.inspect(
                &wrong,
                &probe,
                long,
                trie,
                TransitionMechanism::Recompute,
                1
            ),
            Err(AdmissionError::WrongTrainSource)
        );
        assert_eq!(
            snapshot.inspect(
                &train,
                &train,
                long,
                trie,
                TransitionMechanism::Recompute,
                1
            ),
            Err(AdmissionError::SameDataNotIndependent)
        );
        assert_eq!(
            snapshot.inspect(
                &train,
                &[0; 64],
                long,
                trie,
                TransitionMechanism::Recompute,
                1
            ),
            Err(AdmissionError::InvalidProbe)
        );
        assert_eq!(
            snapshot.inspect(
                &train,
                &probe,
                long,
                NGramProfile::new(NGramFormat::SharedTrie, 7),
                TransitionMechanism::Recompute,
                1
            ),
            Err(AdmissionError::ModelSemanticsChanged)
        );
        assert_eq!(
            snapshot.inspect(&train, &probe, long, trie, TransitionMechanism::Reencode, 1),
            Err(AdmissionError::UnprovenPhysicalReencoder)
        );
        assert_eq!(
            snapshot.inspect(
                &train,
                &probe,
                long,
                trie,
                TransitionMechanism::Recompute,
                u64::MAX
            ),
            Err(AdmissionError::EpochOverflow)
        );
        assert_eq!(
            snapshot.inspect(
                &train,
                &probe,
                long,
                NGramProfile::new(NGramFormat::SharedTrie, 33),
                TransitionMechanism::Recompute,
                1
            ),
            Err(AdmissionError::InvalidDepth)
        );
        let noop = snapshot
            .inspect(
                &train,
                &probe,
                long,
                long,
                TransitionMechanism::Recompute,
                1,
            )
            .unwrap();
        assert_eq!(noop.outcome, InspectionOutcome::Noop);
    }

    #[test]
    fn eng2_trie_gate_poisoned_shadow_fails_even_with_valid_generic_metadata() {
        let train = fixture(0);
        let probe = fixture(13);
        let mut snapshot = TrainingSnapshot::fit(&train, 12).unwrap();
        let poisoned = vec![200u8; train.len()];
        snapshot.trie = ElasticNGramTrie::train(
            &poisoned,
            TrieLimits {
                max_depth: 12,
                ..TrieLimits::default()
            },
        )
        .unwrap();
        let (from, to) = profiles(12);
        assert!(matches!(
            snapshot.inspect(&train, &probe, from, to, TransitionMechanism::Recompute, 0),
            Err(AdmissionError::ProbabilitiesDiffer { .. })
        ));
    }

    #[test]
    fn eng2_trie_gate_frozen_capacity_and_empty_corpus_refuse() {
        let oversized = vec![1u8; LANES * 130];
        assert!(matches!(
            TrainingSnapshot::fit(&oversized, 0),
            Err(AdmissionError::TooManyVerificationEvents)
        ));
        assert!(matches!(
            TrainingSnapshot::fit(&[0u8; LANES], 4),
            Err(AdmissionError::InvalidCorpus)
        ));
        assert!(matches!(
            TrainingSnapshot::fit(&fixture(0), 33),
            Err(AdmissionError::InvalidDepth)
        ));
    }

    #[test]
    fn eng2_trie_gate_twelve_byte_nontruncated_history() {
        let train = (0..LANES)
            .flat_map(|lane| {
                let tag = (lane % 2) as u8;
                std::iter::once(tag)
                    .chain(std::iter::repeat_n(0, 11))
                    .chain(std::iter::once(200 + tag))
                    .chain(std::iter::repeat_n(77, 7))
            })
            .collect::<Vec<_>>();
        let probe = fixture(19);
        let snapshot = TrainingSnapshot::fit(&train, 12).unwrap();
        let a = std::iter::once(0u8)
            .chain(std::iter::repeat_n(0, 11))
            .collect::<Vec<_>>();
        let b = std::iter::once(1u8)
            .chain(std::iter::repeat_n(0, 11))
            .collect::<Vec<_>>();
        assert_eq!(
            snapshot.full.distribution(&a, 7).unwrap(),
            snapshot.full.distribution(&b, 7).unwrap()
        );
        assert!(snapshot.trie.distribution(&a, 12).unwrap()[200] > 0.9);
        assert!(snapshot.trie.distribution(&b, 12).unwrap()[201] > 0.9);
        let (from, to) = profiles(12);
        assert_eq!(
            snapshot
                .inspect(&train, &probe, from, to, TransitionMechanism::Recompute, 4)
                .unwrap()
                .outcome,
            InspectionOutcome::ValidatedThenRolledBack
        );
    }
}
