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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainingAccounting {
    pub same_pairs: usize,
    pub different_pairs: usize,
    pub observed_feature_comparisons: usize,
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
