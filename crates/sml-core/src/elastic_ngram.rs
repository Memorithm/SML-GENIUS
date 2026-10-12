//! ENG-0: immutable, bounded variable-context Witten-Bell N-gram reference.
//!
//! At each fixed depth 0..7, the entire 256-way next-byte distribution must
//! match char_baselines::NGramModel (the independent SML baseline).
//! Changing depth changes model semantics; no ElasticXxx actuation authority
//! or exact residency transition is implied by this module.

use std::collections::{btree_map::Entry, BTreeMap};

use crate::anf_char::{ChunkLayout, LANES};
use crate::char_baselines::ByteScore;

pub const MAX_ORDER: usize = 7;
pub const MAX_EVENTS: usize = 500_000;
pub const MAX_CONTEXTS: usize = 250_000;
pub const MAX_FOLLOWER_PAIRS: usize = 500_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ElasticNGramLimits {
    pub max_order: usize,
    pub max_events: usize,
    pub max_contexts: usize,
    pub max_follower_pairs: usize,
}

impl Default for ElasticNGramLimits {
    fn default() -> Self {
        Self {
            max_order: 4,
            max_events: MAX_EVENTS,
            max_contexts: MAX_CONTEXTS,
            max_follower_pairs: MAX_FOLLOWER_PAIRS,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElasticNGramError {
    InvalidLimits,
    InvalidCorpus,
    TooManyEvents,
    TooManyContexts,
    TooManyFollowerPairs,
    CounterOverflow,
    InvalidDepth,
}

impl std::fmt::Display for ElasticNGramError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ElasticNGramError {}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Histogram {
    total: u32,
    followers: BTreeMap<u8, u32>,
}

impl Histogram {
    fn record(
        &mut self,
        byte: u8,
        total_pairs: &mut usize,
        max_pairs: usize,
    ) -> Result<(), ElasticNGramError> {
        let next_total = self
            .total
            .checked_add(1)
            .ok_or(ElasticNGramError::CounterOverflow)?;
        match self.followers.entry(byte) {
            Entry::Occupied(mut entry) => {
                let incremented = entry
                    .get()
                    .checked_add(1)
                    .ok_or(ElasticNGramError::CounterOverflow)?;
                *entry.get_mut() = incremented;
            }
            Entry::Vacant(entry) => {
                if *total_pairs >= max_pairs {
                    return Err(ElasticNGramError::TooManyFollowerPairs);
                }
                *total_pairs += 1;
                entry.insert(1);
            }
        }
        self.total = next_total;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ElasticNGramAccounting {
    pub training_events: usize,
    pub stored_contexts: usize,
    pub stored_follower_pairs: usize,
    /// Logical u32 counter-value payload only. Excludes BTreeMap/RSS.
    pub counter_value_bytes: usize,
    /// Logical suffix u64 and next-byte u8 key payload only.
    pub logical_key_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DepthTransition {
    Unchanged,
    /// Changes declared model profile, not a semantically exact resource move.
    ModelSemanticsChanged,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElasticNGram {
    max_order: usize,
    levels: Vec<BTreeMap<u64, Histogram>>,
    accounting: ElasticNGramAccounting,
}

fn suffix_key(history: &[u8], depth: usize) -> u64 {
    history[history.len() - depth..]
        .iter()
        .fold(0u64, |key, &byte| (key << 8) | u64::from(byte))
}

impl ElasticNGram {
    /// Build a private snapshot using the same 64-lane context-reset layout
    /// as the existing Witten-Bell baseline. Any failure returns NO model.
    pub fn train(bytes: &[u8], limits: ElasticNGramLimits) -> Result<Self, ElasticNGramError> {
        if limits.max_order > MAX_ORDER
            || limits.max_events == 0
            || limits.max_events > MAX_EVENTS
            || limits.max_contexts == 0
            || limits.max_contexts > MAX_CONTEXTS
            || limits.max_follower_pairs == 0
            || limits.max_follower_pairs > MAX_FOLLOWER_PAIRS
        {
            return Err(ElasticNGramError::InvalidLimits);
        }
        let layout = ChunkLayout::new(bytes.len()).map_err(|_| ElasticNGramError::InvalidCorpus)?;
        let total_events = LANES
            .checked_mul(layout.chunk - 1)
            .ok_or(ElasticNGramError::TooManyEvents)?;
        if total_events > limits.max_events {
            return Err(ElasticNGramError::TooManyEvents);
        }

        let mut levels = vec![BTreeMap::<u64, Histogram>::new(); limits.max_order + 1];
        let mut stored_contexts = 0usize;
        let mut stored_pairs = 0usize;
        for lane in 0..LANES {
            let chunk = &bytes[lane * layout.chunk..(lane + 1) * layout.chunk];
            for t in 1..chunk.len() {
                let history = &chunk[..t];
                for (depth, level) in levels
                    .iter_mut()
                    .enumerate()
                    .take((t + 1).min(limits.max_order + 1))
                {
                    let key = suffix_key(history, depth);
                    let histogram = match level.entry(key) {
                        Entry::Occupied(entry) => entry.into_mut(),
                        Entry::Vacant(entry) => {
                            if stored_contexts >= limits.max_contexts {
                                return Err(ElasticNGramError::TooManyContexts);
                            }
                            stored_contexts += 1;
                            entry.insert(Histogram::default())
                        }
                    };
                    histogram.record(chunk[t], &mut stored_pairs, limits.max_follower_pairs)?;
                }
            }
        }

        Ok(Self {
            max_order: limits.max_order,
            levels,
            accounting: ElasticNGramAccounting {
                training_events: total_events,
                stored_contexts,
                stored_follower_pairs: stored_pairs,
                counter_value_bytes: (stored_contexts + stored_pairs) * std::mem::size_of::<u32>(),
                logical_key_bytes: stored_contexts * std::mem::size_of::<u64>()
                    + stored_pairs * std::mem::size_of::<u8>(),
            },
        })
    }

    #[must_use]
    pub fn trained_max_order(&self) -> usize {
        self.max_order
    }

    #[must_use]
    pub fn accounting(&self) -> ElasticNGramAccounting {
        self.accounting
    }

    #[must_use]
    pub fn contexts_per_order(&self) -> Vec<usize> {
        self.levels.iter().map(BTreeMap::len).collect()
    }

    /// Pure classification: a depth change requires model-quality authority.
    pub fn classify_depth_transition(
        &self,
        current: usize,
        target: usize,
    ) -> Result<DepthTransition, ElasticNGramError> {
        if current > self.max_order || target > self.max_order {
            return Err(ElasticNGramError::InvalidDepth);
        }
        Ok(if current == target {
            DepthTransition::Unchanged
        } else {
            DepthTransition::ModelSemanticsChanged
        })
    }

    /// Explicit caller-selected query depth; never chosen by RAM pressure.
    pub fn distribution(
        &self,
        history: &[u8],
        query_order: usize,
    ) -> Result<[f64; 256], ElasticNGramError> {
        if query_order > self.max_order {
            return Err(ElasticNGramError::InvalidDepth);
        }
        let mut probs = [1.0 / 256.0; 256];
        for (depth, level) in self
            .levels
            .iter()
            .enumerate()
            .take(query_order.min(history.len()) + 1)
        {
            if let Some(hist) = level.get(&suffix_key(history, depth)) {
                let n = f64::from(hist.total);
                let t = hist.followers.len() as f64;
                let scale = t / (n + t);
                for probability in &mut probs {
                    *probability *= scale;
                }
                for (&byte, &count) in &hist.followers {
                    probs[usize::from(byte)] += f64::from(count) / (n + t);
                }
            }
        }
        Ok(probs)
    }

    /// Frozen scoring on independent test data. No labels enter the model.
    pub fn score(&self, bytes: &[u8], query_order: usize) -> Result<ByteScore, ElasticNGramError> {
        if query_order > self.max_order {
            return Err(ElasticNGramError::InvalidDepth);
        }
        let layout = ChunkLayout::new(bytes.len()).map_err(|_| ElasticNGramError::InvalidCorpus)?;
        let (mut bits, mut correct, mut tokens) = (0.0f64, 0u64, 0u64);
        for lane in 0..LANES {
            let chunk = &bytes[lane * layout.chunk..(lane + 1) * layout.chunk];
            for t in 1..chunk.len() {
                let probs = self.distribution(&chunk[..t], query_order)?;
                let y = usize::from(chunk[t]);
                bits -= probs[y].log2();
                let best = (0..256).fold(0, |b, candidate| {
                    if probs[candidate] > probs[b] {
                        candidate
                    } else {
                        b
                    }
                });
                correct += u64::from(best == y);
                tokens += 1;
            }
        }
        Ok(ByteScore {
            tokens,
            bits_per_byte: bits / tokens as f64,
            accuracy: correct as f64 / tokens as f64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::char_baselines::NGramModel;

    fn fixture() -> Vec<u8> {
        (0..LANES)
            .flat_map(|lane| {
                (0..40usize).map(move |t| ((lane * 17 + t * 7 + (t % 3) * 11) % 23) as u8)
            })
            .collect()
    }

    #[test]
    fn each_depth_matches_the_existing_256_way_oracle_bit_exactly() {
        let bytes = fixture();
        let model = ElasticNGram::train(
            &bytes,
            ElasticNGramLimits {
                max_order: 7,
                ..ElasticNGramLimits::default()
            },
        )
        .unwrap();
        assert_eq!(model.accounting().training_events, LANES * 39);
        for depth in 0..=7 {
            let reference = NGramModel::train(&bytes, depth);
            for lane in [0usize, 7, 21, 63] {
                let chunk = &bytes[lane * 40..(lane + 1) * 40];
                for t in [1usize, 2, 4, 8, 17, 39] {
                    let actual = model.distribution(&chunk[..t], depth).unwrap();
                    let mut expected = [0.0; 256];
                    reference.distribution(&chunk[..t], &mut expected);
                    for y in 0..256 {
                        assert_eq!(
                            actual[y].to_bits(),
                            expected[y].to_bits(),
                            "depth={depth}, lane={lane}, t={t}, byte={y}"
                        );
                    }
                }
            }
            let got = model.score(&bytes, depth).unwrap();
            let baseline = reference.score(&bytes);
            assert_eq!(got.tokens, baseline.tokens);
            assert!((got.bits_per_byte - baseline.bits_per_byte).abs() < 1.0e-12);
            assert!((got.accuracy - baseline.accuracy).abs() < 1.0e-12);
        }
    }

    #[test]
    fn depth_change_is_not_claimed_semantics_preserving() {
        let bytes = (0..LANES)
            .flat_map(|_| (0..40usize).map(|t| (t % 2) as u8))
            .collect::<Vec<_>>();
        let model = ElasticNGram::train(&bytes, ElasticNGramLimits::default()).unwrap();
        assert_eq!(
            model.classify_depth_transition(4, 4),
            Ok(DepthTransition::Unchanged)
        );
        assert_eq!(
            model.classify_depth_transition(4, 0),
            Ok(DepthTransition::ModelSemanticsChanged)
        );
        assert_eq!(
            model.classify_depth_transition(4, 5),
            Err(ElasticNGramError::InvalidDepth)
        );
        let short = model.distribution(&[0], 0).unwrap();
        let long = model.distribution(&[0], 1).unwrap();
        assert!(long[1] > short[1]);
        for probs in [short, long] {
            assert!(probs.iter().all(|p| *p > 0.0 && p.is_finite()));
            assert!((probs.iter().sum::<f64>() - 1.0).abs() < 1.0e-10);
        }
    }

    #[test]
    fn immutable_reproducible_training_and_accounting() {
        let bytes = fixture();
        let model = ElasticNGram::train(&bytes, ElasticNGramLimits::default()).unwrap();
        assert_eq!(
            model,
            ElasticNGram::train(&bytes, ElasticNGramLimits::default()).unwrap()
        );
        let saved = model.clone();
        assert!(model.score(&bytes, 2).unwrap().bits_per_byte.is_finite());
        assert_eq!(model, saved);
        let a = model.accounting();
        assert_eq!(model.contexts_per_order().len(), 5);
        assert!(a.stored_follower_pairs >= a.stored_contexts);
        assert_eq!(
            a.counter_value_bytes,
            4 * (a.stored_contexts + a.stored_follower_pairs)
        );
        assert_eq!(
            a.logical_key_bytes,
            8 * a.stored_contexts + a.stored_follower_pairs
        );
    }

    #[test]
    fn fail_closed_resource_limits() {
        let bytes = fixture();
        let mut limits = ElasticNGramLimits::default();
        limits.max_order = 8;
        assert_eq!(
            ElasticNGram::train(&bytes, limits),
            Err(ElasticNGramError::InvalidLimits)
        );
        limits = ElasticNGramLimits::default();
        limits.max_events = 1;
        assert_eq!(
            ElasticNGram::train(&bytes, limits),
            Err(ElasticNGramError::TooManyEvents)
        );
        limits = ElasticNGramLimits::default();
        limits.max_contexts = 1;
        assert_eq!(
            ElasticNGram::train(&bytes, limits),
            Err(ElasticNGramError::TooManyContexts)
        );
        limits = ElasticNGramLimits::default();
        limits.max_follower_pairs = 1;
        assert_eq!(
            ElasticNGram::train(&bytes, limits),
            Err(ElasticNGramError::TooManyFollowerPairs)
        );
        assert_eq!(
            ElasticNGram::train(&[42; 64], ElasticNGramLimits::default()),
            Err(ElasticNGramError::InvalidCorpus)
        );
    }
}
