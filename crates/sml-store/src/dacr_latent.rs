//! DACR-2A: train-only pairwise selection of discrete SBG address bits.
//!
//! This module returns candidate segment IDs, never verified source evidence.
//! Source-exact answers must be checked independently through CONTEXT-1c / DACR-1.
//! No token attention, memory scan, trained language model, or speed claim.

use std::collections::BTreeSet;

use sml_core::{
    context_ledger::{ContextSegmentId, ContextStreamId},
    semantic::SemanticBitField,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SbgAddress<const BITS: usize> {
    pub value: SemanticBitField<BITS>,
    /// One = known. Unknown value bits must not be treated as false.
    pub known: SemanticBitField<BITS>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairConstraint<const BITS: usize> {
    pub left: SbgAddress<BITS>,
    pub right: SbgAddress<BITS>,
    /// Training supervision for same/different conceptual item; not route labels.
    pub same: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AddressBudget {
    pub address_bits: usize,
    pub max_pairs: usize,
    pub max_entries: usize,
    pub max_bucket_entries: usize,
}

impl AddressBudget {
    pub fn validate<const BITS: usize>(self) -> Result<Self, AddressError> {
        if BITS == 0
            || BITS > 64
            || self.address_bits == 0
            || self.address_bits > 8
            || self.address_bits > BITS
            || self.max_pairs == 0
            || self.max_entries == 0
            || self.max_bucket_entries == 0
            || self.max_bucket_entries > self.max_entries
        {
            return Err(AddressError::InvalidBudget);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressError {
    InvalidBudget,
    InsufficientPairs,
    InsufficientSignal,
    InvalidFeature,
    UnknownSelectedFeature,
    WrongStream,
    WrongEpoch,
    InvalidSegment,
    DuplicateSegment,
    IndexFull,
    BucketFull,
    SourceAddressMismatch,
    ProbeBudget,
    CandidateOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainingAccounting {
    pub same_pairs: usize,
    pub different_pairs: usize,
    pub observed_feature_comparisons: usize,
}

/// Opt-in minimum per-coordinate evidence, counting only pairs where the
/// selected feature is known on BOTH sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BalancedPairwisePolicy {
    pub min_same_observed: usize,
    pub min_different_observed: usize,
}

/// Auditable integer evidence for one retained coordinate. Its positive
/// advantage is exactly `numerator / denominator`; no floating-point scoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BalancedFeatureEvidence {
    pub bit: usize,
    pub same_observed: usize,
    pub different_observed: usize,
    pub same_disagreements: usize,
    pub different_disagreements: usize,
    pub advantage_numerator: u128,
    pub advantage_denominator: u128,
}

/// Immutable hard-address encoder. The only inference work is reading the
/// selected known bits: no learned real-valued weights remain after freeze.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscreteAddress<const BITS: usize> {
    features: Vec<usize>,
    budget: AddressBudget,
}

impl<const BITS: usize> DiscreteAddress<BITS> {
    #[must_use]
    pub fn features(&self) -> &[usize] {
        &self.features
    }

    pub fn fixed(features: Vec<usize>, budget: AddressBudget) -> Result<Self, AddressError> {
        budget.validate::<BITS>()?;
        if features.len() != budget.address_bits
            || features.iter().any(|&bit| bit >= BITS)
            || features.iter().copied().collect::<BTreeSet<_>>().len() != features.len()
        {
            return Err(AddressError::InvalidFeature);
        }
        Ok(Self { features, budget })
    }

    /// Selects bits with positive discrimination between observed same/different
    /// pairs. This is pairwise supervised feature selection, not task-loss BPTT.
    pub fn train_pairwise(
        pairs: &[PairConstraint<BITS>],
        budget: AddressBudget,
    ) -> Result<(Self, TrainingAccounting), AddressError> {
        budget.validate::<BITS>()?;
        if pairs.is_empty() || pairs.len() > budget.max_pairs {
            return Err(AddressError::InvalidBudget);
        }
        let mut scores = vec![0_i64; BITS];
        let mut accounting = TrainingAccounting {
            same_pairs: 0,
            different_pairs: 0,
            observed_feature_comparisons: 0,
        };
        for pair in pairs {
            if pair.same {
                accounting.same_pairs += 1;
            } else {
                accounting.different_pairs += 1;
            }
            for (bit, score) in scores.iter_mut().enumerate() {
                if pair
                    .left
                    .known
                    .get(bit)
                    .map_err(|_| AddressError::InvalidFeature)?
                    && pair
                        .right
                        .known
                        .get(bit)
                        .map_err(|_| AddressError::InvalidFeature)?
                {
                    accounting.observed_feature_comparisons += 1;
                    let differs = pair
                        .left
                        .value
                        .get(bit)
                        .map_err(|_| AddressError::InvalidFeature)?
                        != pair
                            .right
                            .value
                            .get(bit)
                            .map_err(|_| AddressError::InvalidFeature)?;
                    if differs {
                        *score += if pair.same { -1 } else { 1 };
                    }
                }
            }
        }
        if accounting.same_pairs == 0 || accounting.different_pairs == 0 {
            return Err(AddressError::InsufficientPairs);
        }
        let mut candidates: Vec<usize> = (0..BITS).filter(|&bit| scores[bit] > 0).collect();
        candidates.sort_unstable_by_key(|&bit| (std::cmp::Reverse(scores[bit]), bit));
        if candidates.len() < budget.address_bits {
            return Err(AddressError::InsufficientSignal);
        }
        candidates.truncate(budget.address_bits);
        candidates.sort_unstable();
        Ok((
            Self {
                features: candidates,
                budget,
            },
            accounting,
        ))
    }

    /// Class-balanced feature learning with explicit known-mask support.
    ///
    /// Retains only bits for which the fraction of *different*-pair
    /// disagreements exceeds that of *same*-pair disagreements, using exact
    /// bounded integer ratios and a deterministic tie break. This is an
    /// opt-in alternative to the unchanged raw-count learner above.
    pub fn train_pairwise_balanced(
        pairs: &[PairConstraint<BITS>],
        budget: AddressBudget,
        policy: BalancedPairwisePolicy,
    ) -> Result<(Self, TrainingAccounting, Vec<BalancedFeatureEvidence>), AddressError> {
        budget.validate::<BITS>()?;
        if budget.max_pairs > 1_000_000
            || pairs.is_empty()
            || pairs.len() > budget.max_pairs
            || policy.min_same_observed == 0
            || policy.min_different_observed == 0
        {
            return Err(AddressError::InvalidBudget);
        }
        #[derive(Clone, Copy, Default)]
        struct Observations {
            same_seen: usize,
            different_seen: usize,
            same_disagreed: usize,
            different_disagreed: usize,
        }
        let mut observed = vec![Observations::default(); BITS];
        let mut accounting = TrainingAccounting {
            same_pairs: 0,
            different_pairs: 0,
            observed_feature_comparisons: 0,
        };
        for pair in pairs {
            if pair.same {
                accounting.same_pairs += 1;
            } else {
                accounting.different_pairs += 1;
            }
            for (bit, counts) in observed.iter_mut().enumerate() {
                if !pair
                    .left
                    .known
                    .get(bit)
                    .map_err(|_| AddressError::InvalidFeature)?
                    || !pair
                        .right
                        .known
                        .get(bit)
                        .map_err(|_| AddressError::InvalidFeature)?
                {
                    continue;
                }
                accounting.observed_feature_comparisons += 1;
                let disagreed = pair
                    .left
                    .value
                    .get(bit)
                    .map_err(|_| AddressError::InvalidFeature)?
                    != pair
                        .right
                        .value
                        .get(bit)
                        .map_err(|_| AddressError::InvalidFeature)?;
                if pair.same {
                    counts.same_seen += 1;
                    counts.same_disagreed += usize::from(disagreed);
                } else {
                    counts.different_seen += 1;
                    counts.different_disagreed += usize::from(disagreed);
                }
            }
        }
        if accounting.same_pairs == 0 || accounting.different_pairs == 0 {
            return Err(AddressError::InsufficientPairs);
        }
        let mut eligible = Vec::new();
        for (bit, counts) in observed.into_iter().enumerate() {
            if counts.same_seen < policy.min_same_observed
                || counts.different_seen < policy.min_different_observed
            {
                continue;
            }
            let positive = (counts.different_disagreed as u128) * (counts.same_seen as u128);
            let negative = (counts.same_disagreed as u128) * (counts.different_seen as u128);
            if positive <= negative {
                continue;
            }
            eligible.push(BalancedFeatureEvidence {
                bit,
                same_observed: counts.same_seen,
                different_observed: counts.different_seen,
                same_disagreements: counts.same_disagreed,
                different_disagreements: counts.different_disagreed,
                advantage_numerator: positive - negative,
                advantage_denominator: (counts.same_seen as u128) * (counts.different_seen as u128),
            });
        }
        eligible.sort_unstable_by(|left, right| {
            let left_rate = left.advantage_numerator * right.advantage_denominator;
            let right_rate = right.advantage_numerator * left.advantage_denominator;
            right_rate
                .cmp(&left_rate)
                .then_with(|| left.bit.cmp(&right.bit))
        });
        if eligible.len() < budget.address_bits {
            return Err(AddressError::InsufficientSignal);
        }
        eligible.truncate(budget.address_bits);
        eligible.sort_unstable_by_key(|entry| entry.bit);
        let features = eligible.iter().map(|entry| entry.bit).collect();
        Ok((Self { features, budget }, accounting, eligible))
    }

    pub fn encode(&self, sbg: &SbgAddress<BITS>) -> Result<u16, AddressError> {
        let mut code = 0_u16;
        for (position, &bit) in self.features.iter().enumerate() {
            if !sbg
                .known
                .get(bit)
                .map_err(|_| AddressError::InvalidFeature)?
            {
                return Err(AddressError::UnknownSelectedFeature);
            }
            if sbg
                .value
                .get(bit)
                .map_err(|_| AddressError::InvalidFeature)?
            {
                code |= 1_u16 << position;
            }
        }
        Ok(code)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CandidateAccounting {
    pub address_buckets_probed: u64,
    pub candidates_returned: u64,
}

/// This is an unverified candidate index and NEVER certifies source truth.
#[derive(Debug)]
pub struct CandidateIndex<const BITS: usize> {
    encoder: DiscreteAddress<BITS>,
    stream: ContextStreamId,
    epoch: u64,
    entries: BTreeSet<ContextSegmentId>,
    /// Exactly 2^address_bits slots. Address decoding is a direct offset.
    buckets: Vec<Vec<ContextSegmentId>>,
}

impl<const BITS: usize> CandidateIndex<BITS> {
    pub fn new(
        encoder: DiscreteAddress<BITS>,
        stream: ContextStreamId,
        epoch: u64,
    ) -> Result<Self, AddressError> {
        if epoch == 0 {
            return Err(AddressError::WrongEpoch);
        }
        let address_slots = 1_usize << encoder.budget.address_bits;
        Ok(Self {
            encoder,
            stream,
            epoch,
            entries: BTreeSet::new(),
            buckets: vec![Vec::new(); address_slots],
        })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Number of preallocated direct-address slots, including empty ones.
    #[must_use]
    pub fn address_slot_count(&self) -> usize {
        self.buckets.len()
    }

    /// Frozen maximum admitted source IDs, independent of current index size.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.encoder.budget.max_entries
    }

    /// Epoch of the currently published segment-to-address bindings.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn insert(
        &mut self,
        segment: ContextSegmentId,
        key: &SbgAddress<BITS>,
    ) -> Result<(), AddressError> {
        if segment.stream_id != self.stream {
            return Err(AddressError::WrongStream);
        }
        if segment.segment_id == 0 {
            return Err(AddressError::InvalidSegment);
        }
        let address = self.encoder.encode(key)?;
        if self.entries.contains(&segment) {
            return Err(AddressError::DuplicateSegment);
        }
        if self.entries.len() >= self.encoder.budget.max_entries {
            return Err(AddressError::IndexFull);
        }
        if self.buckets[address as usize].len() >= self.encoder.budget.max_bucket_entries {
            return Err(AddressError::BucketFull);
        }
        self.entries.insert(segment);
        self.buckets[address as usize].push(segment);
        Ok(())
    }

    /// Atomically reassigns an existing candidate to a new SBG address and
    /// publishes the immediate successor epoch.
    ///
    /// All validation precedes mutation: wrong source, stale epoch, unknown
    /// selected bits, incorrect old address and full destination buckets leave
    /// all source bindings untouched. Reading still requires exact epoch
    /// admission and never upgrades candidates to verified source evidence.
    pub fn rebind(
        &mut self,
        segment: ContextSegmentId,
        expected_epoch: u64,
        next_epoch: u64,
        old_key: &SbgAddress<BITS>,
        new_key: &SbgAddress<BITS>,
    ) -> Result<(), AddressError> {
        if segment.stream_id != self.stream {
            return Err(AddressError::WrongStream);
        }
        if expected_epoch != self.epoch || self.epoch.checked_add(1) != Some(next_epoch) {
            return Err(AddressError::WrongEpoch);
        }
        if segment.segment_id == 0 || !self.entries.contains(&segment) {
            return Err(AddressError::InvalidSegment);
        }
        let old_address = self.encoder.encode(old_key)?;
        let new_address = self.encoder.encode(new_key)?;
        let old_position = self.buckets[old_address as usize]
            .iter()
            .position(|&candidate| candidate == segment)
            .ok_or(AddressError::SourceAddressMismatch)?;
        if old_address != new_address {
            if self.buckets[new_address as usize].len() >= self.encoder.budget.max_bucket_entries {
                return Err(AddressError::BucketFull);
            }
            self.buckets[old_address as usize].remove(old_position);
            self.buckets[new_address as usize].push(segment);
        }
        self.epoch = next_epoch;
        Ok(())
    }

    /// Returns only candidate IDs. Any claimed fact requires independent raw
    /// source read, integrity verification, and conflict disambiguation.
    pub fn lookup(
        &self,
        stream: ContextStreamId,
        epoch: u64,
        key: &SbgAddress<BITS>,
    ) -> Result<(Vec<ContextSegmentId>, CandidateAccounting), AddressError> {
        if stream != self.stream {
            return Err(AddressError::WrongStream);
        }
        if epoch != self.epoch {
            return Err(AddressError::WrongEpoch);
        }
        let address = self.encoder.encode(key)?;
        let hits = self.buckets[address as usize].clone();
        let accounting = CandidateAccounting {
            address_buckets_probed: 1,
            candidates_returned: hits.len() as u64,
        };
        Ok((hits, accounting))
    }

    /// Opt-in direct probes over BOTH possible values of each unknown selected
    /// SBG bit. The default exact lookup continues to reject unknown bits.
    ///
    /// No history scan, approximate scoring or implicit unknown->false coercion.
    /// All caps are preflighted; oversized candidate unions are never truncated.
    pub fn lookup_partial(
        &self,
        stream: ContextStreamId,
        epoch: u64,
        key: &SbgAddress<BITS>,
        max_unknown_bits: usize,
        max_address_probes: usize,
        max_candidates: usize,
    ) -> Result<(Vec<ContextSegmentId>, CandidateAccounting), AddressError> {
        if stream != self.stream {
            return Err(AddressError::WrongStream);
        }
        if epoch != self.epoch {
            return Err(AddressError::WrongEpoch);
        }
        if max_unknown_bits > self.encoder.budget.address_bits
            || !(1..=256).contains(&max_address_probes)
            || max_candidates == 0
            || max_candidates > self.encoder.budget.max_entries
        {
            return Err(AddressError::ProbeBudget);
        }
        let mut base = 0_u16;
        let mut unknown_positions = Vec::new();
        for (address_position, &bit) in self.encoder.features.iter().enumerate() {
            if key
                .known
                .get(bit)
                .map_err(|_| AddressError::InvalidFeature)?
            {
                if key
                    .value
                    .get(bit)
                    .map_err(|_| AddressError::InvalidFeature)?
                {
                    base |= 1_u16 << address_position;
                }
            } else {
                unknown_positions.push(address_position);
            }
        }
        if unknown_positions.len() > max_unknown_bits {
            return Err(AddressError::UnknownSelectedFeature);
        }
        // address_bits <= 8 by construction, so the maximum is 256.
        let probe_count = 1_usize << unknown_positions.len();
        if probe_count > max_address_probes {
            return Err(AddressError::ProbeBudget);
        }
        let mut hits = Vec::new();
        for assignment in 0..probe_count {
            let mut address = base;
            for (index, &position) in unknown_positions.iter().enumerate() {
                if assignment & (1_usize << index) != 0 {
                    address |= 1_u16 << position;
                }
            }
            let bucket = &self.buckets[address as usize];
            if bucket.len() > max_candidates - hits.len() {
                return Err(AddressError::CandidateOverflow);
            }
            hits.extend_from_slice(bucket);
        }
        let accounting = CandidateAccounting {
            address_buckets_probed: probe_count as u64,
            candidates_returned: hits.len() as u64,
        };
        Ok((hits, accounting))
    }
}

#[cfg(test)]
mod dacr_latent_tests {
    use super::*;

    const BITS: usize = 16;
    const CLASSES: usize = 8;
    const CLASS_BITS: [usize; 3] = [4, 6, 7];
    const NOISE_BITS: [usize; 5] = [0, 1, 2, 3, 5];

    fn sample(class: usize, noise: usize) -> SbgAddress<BITS> {
        let mut word = 0_u64;
        for (i, &bit) in CLASS_BITS.iter().enumerate() {
            if class & (1 << i) != 0 {
                word |= 1 << bit;
            }
        }
        for (i, &bit) in NOISE_BITS.iter().enumerate() {
            if noise & (1 << i) != 0 {
                word |= 1 << bit;
            }
        }
        SbgAddress {
            value: SemanticBitField::from_words(vec![word]).unwrap(),
            known: SemanticBitField::ones().unwrap(),
        }
    }

    fn budget() -> AddressBudget {
        AddressBudget {
            address_bits: 3,
            max_pairs: 32,
            max_entries: 8,
            max_bucket_entries: 8,
        }
    }

    fn pairs() -> Vec<PairConstraint<BITS>> {
        let mut constraints = Vec::new();
        for class in 0..CLASSES {
            constraints.push(PairConstraint {
                left: sample(class, 0),
                right: sample(class, 31),
                same: true,
            });
            for bit in 0..3 {
                constraints.push(PairConstraint {
                    left: sample(class, 10),
                    right: sample(class ^ (1 << bit), 10),
                    same: false,
                });
            }
        }
        constraints
    }

    fn id(stream: ContextStreamId, n: u64) -> ContextSegmentId {
        ContextSegmentId::new(stream, n).unwrap()
    }

    #[test]
    fn dacr_latent_learns_signal_and_generalizes_heldout_nuisance() {
        let (learned, accounting) = DiscreteAddress::train_pairwise(&pairs(), budget()).unwrap();
        assert_eq!(learned.features(), &CLASS_BITS);
        assert_eq!(accounting.same_pairs, 8);
        assert_eq!(accounting.different_pairs, 24);
        let fixed = DiscreteAddress::fixed(vec![0, 1, 2], budget()).unwrap();
        let stream = ContextStreamId::new(91).unwrap();
        let mut learned_index = CandidateIndex::new(learned, stream, 3).unwrap();
        let mut fixed_index = CandidateIndex::new(fixed, stream, 3).unwrap();
        for class in 0..CLASSES {
            let key = sample(class, 0b01001);
            learned_index
                .insert(id(stream, (class + 1) as u64), &key)
                .unwrap();
            fixed_index
                .insert(id(stream, (class + 1) as u64), &key)
                .unwrap();
        }
        let mut learned_recall = 0;
        let mut fixed_recall = 0;
        for class in 0..CLASSES {
            let query = sample(class, 0b10110);
            let expected = id(stream, (class + 1) as u64);
            let (hits, accounting) = learned_index.lookup(stream, 3, &query).unwrap();
            assert_eq!(accounting.address_buckets_probed, 1);
            assert!(accounting.candidates_returned <= 8);
            learned_recall += usize::from(hits.contains(&expected));
            fixed_recall += usize::from(
                fixed_index
                    .lookup(stream, 3, &query)
                    .unwrap()
                    .0
                    .contains(&expected),
            );
        }
        assert_eq!(learned_recall, 8);
        assert!(fixed_recall < 8);
    }

    #[test]
    fn dacr_latent_refuses_unknown_selected_feature() {
        let (encoder, _) = DiscreteAddress::train_pairwise(&pairs(), budget()).unwrap();
        let mut key = sample(1, 2);
        key.known.set(4, false).unwrap();
        assert!(key.value.get(4).unwrap()); // unknown != known false
        assert_eq!(
            encoder.encode(&key),
            Err(AddressError::UnknownSelectedFeature)
        );
        let stream = ContextStreamId::new(92).unwrap();
        let mut index = CandidateIndex::new(encoder, stream, 1).unwrap();
        assert_eq!(
            index.insert(id(stream, 1), &key),
            Err(AddressError::UnknownSelectedFeature)
        );
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn dacr_latent_collision_and_atomic_budgets() {
        let stream = ContextStreamId::new(93).unwrap();
        let mut caps = budget();
        caps.address_bits = 1;
        caps.max_bucket_entries = 2;
        let encoder = DiscreteAddress::fixed(vec![0], caps).unwrap();
        let mut index = CandidateIndex::new(encoder, stream, 1).unwrap();
        index.insert(id(stream, 1), &sample(0, 0)).unwrap();
        index.insert(id(stream, 2), &sample(4, 0)).unwrap();
        assert_eq!(
            index.insert(id(stream, 3), &sample(5, 0)),
            Err(AddressError::BucketFull)
        );
        assert_eq!(index.len(), 2);
        let (candidates, accounting) = index.lookup(stream, 1, &sample(1, 0)).unwrap();
        assert_eq!(candidates, vec![id(stream, 1), id(stream, 2)]);
        assert_eq!(accounting.candidates_returned, 2);
    }

    #[test]
    fn dacr_latent_duplicate_epoch_stream_and_invalid_budget() {
        let stream = ContextStreamId::new(94).unwrap();
        let other = ContextStreamId::new(95).unwrap();
        let encoder = DiscreteAddress::fixed(vec![0, 1, 2], budget()).unwrap();
        let mut index = CandidateIndex::new(encoder, stream, 2).unwrap();
        let key = sample(0, 0);
        index.insert(id(stream, 1), &key).unwrap();
        assert_eq!(
            index.insert(id(stream, 1), &key),
            Err(AddressError::DuplicateSegment)
        );
        assert_eq!(
            index.insert(id(other, 1), &key),
            Err(AddressError::WrongStream)
        );
        assert_eq!(index.lookup(stream, 3, &key), Err(AddressError::WrongEpoch));
        assert_eq!(index.lookup(other, 2, &key), Err(AddressError::WrongStream));
        assert_eq!(index.len(), 1);
        let mut bad = budget();
        bad.address_bits = 0;
        assert_eq!(bad.validate::<BITS>(), Err(AddressError::InvalidBudget));
        assert_eq!(budget().validate::<65>(), Err(AddressError::InvalidBudget));
        assert_eq!(
            DiscreteAddress::<BITS>::train_pairwise(&[], budget()).err(),
            Some(AddressError::InvalidBudget)
        );
    }

    #[test]
    fn dacr_latent_insufficient_pairs_or_signal_and_determinism() {
        let data = pairs();
        let first = DiscreteAddress::train_pairwise(&data, budget()).unwrap();
        let second = DiscreteAddress::train_pairwise(&data, budget()).unwrap();
        assert_eq!(first, second);
        let only_same = data.into_iter().filter(|p| p.same).collect::<Vec<_>>();
        assert_eq!(
            DiscreteAddress::train_pairwise(&only_same, budget()).err(),
            Some(AddressError::InsufficientPairs)
        );
        let no_signal = vec![
            PairConstraint {
                left: sample(0, 0),
                right: sample(0, 0),
                same: true,
            },
            PairConstraint {
                left: sample(0, 0),
                right: sample(0, 0),
                same: false,
            },
        ];
        assert_eq!(
            DiscreteAddress::train_pairwise(&no_signal, budget()).err(),
            Some(AddressError::InsufficientSignal)
        );
    }
}

#[cfg(test)]
mod dacr_dense_address_tests {
    use super::*;
    use std::collections::BTreeMap;

    fn sbg(address: u16) -> SbgAddress<16> {
        SbgAddress {
            value: SemanticBitField::<16>::from_words(vec![u64::from(address)]).unwrap(),
            known: SemanticBitField::<16>::ones().unwrap(),
        }
    }

    fn budget(bits: usize, entries: usize, bucket: usize) -> AddressBudget {
        AddressBudget {
            address_bits: bits,
            max_pairs: 1,
            max_entries: entries,
            max_bucket_entries: bucket,
        }
    }

    #[test]
    fn dacr_dense_256_addresses_match_independent_map_oracle() {
        let stream = ContextStreamId::new(127).unwrap();
        let encoder = DiscreteAddress::fixed((0..8).collect(), budget(8, 256, 1)).unwrap();
        let mut index = CandidateIndex::new(encoder, stream, 19).unwrap();
        let mut oracle = BTreeMap::<u16, Vec<ContextSegmentId>>::new();
        assert_eq!(index.address_slot_count(), 256);
        assert_eq!(index.len(), 0);
        for address in (0..256_u16).rev() {
            let segment = ContextSegmentId::new(stream, u64::from(address) + 1).unwrap();
            index.insert(segment, &sbg(address)).unwrap();
            oracle.entry(address).or_default().push(segment);
        }
        assert_eq!(index.len(), 256);
        assert!(!index.is_empty());
        for address in 0..256_u16 {
            let expected = oracle.get(&address).unwrap();
            let (candidates, counters) = index.lookup(stream, 19, &sbg(address)).unwrap();
            assert_eq!(&candidates, expected);
            assert_eq!(counters.address_buckets_probed, 1);
            assert_eq!(counters.candidates_returned, 1);
            assert_eq!(
                index.lookup(stream, 19, &sbg(address)).unwrap(),
                (candidates, counters)
            );
        }
        let duplicate = ContextSegmentId::new(stream, 1).unwrap();
        assert_eq!(
            index.insert(duplicate, &sbg(4)),
            Err(AddressError::DuplicateSegment)
        );
        assert_eq!(
            index.insert(ContextSegmentId::new(stream, 257).unwrap(), &sbg(0)),
            Err(AddressError::IndexFull)
        );
        assert_eq!(index.len(), 256);
        assert_eq!(index.address_slot_count(), 256);
    }

    #[test]
    fn dacr_dense_bounded_collisions_retain_insertion_order() {
        let stream = ContextStreamId::new(128).unwrap();
        let encoder = DiscreteAddress::fixed(vec![0, 1], budget(2, 3, 2)).unwrap();
        let mut index = CandidateIndex::new(encoder, stream, 21).unwrap();
        assert_eq!(index.address_slot_count(), 4);
        let first = ContextSegmentId::new(stream, 1).unwrap();
        let second = ContextSegmentId::new(stream, 2).unwrap();
        let third = ContextSegmentId::new(stream, 3).unwrap();
        let fourth = ContextSegmentId::new(stream, 4).unwrap();
        index.insert(first, &sbg(0)).unwrap();
        index.insert(second, &sbg(0)).unwrap();
        assert_eq!(index.insert(third, &sbg(0)), Err(AddressError::BucketFull));
        assert_eq!(index.len(), 2);
        assert_eq!(
            index.lookup(stream, 21, &sbg(0)).unwrap().0,
            vec![first, second]
        );
        index.insert(third, &sbg(1)).unwrap();
        assert_eq!(index.insert(fourth, &sbg(2)), Err(AddressError::IndexFull));
        assert_eq!(index.len(), 3);
        assert_eq!(
            index.lookup(stream, 21, &sbg(0)).unwrap().0,
            vec![first, second]
        );
        let (empty, counters) = index.lookup(stream, 21, &sbg(3)).unwrap();
        assert!(empty.is_empty());
        assert_eq!(counters.address_buckets_probed, 1);
        assert_eq!(counters.candidates_returned, 0);
    }

    #[test]
    fn dacr_dense_unknown_stream_epoch_and_invalid_budgets_fail_closed() {
        let stream = ContextStreamId::new(129).unwrap();
        let other = ContextStreamId::new(130).unwrap();
        let encoder = DiscreteAddress::fixed(vec![0, 1], budget(2, 2, 2)).unwrap();
        let mut index = CandidateIndex::new(encoder, stream, 4).unwrap();
        index
            .insert(ContextSegmentId::new(stream, 1).unwrap(), &sbg(2))
            .unwrap();
        let mut unknown = sbg(3);
        unknown.known.set(0, false).unwrap();
        assert!(unknown.value.get(0).unwrap());
        assert_eq!(
            index.lookup(stream, 4, &unknown),
            Err(AddressError::UnknownSelectedFeature)
        );
        assert_eq!(
            index.lookup(other, 4, &sbg(2)),
            Err(AddressError::WrongStream)
        );
        assert_eq!(
            index.lookup(stream, 5, &sbg(2)),
            Err(AddressError::WrongEpoch)
        );
        assert_eq!(index.len(), 1);
        assert_eq!(index.address_slot_count(), 4);
        assert_eq!(
            DiscreteAddress::<16>::fixed(vec![], budget(0, 1, 1)).err(),
            Some(AddressError::InvalidBudget)
        );
        assert_eq!(
            DiscreteAddress::<16>::fixed((0..9).collect(), budget(9, 1, 1)).err(),
            Some(AddressError::InvalidBudget)
        );
    }
}

#[cfg(test)]
mod dacr_partial_tests {
    use super::*;

    fn key(code: u8, known_low: bool, known_high: bool) -> SbgAddress<16> {
        let mut value = SemanticBitField::<16>::zeroed().unwrap();
        value.set(4, code & 1 != 0).unwrap();
        value.set(6, code & 2 != 0).unwrap();
        let mut known = SemanticBitField::<16>::ones().unwrap();
        known.set(4, known_low).unwrap();
        known.set(6, known_high).unwrap();
        SbgAddress { value, known }
    }

    fn setup() -> (ContextStreamId, CandidateIndex<16>) {
        let stream = ContextStreamId::new(90).unwrap();
        let budget = AddressBudget {
            address_bits: 2,
            max_pairs: 1,
            max_entries: 4,
            max_bucket_entries: 4,
        };
        let encoder = DiscreteAddress::fixed(vec![4, 6], budget).unwrap();
        let mut index = CandidateIndex::new(encoder, stream, 17).unwrap();
        for code in 0..4_u8 {
            index
                .insert(
                    ContextSegmentId::new(stream, u64::from(code) + 1).unwrap(),
                    &key(code, true, true),
                )
                .unwrap();
        }
        (stream, index)
    }

    #[test]
    fn dacr_partial_one_unknown_enumerates_both_values_without_false_coercion() {
        let (stream, index) = setup();
        let left = key(0, false, true);
        let right = key(1, false, true); // Unknown underlying value is TRUE.
        assert_eq!(
            index.lookup(stream, 17, &left),
            Err(AddressError::UnknownSelectedFeature)
        );
        let expected = vec![
            ContextSegmentId::new(stream, 1).unwrap(),
            ContextSegmentId::new(stream, 2).unwrap(),
        ];
        for key in [&left, &right] {
            let (hits, counters) = index.lookup_partial(stream, 17, key, 1, 2, 2).unwrap();
            assert_eq!(hits, expected);
            assert_eq!(counters.address_buckets_probed, 2);
            assert_eq!(counters.candidates_returned, 2);
            assert_eq!(
                index.lookup_partial(stream, 17, key, 1, 2, 2).unwrap(),
                (hits, counters)
            );
        }
        assert_eq!(
            index
                .lookup_partial(stream, 17, &key(2, true, true), 0, 1, 1)
                .unwrap(),
            (
                vec![ContextSegmentId::new(stream, 3).unwrap()],
                CandidateAccounting {
                    address_buckets_probed: 1,
                    candidates_returned: 1
                }
            )
        );
    }

    #[test]
    fn dacr_partial_two_unknowns_probe_four_and_budget_rejects_without_truncation() {
        let (stream, index) = setup();
        let all_unknown = key(3, false, false);
        let (hits, counters) = index
            .lookup_partial(stream, 17, &all_unknown, 2, 4, 4)
            .unwrap();
        assert_eq!(
            hits,
            (1..=4)
                .map(|n| ContextSegmentId::new(stream, n).unwrap())
                .collect::<Vec<_>>()
        );
        assert_eq!(counters.address_buckets_probed, 4);
        assert_eq!(counters.candidates_returned, 4);
        assert_eq!(
            index.lookup_partial(stream, 17, &all_unknown, 1, 4, 4),
            Err(AddressError::UnknownSelectedFeature)
        );
        assert_eq!(
            index.lookup_partial(stream, 17, &all_unknown, 2, 2, 4),
            Err(AddressError::ProbeBudget)
        );
        assert_eq!(
            index.lookup_partial(stream, 17, &all_unknown, 2, 4, 3),
            Err(AddressError::CandidateOverflow)
        );
        for (unknown, probes, candidates) in
            [(3, 4, 4), (2, 0, 4), (2, 257, 4), (2, 4, 0), (2, 4, 5)]
        {
            assert_eq!(
                index.lookup_partial(stream, 17, &all_unknown, unknown, probes, candidates),
                Err(AddressError::ProbeBudget)
            );
        }
    }

    #[test]
    fn dacr_partial_stream_and_epoch_guards_are_retained() {
        let (stream, index) = setup();
        let query = key(0, false, true);
        let foreign = ContextStreamId::new(91).unwrap();
        assert_eq!(
            index.lookup_partial(foreign, 17, &query, 1, 2, 2),
            Err(AddressError::WrongStream)
        );
        assert_eq!(
            index.lookup_partial(stream, 18, &query, 1, 2, 2),
            Err(AddressError::WrongEpoch)
        );
        assert_eq!(
            index.lookup_partial(stream, 17, &query, 0, 2, 2),
            Err(AddressError::UnknownSelectedFeature)
        );
    }
}

#[cfg(test)]
mod dacr_balanced_tests {
    use super::*;

    const BITS: usize = 16;

    fn sample(bits: u64) -> SbgAddress<BITS> {
        SbgAddress {
            value: SemanticBitField::from_words(vec![bits]).unwrap(),
            known: SemanticBitField::ones().unwrap(),
        }
    }

    fn pair(left: SbgAddress<BITS>, right: SbgAddress<BITS>, same: bool) -> PairConstraint<BITS> {
        PairConstraint { left, right, same }
    }

    fn budget() -> AddressBudget {
        AddressBudget {
            address_bits: 1,
            max_pairs: 36,
            max_entries: 2,
            max_bucket_entries: 2,
        }
    }

    fn policy() -> BalancedPairwisePolicy {
        BalancedPairwisePolicy {
            min_same_observed: 4,
            min_different_observed: 32,
        }
    }

    fn imbalanced_pairs() -> Vec<PairConstraint<BITS>> {
        let mut pairs = Vec::new();
        // Same-label examples always disagree on nuisance coordinate 1.
        for _ in 0..4 {
            pairs.push(pair(sample(0), sample(1 << 1), true));
        }
        // Overrepresented different-label comparisons also disagree on the
        // nuisance, so a raw count incorrectly rewards coordinate 1.
        for _ in 0..24 {
            pairs.push(pair(sample(0), sample(1 << 1), false));
        }
        for _ in 0..4 {
            pairs.push(pair(sample(0), sample(1 << 4), false));
        }
        for _ in 0..4 {
            pairs.push(pair(sample(0), sample(1 << 15), false));
        }
        pairs
    }

    #[test]
    fn dacr_balanced_corrects_imbalanced_pair_signal_with_frozen_holdout() {
        let pairs = imbalanced_pairs();
        assert_eq!(pairs.len(), 36);
        let (legacy, _) = DiscreteAddress::train_pairwise(&pairs, budget()).unwrap();
        assert_eq!(legacy.features(), &[1]);
        let (balanced, accounting, evidence) =
            DiscreteAddress::train_pairwise_balanced(&pairs, budget(), policy()).unwrap();
        assert_eq!(balanced.features(), &[4]);
        assert_eq!(accounting.same_pairs, 4);
        assert_eq!(accounting.different_pairs, 32);
        assert_eq!(accounting.observed_feature_comparisons, 36 * BITS);
        assert_eq!(
            evidence,
            vec![BalancedFeatureEvidence {
                bit: 4,
                same_observed: 4,
                different_observed: 32,
                same_disagreements: 0,
                different_disagreements: 4,
                advantage_numerator: 16,
                advantage_denominator: 128,
            }]
        );
        assert_eq!(
            DiscreteAddress::train_pairwise_balanced(&pairs, budget(), policy()).unwrap(),
            (balanced.clone(), accounting, evidence)
        );
        let stream = ContextStreamId::new(119).unwrap();
        let mut legacy_index = CandidateIndex::new(legacy, stream, 7).unwrap();
        let mut balanced_index = CandidateIndex::new(balanced, stream, 7).unwrap();
        for (class, word) in [0_u64, 1_u64 << 4].into_iter().enumerate() {
            let id = ContextSegmentId::new(stream, (class + 1) as u64).unwrap();
            legacy_index.insert(id, &sample(word)).unwrap();
            balanced_index.insert(id, &sample(word)).unwrap();
        }
        let mut legacy_found = 0;
        let mut balanced_found = 0;
        // TEST nuisance bit 1 = 1, whereas all indexed sources had bit 1 = 0.
        for (class, word) in [1_u64 << 1, (1_u64 << 1) | (1_u64 << 4)]
            .into_iter()
            .enumerate()
        {
            let expected = ContextSegmentId::new(stream, (class + 1) as u64).unwrap();
            legacy_found += usize::from(
                legacy_index
                    .lookup(stream, 7, &sample(word))
                    .unwrap()
                    .0
                    .contains(&expected),
            );
            let (candidates, work) = balanced_index.lookup(stream, 7, &sample(word)).unwrap();
            balanced_found += usize::from(candidates.contains(&expected));
            assert_eq!(candidates, vec![expected]);
            assert_eq!(work.address_buckets_probed, 1);
            assert_eq!(work.candidates_returned, 1);
        }
        assert_eq!(legacy_found, 0);
        assert_eq!(balanced_found, 2);
    }

    #[test]
    fn dacr_balanced_observes_both_known_masks_and_support_floors() {
        let mut hidden_positive_value = sample(1 << 4);
        hidden_positive_value.known.set(4, false).unwrap();
        assert!(hidden_positive_value.value.get(4).unwrap());
        let pairs = vec![
            pair(sample(0), sample(0), true),
            pair(hidden_positive_value, sample(0), true),
            pair(sample(0), sample(1 << 4), false),
        ];
        let caps = AddressBudget {
            max_pairs: 3,
            ..budget()
        };
        assert_eq!(
            DiscreteAddress::train_pairwise_balanced(
                &pairs,
                caps,
                BalancedPairwisePolicy {
                    min_same_observed: 2,
                    min_different_observed: 1,
                },
            )
            .err(),
            Some(AddressError::InsufficientSignal)
        );
        let (model, work, evidence) = DiscreteAddress::train_pairwise_balanced(
            &pairs,
            caps,
            BalancedPairwisePolicy {
                min_same_observed: 1,
                min_different_observed: 1,
            },
        )
        .unwrap();
        assert_eq!(model.features(), &[4]);
        assert_eq!(evidence[0].same_observed, 1);
        assert_eq!(evidence[0].different_observed, 1);
        assert_eq!(evidence[0].same_disagreements, 0);
        assert_eq!(evidence[0].different_disagreements, 1);
        assert_eq!(work.observed_feature_comparisons, 3 * BITS - 1);
        let mut unknown = sample(1 << 4);
        unknown.known.set(4, false).unwrap();
        assert_eq!(
            model.encode(&unknown),
            Err(AddressError::UnknownSelectedFeature)
        );
    }

    #[test]
    fn dacr_balanced_rejects_invalid_or_one_label_protocols() {
        let pairs = imbalanced_pairs();
        let bad_policy = BalancedPairwisePolicy {
            min_same_observed: 0,
            ..policy()
        };
        assert_eq!(
            DiscreteAddress::train_pairwise_balanced(&pairs, budget(), bad_policy).err(),
            Some(AddressError::InvalidBudget)
        );
        let bad_caps = AddressBudget {
            max_pairs: 1_000_001,
            ..budget()
        };
        assert_eq!(
            DiscreteAddress::train_pairwise_balanced(&pairs, bad_caps, policy()).err(),
            Some(AddressError::InvalidBudget)
        );
        let too_few = AddressBudget {
            max_pairs: 35,
            ..budget()
        };
        assert_eq!(
            DiscreteAddress::train_pairwise_balanced(&pairs, too_few, policy()).err(),
            Some(AddressError::InvalidBudget)
        );
        let same = pairs.iter().filter(|p| p.same).cloned().collect::<Vec<_>>();
        assert_eq!(
            DiscreteAddress::train_pairwise_balanced(&same, budget(), policy()).err(),
            Some(AddressError::InsufficientPairs)
        );
        let different = pairs
            .iter()
            .filter(|p| !p.same)
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            DiscreteAddress::train_pairwise_balanced(&different, budget(), policy()).err(),
            Some(AddressError::InsufficientPairs)
        );
    }
}

#[cfg(test)]
mod dacr_rebind_tests {
    use super::*;

    fn key(code: u64) -> SbgAddress<16> {
        SbgAddress {
            value: SemanticBitField::<16>::from_words(vec![code]).unwrap(),
            known: SemanticBitField::<16>::ones().unwrap(),
        }
    }

    fn budget(bucket_entries: usize) -> AddressBudget {
        AddressBudget {
            address_bits: 2,
            max_pairs: 1,
            max_entries: 3,
            max_bucket_entries: bucket_entries,
        }
    }

    fn fixture(epoch: u64, bucket_entries: usize) -> (ContextStreamId, CandidateIndex<16>) {
        let stream = ContextStreamId::new(136).unwrap();
        let encoder = DiscreteAddress::fixed(vec![0, 1], budget(bucket_entries)).unwrap();
        let mut index = CandidateIndex::new(encoder, stream, epoch).unwrap();
        index
            .insert(ContextSegmentId::new(stream, 1).unwrap(), &key(0))
            .unwrap();
        index
            .insert(ContextSegmentId::new(stream, 2).unwrap(), &key(1))
            .unwrap();
        (stream, index)
    }

    #[test]
    fn dacr_rebind_moves_one_segment_and_invalidates_old_epoch() {
        let (stream, mut index) = fixture(7, 2);
        let segment = ContextSegmentId::new(stream, 1).unwrap();
        let other = ContextSegmentId::new(stream, 2).unwrap();
        assert_eq!(index.epoch(), 7);
        assert_eq!(index.len(), 2);
        index.rebind(segment, 7, 8, &key(0), &key(2)).unwrap();
        assert_eq!(index.epoch(), 8);
        assert_eq!(index.len(), 2);
        assert_eq!(
            index.lookup(stream, 7, &key(2)),
            Err(AddressError::WrongEpoch)
        );
        assert!(index.lookup(stream, 8, &key(0)).unwrap().0.is_empty());
        assert_eq!(index.lookup(stream, 8, &key(2)).unwrap().0, vec![segment]);
        assert_eq!(index.lookup(stream, 8, &key(1)).unwrap().0, vec![other]);
        assert_eq!(
            index
                .lookup(stream, 8, &key(2))
                .unwrap()
                .1
                .address_buckets_probed,
            1
        );
        let (_, mut replay) = fixture(7, 2);
        replay.rebind(segment, 7, 8, &key(0), &key(2)).unwrap();
        for code in 0..4 {
            assert_eq!(
                index.lookup(stream, 8, &key(code)),
                replay.lookup(stream, 8, &key(code))
            );
        }
    }

    #[test]
    fn dacr_rebind_refuses_stale_epoch_wrong_address_unknown_bit_and_unregistered_id() {
        let (stream, mut index) = fixture(12, 2);
        let segment = ContextSegmentId::new(stream, 1).unwrap();
        let foreign = ContextSegmentId::new(ContextStreamId::new(137).unwrap(), 1).unwrap();
        let missing = ContextSegmentId::new(stream, 3).unwrap();
        let mut unknown = key(1);
        unknown.known.set(0, false).unwrap();
        assert!(unknown.value.get(0).unwrap());
        let invalid = [
            index.rebind(foreign, 12, 13, &key(0), &key(2)),
            index.rebind(segment, 11, 12, &key(0), &key(2)),
            index.rebind(segment, 12, 12, &key(0), &key(2)),
            index.rebind(segment, 12, 14, &key(0), &key(2)),
            index.rebind(missing, 12, 13, &key(0), &key(2)),
            index.rebind(segment, 12, 13, &key(1), &key(2)),
            index.rebind(segment, 12, 13, &unknown, &key(2)),
            index.rebind(segment, 12, 13, &key(0), &unknown),
        ];
        assert_eq!(
            invalid,
            [
                Err(AddressError::WrongStream),
                Err(AddressError::WrongEpoch),
                Err(AddressError::WrongEpoch),
                Err(AddressError::WrongEpoch),
                Err(AddressError::InvalidSegment),
                Err(AddressError::SourceAddressMismatch),
                Err(AddressError::UnknownSelectedFeature),
                Err(AddressError::UnknownSelectedFeature),
            ]
        );
        assert_eq!(index.epoch(), 12);
        assert_eq!(index.len(), 2);
        assert_eq!(index.lookup(stream, 12, &key(0)).unwrap().0, vec![segment]);
        assert!(index.lookup(stream, 12, &key(2)).unwrap().0.is_empty());
    }

    #[test]
    fn dacr_rebind_destination_full_fails_atomically_but_same_bucket_can_advance_epoch() {
        let (stream, mut index) = fixture(3, 1);
        let segment = ContextSegmentId::new(stream, 1).unwrap();
        let other = ContextSegmentId::new(stream, 2).unwrap();
        assert_eq!(
            index.rebind(segment, 3, 4, &key(0), &key(1)),
            Err(AddressError::BucketFull)
        );
        assert_eq!(index.epoch(), 3);
        assert_eq!(index.lookup(stream, 3, &key(0)).unwrap().0, vec![segment]);
        assert_eq!(index.lookup(stream, 3, &key(1)).unwrap().0, vec![other]);
        index.rebind(segment, 3, 4, &key(0), &key(0)).unwrap();
        assert_eq!(index.epoch(), 4);
        assert_eq!(index.lookup(stream, 4, &key(0)).unwrap().0, vec![segment]);
        assert_eq!(index.lookup(stream, 4, &key(1)).unwrap().0, vec![other]);
        assert_eq!(index.len(), 2);
    }

    #[test]
    fn dacr_rebind_epoch_overflow_rejects_before_mutation() {
        let (stream, mut index) = fixture(u64::MAX, 2);
        let segment = ContextSegmentId::new(stream, 1).unwrap();
        assert_eq!(
            index.rebind(segment, u64::MAX, 0, &key(0), &key(2)),
            Err(AddressError::WrongEpoch)
        );
        assert_eq!(index.epoch(), u64::MAX);
        assert_eq!(
            index.lookup(stream, u64::MAX, &key(0)).unwrap().0,
            vec![segment]
        );
    }
}
