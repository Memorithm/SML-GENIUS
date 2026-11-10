//! SBG-REA-3: TRAIN-only sparse association learning from genuine MOR traces.
//!
//! Model-owned, research-only hard event routing. An observed 0 -> 1 R-field
//! feature votes for the original page IDs selected by native MOR. Predictions
//! are *counterfactual*: this module never suppresses the authoritative route,
//! expert execution, recurrent state, memory access, or decision.

use core::fmt;
use std::collections::BTreeMap;

use crate::{
    semantic::rae_native_trace::{SbgRaeNativeFrame, SBG_RAE_MAX_EXPERTS, SBG_RAE_MAX_FRAMES},
    PageId,
};

pub const RAE_LEARNING_PPM: u32 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RaeTrainPolicy {
    pub minimum_exposures: u64,
    pub minimum_precision_ppm: u32,
    pub max_associations: usize,
}

impl RaeTrainPolicy {
    fn validate(self) -> Result<Self, RaeLearnError> {
        if self.minimum_exposures == 0
            || self.minimum_precision_ppm > RAE_LEARNING_PPM
            || self.max_associations == 0
            || self.max_associations > 1_000_000
        {
            return Err(RaeLearnError::InvalidPolicy);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RaeLearnError {
    InvalidPolicy,
    InvalidDimensions,
    InvalidDatasetIdentity,
    IdenticalTrainingAndEvaluationIdentity,
    EmptyFrameSet,
    TooManyFrames,
    MalformedFrame { index: usize },
    TooManyAssociations,
}

impl fmt::Display for RaeLearnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for RaeLearnError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RaeLearnedAssociation {
    pub page_id: PageId,
    pub hits: u64,
    pub exposures: u64,
    pub precision_ppm: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RaeHoldoutCounts {
    pub selected_pages: u128,
    pub required_pages: u128,
    pub missed_required_pages: u128,
    pub extra_selected_pages: u128,
}

impl RaeHoldoutCounts {
    fn accumulate(&mut self, other: Self) {
        self.selected_pages += other.selected_pages;
        self.required_pages += other.required_pages;
        self.missed_required_pages += other.missed_required_pages;
        self.extra_selected_pages += other.extra_selected_pages;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaeHoldoutReport {
    pub frames: usize,
    pub totals: RaeHoldoutCounts,
    pub per_frame: Vec<RaeHoldoutCounts>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaeTrainedEventRouter {
    feature_width: usize,
    expert_count: usize,
    train_dataset_id: String,
    assignments: Vec<Option<RaeLearnedAssociation>>,
    observed_associations: usize,
    training_frames: usize,
}

impl RaeTrainedEventRouter {
    /// Fit one bounded, deterministic page association per rising feature.
    ///
    /// Each rising bit votes for every native MOR selected expert in the
    /// same TRAIN frame. A single feature with repeated co-occurrences can
    /// learn one page; ties select the lowest original PageId. A feature
    /// with insufficient evidence remains unrouted, never guessed.
    ///
    /// `train_dataset_id` is caller-supplied provenance, not cryptographic
    /// attestation. Caller MUST keep TRAIN and independent holdout separated.
    pub fn train<const BITS: usize>(
        train_dataset_id: &str,
        expert_count: usize,
        policy: RaeTrainPolicy,
        frames: &[SbgRaeNativeFrame],
    ) -> Result<Self, RaeLearnError> {
        let policy = policy.validate()?;
        validate_dimensions(BITS, expert_count)?;
        validate_identity(train_dataset_id)?;
        if frames.is_empty() {
            return Err(RaeLearnError::EmptyFrameSet);
        }
        if frames.len() > SBG_RAE_MAX_FRAMES {
            return Err(RaeLearnError::TooManyFrames);
        }
        let mut exposures = vec![0_u64; BITS];
        let mut votes = BTreeMap::<(usize, u32), u64>::new();

        for (index, frame) in frames.iter().enumerate() {
            validate_frame(frame, BITS, expert_count, index)?;
            if index > 0 {
                validate_transition(&frames[index - 1], frame, index)?;
            }
            for feature in set_indices(&frame.rising_words) {
                exposures[feature] += 1;
                for page_id in &frame.selected_experts {
                    let key = (feature, page_id.0);
                    if !votes.contains_key(&key) && votes.len() >= policy.max_associations {
                        return Err(RaeLearnError::TooManyAssociations);
                    }
                    *votes.entry(key).or_default() += 1;
                }
            }
        }

        let observed_associations = votes.len();
        let mut assignments = vec![None; BITS];
        for ((feature, page), hits) in votes {
            let total = exposures[feature];
            debug_assert!(total >= hits);
            let precision_ppm = ((u128::from(hits) * u128::from(RAE_LEARNING_PPM))
                / u128::from(total)) as u32;
            if total < policy.minimum_exposures || precision_ppm < policy.minimum_precision_ppm {
                continue;
            }
            let candidate = RaeLearnedAssociation {
                page_id: PageId(page),
                hits,
                exposures: total,
                precision_ppm,
            };
            let replace = assignments[feature].is_none_or(|best: RaeLearnedAssociation| {
                (hits, core::cmp::Reverse(page))
                    > (best.hits, core::cmp::Reverse(best.page_id.0))
            });
            if replace {
                assignments[feature] = Some(candidate);
            }
        }

        Ok(Self {
            feature_width: BITS,
            expert_count,
            train_dataset_id: train_dataset_id.to_owned(),
            assignments,
            observed_associations,
            training_frames: frames.len(),
        })
    }

    #[must_use]
    pub fn training_dataset_id(&self) -> &str {
        &self.train_dataset_id
    }

    #[must_use]
    pub const fn feature_width(&self) -> usize {
        self.feature_width
    }

    #[must_use]
    pub const fn expert_count(&self) -> usize {
        self.expert_count
    }

    #[must_use]
    pub const fn training_frames(&self) -> usize {
        self.training_frames
    }

    #[must_use]
    pub const fn observed_associations(&self) -> usize {
        self.observed_associations
    }

    #[must_use]
    pub fn learned_feature_count(&self) -> usize {
        self.assignments.iter().filter(|value| value.is_some()).count()
    }

    #[must_use]
    pub fn association(&self, feature: usize) -> Option<RaeLearnedAssociation> {
        self.assignments.get(feature).copied().flatten()
    }

    /// Select page IDs using only the observed rising-feature mask and
    /// frozen TRAIN-derived hard mapping. Zero fronts means zero selections.
    pub fn predict(&self, rising_words: &[u64]) -> Result<Vec<u64>, RaeLearnError> {
        validate_words(rising_words, self.feature_width)
            .map_err(|_| RaeLearnError::InvalidDimensions)?;
        let mut selected = vec![0_u64; self.expert_count.div_ceil(64)];
        for feature in set_indices(rising_words) {
            if let Some(association) = self.assignments[feature] {
                let index = association.page_id.0 as usize;
                selected[index / 64] |= 1_u64 << (index % 64);
            }
        }
        Ok(selected)
    }

    /// Score the frozen model against an *independent* original-page MOR
    /// reference. The split-ID check is not a proof of true data independence.
    pub fn evaluate(
        &self,
        holdout_dataset_id: &str,
        frames: &[SbgRaeNativeFrame],
    ) -> Result<RaeHoldoutReport, RaeLearnError> {
        validate_identity(holdout_dataset_id)?;
        if holdout_dataset_id == self.train_dataset_id {
            return Err(RaeLearnError::IdenticalTrainingAndEvaluationIdentity);
        }
        if frames.is_empty() {
            return Err(RaeLearnError::EmptyFrameSet);
        }
        if frames.len() > SBG_RAE_MAX_FRAMES {
            return Err(RaeLearnError::TooManyFrames);
        }

        let mut totals = RaeHoldoutCounts::default();
        let mut per_frame = Vec::with_capacity(frames.len());
        for (index, frame) in frames.iter().enumerate() {
            validate_frame(frame, self.feature_width, self.expert_count, index)?;
            if index > 0 {
                validate_transition(&frames[index - 1], frame, index)?;
            }
            let predicted = self.predict(&frame.rising_words)?;
            let counts = count_page_set_errors(&predicted, &frame.required_pages_words);
            totals.accumulate(counts);
            per_frame.push(counts);
        }
        Ok(RaeHoldoutReport {
            frames: frames.len(),
            totals,
            per_frame,
        })
    }
}

fn validate_dimensions(features: usize, experts: usize) -> Result<(), RaeLearnError> {
    if features == 0
        || features > SBG_RAE_MAX_EXPERTS
        || experts == 0
        || experts > SBG_RAE_MAX_EXPERTS
    {
        return Err(RaeLearnError::InvalidDimensions);
    }
    Ok(())
}

fn validate_identity(identity: &str) -> Result<(), RaeLearnError> {
    if identity.is_empty() || identity.len() > 128 || identity.trim() != identity {
        return Err(RaeLearnError::InvalidDatasetIdentity);
    }
    Ok(())
}

fn validate_words(words: &[u64], bits: usize) -> Result<(), ()> {
    if words.len() != bits.div_ceil(64) {
        return Err(());
    }
    if bits % 64 != 0 && words[bits.div_ceil(64) - 1] >> (bits % 64) != 0 {
        return Err(());
    }
    Ok(())
}

fn validate_frame(
    frame: &SbgRaeNativeFrame,
    feature_width: usize,
    expert_count: usize,
    index: usize,
) -> Result<(), RaeLearnError> {
    let malformed = || RaeLearnError::MalformedFrame { index };
    for words in [&frame.routing_words, &frame.rising_words, &frame.falling_words] {
        validate_words(words, feature_width).map_err(|()| malformed())?;
    }
    validate_words(&frame.required_pages_words, expert_count).map_err(|()| malformed())?;
    if frame.selected_experts.is_empty() {
        return Err(malformed());
    }
    let mut rebuilt = vec![0_u64; expert_count.div_ceil(64)];
    for page in &frame.selected_experts {
        let index = page.0 as usize;
        if index >= expert_count {
            return Err(malformed());
        }
        let word = &mut rebuilt[index / 64];
        let mask = 1_u64 << (index % 64);
        if *word & mask != 0 {
            return Err(malformed());
        }
        *word |= mask;
    }
    if rebuilt != frame.required_pages_words {
        return Err(malformed());
    }
    let mut rising_count = 0_u64;
    let mut falling_count = 0_u64;
    for ((&state, &rise), &fall) in frame
        .routing_words
        .iter()
        .zip(&frame.rising_words)
        .zip(&frame.falling_words)
    {
        if rise & !state != 0 || fall & state != 0 || rise & fall != 0 {
            return Err(malformed());
        }
        rising_count += u64::from(rise.count_ones());
        falling_count += u64::from(fall.count_ones());
    }
    if rising_count != frame.rising_routing_bits
        || falling_count != frame.falling_routing_bits
        || u64::from(
            frame
                .routing_words
                .iter()
                .map(|word| word.count_ones())
                .sum::<u32>(),
        ) != frame.active_routing_bits
    {
        return Err(malformed());
    }
    Ok(())
}

/// Confirm that adjacent native snapshots truly contain the declared
/// 0 -> 1 / 1 -> 0 transitions. The first frame remains relative to its
/// collector-owned baseline, which must be verified by the trace producer.
fn validate_transition(
    previous: &SbgRaeNativeFrame,
    current: &SbgRaeNativeFrame,
    index: usize,
) -> Result<(), RaeLearnError> {
    if current.cycle <= previous.cycle {
        return Err(RaeLearnError::MalformedFrame { index });
    }
    for (((&old, &new), &rise), &fall) in previous
        .routing_words
        .iter()
        .zip(&current.routing_words)
        .zip(&current.rising_words)
        .zip(&current.falling_words)
    {
        if rise != (new & !old) || fall != (old & !new) {
            return Err(RaeLearnError::MalformedFrame { index });
        }
    }
    Ok(())
}

fn set_indices(words: &[u64]) -> impl Iterator<Item = usize> + '_ {
    words
        .iter()
        .copied()
        .enumerate()
        .flat_map(|(word_index, word)| {
            let mut remaining = word;
            core::iter::from_fn(move || {
                if remaining == 0 {
                    return None;
                }
                let bit = remaining.trailing_zeros() as usize;
                remaining &= remaining - 1;
                Some(word_index * 64 + bit)
            })
        })
}

fn count_page_set_errors(selected: &[u64], required: &[u64]) -> RaeHoldoutCounts {
    let mut result = RaeHoldoutCounts::default();
    for (&prediction, &truth) in selected.iter().zip(required) {
        result.selected_pages += u128::from(prediction.count_ones());
        result.required_pages += u128::from(truth.count_ones());
        result.missed_required_pages += u128::from((truth & !prediction).count_ones());
        result.extra_selected_pages += u128::from((prediction & !truth).count_ones());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(
        cycle: u64,
        active: u64,
        rising: u64,
        falling: u64,
        required: u64,
    ) -> SbgRaeNativeFrame {
        let selected_experts = (0..2)
            .filter(|bit| required & (1_u64 << *bit) != 0)
            .map(PageId)
            .collect();
        SbgRaeNativeFrame {
            cycle,
            routing_words: vec![active],
            rising_words: vec![rising],
            falling_words: vec![falling],
            required_pages_words: vec![required],
            selected_experts,
            rising_routing_bits: u64::from(rising.count_ones()),
            falling_routing_bits: u64::from(falling.count_ones()),
            active_routing_bits: u64::from(active.count_ones()),
            context_invalidated: false,
        }
    }

    fn policy() -> RaeTrainPolicy {
        RaeTrainPolicy {
            minimum_exposures: 2,
            minimum_precision_ppm: 800_000,
            max_associations: 32,
        }
    }

    #[test]
    fn learns_frozen_hard_event_to_page_and_detects_held_high_miss() {
        let train = [
            frame(1, 1, 1, 0, 0b10),
            frame(2, 0, 0, 1, 0b01),
            frame(3, 1, 1, 0, 0b10),
            frame(4, 0, 0, 1, 0b01),
            frame(5, 1, 1, 0, 0b10),
            frame(6, 0, 0, 1, 0b01),
            frame(7, 2, 2, 0, 0b01),
        ];
        let learned = RaeTrainedEventRouter::train::<8>("train-A", 2, policy(), &train).unwrap();
        assert_eq!(learned.learned_feature_count(), 1);
        assert_eq!(learned.association(0).unwrap().page_id, PageId(1));
        assert_eq!(learned.predict(&[0b01]).unwrap(), vec![0b10]);
        let holdout = [frame(8, 1, 1, 0, 0b10), frame(9, 1, 0, 0, 0b10)];
        let report = learned.evaluate("holdout-A", &holdout).unwrap();
        assert_eq!(report.frames, 2);
        assert_eq!(report.totals.required_pages, 2);
        assert_eq!(report.totals.selected_pages, 1);
        assert_eq!(report.totals.missed_required_pages, 1);
        assert_eq!(report.totals.extra_selected_pages, 0);
    }

    #[test]
    fn exact_vote_ties_resolve_by_original_page_id() {
        let train = [
            frame(1, 1, 1, 0, 0b01),
            frame(2, 0, 0, 1, 0b01),
            frame(3, 1, 1, 0, 0b10),
        ];
        let chosen = RaeTrainedEventRouter::train::<8>(
            "train-tie",
            2,
            RaeTrainPolicy {
                minimum_exposures: 2,
                minimum_precision_ppm: 500_000,
                max_associations: 2,
            },
            &train,
        )
        .unwrap();
        assert_eq!(chosen.association(0).unwrap().page_id, PageId(0));
    }

    #[test]
    fn source_identity_and_max_associations_are_enforced() {
        let train = [frame(1, 3, 3, 0, 0b01)];
        assert_eq!(
            RaeTrainedEventRouter::train::<8>("train", 2, policy(), &train[..0]),
            Err(RaeLearnError::EmptyFrameSet)
        );
        let model = RaeTrainedEventRouter::train::<8>(
            "train",
            2,
            RaeTrainPolicy {
                minimum_exposures: 1,
                minimum_precision_ppm: 0,
                max_associations: 2,
            },
            &train,
        )
        .unwrap();
        assert_eq!(
            model.evaluate("train", &train),
            Err(RaeLearnError::IdenticalTrainingAndEvaluationIdentity)
        );
        assert_eq!(
            RaeTrainedEventRouter::train::<8>(
                "train",
                2,
                RaeTrainPolicy {
                    max_associations: 1,
                    ..policy()
                },
                &train,
            ),
            Err(RaeLearnError::TooManyAssociations)
        );
    }

    #[test]
    fn malformed_event_masks_and_inconsistent_mor_masks_are_refused() {
        let mut invalid = frame(1, 1, 1, 0, 0b10);
        invalid.rising_words[0] = 0b10;
        assert_eq!(
            RaeTrainedEventRouter::train::<8>("t", 2, policy(), &[invalid]),
            Err(RaeLearnError::MalformedFrame { index: 0 })
        );
        let mut inconsistent = frame(1, 1, 1, 0, 0b10);
        inconsistent.required_pages_words[0] = 0b01;
        assert_eq!(
            RaeTrainedEventRouter::train::<8>("t", 2, policy(), &[inconsistent]),
            Err(RaeLearnError::MalformedFrame { index: 0 })
        );
    }
    #[test]
    fn genuine_sbg_mor_trace_trains_a_frozen_event_route_and_exposes_hold_miss() {
        use crate::{
            mor::{MixtureOfRouters, MorPolicy, PreviousRoute, RouterId},
            mor_registry::{MorRegistryPolicy, MorRouterRegistry, RouterSpecialization},
            semantic::{
                rae_native_trace::SbgRaeNativeTrace,
                rising_edge::SbgNativeRouteContext,
                SemanticBitField, SemanticExpertSignature, SemanticMorBridge, SemanticMorTopK,
                SemanticRouterSignature, SemanticState,
            },
        };

        let registry = MorRouterRegistry::new(
            vec![
                RouterSpecialization {
                    router_id: RouterId(0),
                    experts: vec![PageId(0)],
                },
                RouterSpecialization {
                    router_id: RouterId(1),
                    experts: vec![PageId(1)],
                },
            ],
            MorRegistryPolicy {
                expert_count: 2,
                max_routers: 2,
                max_edges: 2,
                max_fanout: 1,
                require_full_coverage: true,
            },
        )
        .unwrap();
        let bridge = SemanticMorBridge::new(
            vec![
                SemanticRouterSignature {
                    router_id: RouterId(0),
                    signature: SemanticBitField::<8>::zeroed().unwrap(),
                },
                SemanticRouterSignature {
                    router_id: RouterId(1),
                    signature: SemanticBitField::<8>::ones().unwrap(),
                },
            ],
            vec![
                SemanticExpertSignature {
                    page_id: PageId(0),
                    signature: SemanticBitField::<8>::zeroed().unwrap(),
                },
                SemanticExpertSignature {
                    page_id: PageId(1),
                    signature: SemanticBitField::<8>::ones().unwrap(),
                },
            ],
            SemanticMorTopK::One,
        )
        .unwrap();
        let engine = MixtureOfRouters::new(MorPolicy::default()).unwrap();
        let baseline = SemanticState::<8>::zeroed().unwrap();
        let mut trace = SbgRaeNativeTrace::new(&baseline, 0, 2).unwrap();

        // Actual SBG->MOR is invoked for every source state, including
        // the final held-high cycle with no new rising front.
        for (index, high) in [true, false, true, false, true, false, true, true]
            .into_iter()
            .enumerate()
        {
            let mut state = baseline.clone();
            if high {
                state.routing = SemanticBitField::ones().unwrap();
            }
            trace
                .capture(
                    (index + 1) as u64,
                    &state,
                    SbgNativeRouteContext {
                        bridge: &bridge,
                        registry: &registry,
                        engine: &engine,
                        runtime_hints: &[],
                        previous_route: &PreviousRoute::default(),
                    },
                    false,
                )
                .unwrap();
        }

        assert_eq!(trace.frames().len(), 8);
        assert_eq!(trace.frames()[6].required_pages_words, vec![0b10]);
        assert_eq!(trace.frames()[6].rising_words, vec![0xff]);
        assert_eq!(trace.frames()[7].required_pages_words, vec![0b10]);
        assert_eq!(trace.frames()[7].rising_words, vec![0]);

        let model = RaeTrainedEventRouter::train::<8>(
            "synthetic-native-train",
            2,
            RaeTrainPolicy {
                minimum_exposures: 3,
                minimum_precision_ppm: RAE_LEARNING_PPM,
                max_associations: 16,
            },
            &trace.frames()[..6],
        )
        .unwrap();
        assert_eq!(model.learned_feature_count(), 8);
        let heldout = model
            .evaluate("synthetic-native-holdout", &trace.frames()[6..])
            .unwrap();
        assert_eq!(heldout.totals.required_pages, 2);
        assert_eq!(heldout.totals.selected_pages, 1);
        assert_eq!(heldout.totals.missed_required_pages, 1);
    }

}
