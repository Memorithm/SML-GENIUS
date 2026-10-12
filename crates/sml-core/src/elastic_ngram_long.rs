//! ENG-1a: exact 0..32-byte Witten-Bell context reference.
//!
//! Long suffixes are held as complete ordered byte vectors, never a lossy
//! hash or truncated machine word. ENG-0 remains intact as an independent
//! bit-exact reference at depths 0..7.
//!
//! This is a model-owned, opt-in reference implementation. Query depth
//! changes MODEL semantics: this module does not actuate ElasticXxx resources
//! or assert any improvement in model quality or physical memory.

use std::collections::{btree_map::Entry, BTreeMap};

use crate::{
    anf_char::{ChunkLayout, LANES},
    char_baselines::ByteScore,
    elastic_ngram::DepthTransition,
};

pub const MAX_LONG_CONTEXT_DEPTH: usize = 32;
pub const MAX_LONG_CONTEXT_EVENTS: usize = 500_000;
pub const MAX_LONG_CONTEXT_KEYS: usize = 250_000;
pub const MAX_LONG_FOLLOWER_PAIRS: usize = 500_000;
pub const MAX_LONG_CONTEXT_KEY_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LongNGramLimits {
    pub max_depth: usize,
    pub max_events: usize,
    pub max_contexts: usize,
    pub max_follower_pairs: usize,
    /// Only the sum of stored logical suffix lengths, not allocated RSS.
    pub max_context_key_bytes: usize,
}

impl Default for LongNGramLimits {
    fn default() -> Self {
        Self {
            max_depth: 12,
            max_events: MAX_LONG_CONTEXT_EVENTS,
            max_contexts: MAX_LONG_CONTEXT_KEYS,
            max_follower_pairs: MAX_LONG_FOLLOWER_PAIRS,
            max_context_key_bytes: MAX_LONG_CONTEXT_KEY_BYTES,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LongNGramError {
    InvalidLimits,
    InvalidCorpus,
    TooManyEvents,
    TooManyContexts,
    TooManyFollowerPairs,
    TooManyKeyBytes,
    CounterOverflow,
    InvalidDepth,
}

impl std::fmt::Display for LongNGramError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for LongNGramError {}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct LongHistogram {
    total: u32,
    followers: BTreeMap<u8, u32>,
}

impl LongHistogram {
    fn record(
        &mut self,
        target: u8,
        observed_follower_pairs: &mut usize,
        max_pairs: usize,
    ) -> Result<(), LongNGramError> {
        let new_total = self
            .total
            .checked_add(1)
            .ok_or(LongNGramError::CounterOverflow)?;
        match self.followers.entry(target) {
            Entry::Occupied(mut entry) => {
                let new_count = entry
                    .get()
                    .checked_add(1)
                    .ok_or(LongNGramError::CounterOverflow)?;
                *entry.get_mut() = new_count;
            }
            Entry::Vacant(entry) => {
                if *observed_follower_pairs >= max_pairs {
                    return Err(LongNGramError::TooManyFollowerPairs);
                }
                *observed_follower_pairs += 1;
                entry.insert(1);
            }
        }
        self.total = new_total;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LongNGramAccounting {
    pub training_events: usize,
    pub contexts: usize,
    pub follower_pairs: usize,
    /// Sum of suffix lengths of every stored context key.
    pub context_key_bytes: usize,
    /// One logical byte per stored distinct follower key.
    pub follower_key_bytes: usize,
    /// Logical u32 context totals plus logical u32 follower counters.
    pub counter_value_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElasticNGramLong {
    max_depth: usize,
    levels: Vec<BTreeMap<Vec<u8>, LongHistogram>>,
    accounting: LongNGramAccounting,
}

impl ElasticNGramLong {
    /// Build one immutable TRAIN snapshot. All count/resource failures return
    /// an error; no partially built model is published.
    pub fn train(bytes: &[u8], limits: LongNGramLimits) -> Result<Self, LongNGramError> {
        if limits.max_depth > MAX_LONG_CONTEXT_DEPTH
            || limits.max_events == 0
            || limits.max_events > MAX_LONG_CONTEXT_EVENTS
            || limits.max_contexts == 0
            || limits.max_contexts > MAX_LONG_CONTEXT_KEYS
            || limits.max_follower_pairs == 0
            || limits.max_follower_pairs > MAX_LONG_FOLLOWER_PAIRS
            || limits.max_context_key_bytes == 0
            || limits.max_context_key_bytes > MAX_LONG_CONTEXT_KEY_BYTES
        {
            return Err(LongNGramError::InvalidLimits);
        }
        let layout = ChunkLayout::new(bytes.len()).map_err(|_| LongNGramError::InvalidCorpus)?;
        let events = LANES
            .checked_mul(layout.chunk - 1)
            .ok_or(LongNGramError::TooManyEvents)?;
        if events > limits.max_events {
            return Err(LongNGramError::TooManyEvents);
        }

        let mut levels = vec![BTreeMap::<Vec<u8>, LongHistogram>::new(); limits.max_depth + 1];
        let mut contexts = 0usize;
        let mut follower_pairs = 0usize;
        let mut key_bytes = 0usize;

        for lane in 0..LANES {
            let chunk = &bytes[lane * layout.chunk..(lane + 1) * layout.chunk];
            for t in 1..chunk.len() {
                let history = &chunk[..t];
                for (depth, level) in levels
                    .iter_mut()
                    .enumerate()
                    .take((t + 1).min(limits.max_depth + 1))
                {
                    let suffix = &history[history.len() - depth..];
                    let hist = match level.entry(suffix.to_vec()) {
                        Entry::Occupied(entry) => entry.into_mut(),
                        Entry::Vacant(entry) => {
                            if contexts >= limits.max_contexts {
                                return Err(LongNGramError::TooManyContexts);
                            }
                            let new_key_bytes = key_bytes
                                .checked_add(depth)
                                .ok_or(LongNGramError::TooManyKeyBytes)?;
                            if new_key_bytes > limits.max_context_key_bytes {
                                return Err(LongNGramError::TooManyKeyBytes);
                            }
                            key_bytes = new_key_bytes;
                            contexts += 1;
                            entry.insert(LongHistogram::default())
                        }
                    };
                    hist.record(chunk[t], &mut follower_pairs, limits.max_follower_pairs)?;
                }
            }
        }
        let counter_value_bytes = contexts
            .checked_add(follower_pairs)
            .and_then(|count| count.checked_mul(std::mem::size_of::<u32>()))
            .ok_or(LongNGramError::CounterOverflow)?;
        Ok(Self {
            max_depth: limits.max_depth,
            levels,
            accounting: LongNGramAccounting {
                training_events: events,
                contexts,
                follower_pairs,
                context_key_bytes: key_bytes,
                follower_key_bytes: follower_pairs,
                counter_value_bytes,
            },
        })
    }

    #[must_use]
    pub fn max_depth(&self) -> usize {
        self.max_depth
    }

    #[must_use]
    pub fn accounting(&self) -> LongNGramAccounting {
        self.accounting
    }

    #[must_use]
    pub fn contexts_by_depth(&self) -> Vec<usize> {
        self.levels.iter().map(BTreeMap::len).collect()
    }

    /// Inform the model owner that changing context depth can change
    /// probabilities. This does not itself authorize the change.
    pub fn classify_depth_transition(
        &self,
        current: usize,
        target: usize,
    ) -> Result<DepthTransition, LongNGramError> {
        if current > self.max_depth || target > self.max_depth {
            return Err(LongNGramError::InvalidDepth);
        }
        Ok(if current == target {
            DepthTransition::Unchanged
        } else {
            DepthTransition::ModelSemanticsChanged
        })
    }

    /// Exact Witten-Bell next-byte distribution at an EXPLICIT depth.
    /// Missing complete suffixes back off to observed shorter suffixes.
    pub fn distribution(
        &self,
        history: &[u8],
        query_depth: usize,
    ) -> Result<[f64; 256], LongNGramError> {
        if query_depth > self.max_depth {
            return Err(LongNGramError::InvalidDepth);
        }
        let mut probs = [1.0 / 256.0; 256];
        for (depth, level) in self
            .levels
            .iter()
            .enumerate()
            .take(query_depth.min(history.len()) + 1)
        {
            let suffix = &history[history.len() - depth..];
            if let Some(hist) = level.get(suffix) {
                let n = f64::from(hist.total);
                let distinct = hist.followers.len() as f64;
                let scale = distinct / (n + distinct);
                for probability in &mut probs {
                    *probability *= scale;
                }
                for (&byte, &count) in &hist.followers {
                    probs[usize::from(byte)] += f64::from(count) / (n + distinct);
                }
            }
        }
        Ok(probs)
    }

    /// Heldout 64-lane scoring. Does not train or modify the count tables.
    pub fn score(&self, bytes: &[u8], query_depth: usize) -> Result<ByteScore, LongNGramError> {
        if query_depth > self.max_depth {
            return Err(LongNGramError::InvalidDepth);
        }
        let layout = ChunkLayout::new(bytes.len()).map_err(|_| LongNGramError::InvalidCorpus)?;
        let (mut bits, mut correct, mut tokens) = (0.0f64, 0u64, 0u64);
        for lane in 0..LANES {
            let chunk = &bytes[lane * layout.chunk..(lane + 1) * layout.chunk];
            for t in 1..chunk.len() {
                let probs = self.distribution(&chunk[..t], query_depth)?;
                let target = usize::from(chunk[t]);
                bits -= probs[target].log2();
                let best = (0..256).fold(0, |prev, candidate| {
                    if probs[candidate] > probs[prev] {
                        candidate
                    } else {
                        prev
                    }
                });
                correct += u64::from(best == target);
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
    use crate::{
        char_baselines::NGramModel,
        elastic_ngram::{ElasticNGram, ElasticNGramLimits},
    };

    fn patterned() -> Vec<u8> {
        (0..LANES)
            .flat_map(|lane| {
                (0..40usize).map(move |t| ((lane * 17 + t * 7 + (t % 3) * 11) % 23) as u8)
            })
            .collect()
    }

    fn different_probe() -> Vec<u8> {
        (0..LANES)
            .flat_map(|lane| {
                (0..40usize).map(move |t| ((lane * 19 + t * 3 + (t % 5) * 23 + 1) % 29) as u8)
            })
            .collect()
    }

    fn depth_limits(depth: usize) -> LongNGramLimits {
        LongNGramLimits {
            max_depth: depth,
            ..LongNGramLimits::default()
        }
    }

    #[test]
    fn all_legacy_depths_match_two_independent_fixed_depth_oracles() {
        let train = patterned();
        let heldout = different_probe();
        let long = ElasticNGramLong::train(&train, depth_limits(32)).unwrap();
        let old = ElasticNGram::train(
            &train,
            ElasticNGramLimits {
                max_order: 7,
                ..ElasticNGramLimits::default()
            },
        )
        .unwrap();
        for depth in 0..=7 {
            let ngram = NGramModel::train(&train, depth);
            for history in [
                &train[0..24],
                &train[40..60],
                &heldout[0..32],
                &heldout[72..96],
                &[255u8; 32][..],
            ] {
                let actual = long.distribution(history, depth).unwrap();
                let previous = old.distribution(history, depth).unwrap();
                let mut reference = [0.0; 256];
                ngram.distribution(history, &mut reference);
                for y in 0..256 {
                    assert_eq!(actual[y].to_bits(), previous[y].to_bits());
                    assert_eq!(actual[y].to_bits(), reference[y].to_bits());
                }
            }
            let left = long.score(&heldout, depth).unwrap();
            let right = ngram.score(&heldout);
            assert_eq!(left.tokens, right.tokens);
            assert_eq!(left.bits_per_byte.to_bits(), right.bits_per_byte.to_bits());
            assert_eq!(left.accuracy.to_bits(), right.accuracy.to_bits());
        }
    }

    #[test]
    fn different_12_byte_histories_with_same_last_seven_never_collide() {
        let train = (0..LANES)
            .flat_map(|lane| {
                let tag = (lane % 2) as u8;
                std::iter::once(tag)
                    .chain(std::iter::repeat_n(0, 11))
                    .chain(std::iter::once(200 + tag))
                    .chain(std::iter::repeat_n(77, 7))
            })
            .collect::<Vec<_>>();
        let long = ElasticNGramLong::train(&train, depth_limits(12)).unwrap();
        let a = [
            std::iter::once(0u8)
                .chain(std::iter::repeat_n(0, 11))
                .collect::<Vec<_>>(),
            std::iter::once(1u8)
                .chain(std::iter::repeat_n(0, 11))
                .collect::<Vec<_>>(),
        ];
        let short_a = long.distribution(&a[0], 7).unwrap();
        let short_b = long.distribution(&a[1], 7).unwrap();
        for y in 0..256 {
            assert_eq!(short_a[y].to_bits(), short_b[y].to_bits());
        }
        let detailed_a = long.distribution(&a[0], 12).unwrap();
        let detailed_b = long.distribution(&a[1], 12).unwrap();
        assert!(detailed_a[200] > 0.9);
        assert!(detailed_b[201] > 0.9);
        assert_ne!(detailed_a[200].to_bits(), detailed_b[200].to_bits());
        assert_eq!(
            long.classify_depth_transition(7, 12),
            Ok(DepthTransition::ModelSemanticsChanged)
        );
        assert_eq!(
            long.classify_depth_transition(12, 12),
            Ok(DepthTransition::Unchanged)
        );
    }

    #[test]
    fn all_depths_normalize_and_fitted_state_never_mutates() {
        let train = patterned();
        let long = ElasticNGramLong::train(&train, depth_limits(32)).unwrap();
        let snapshot = long.clone();
        for history in [&train[0..40], &different_probe()[0..40], &[255u8; 40][..]] {
            for depth in 0..=32 {
                let dist = long.distribution(history, depth).unwrap();
                assert!(dist.iter().all(|x| x.is_finite() && *x > 0.0));
                assert!((dist.iter().sum::<f64>() - 1.0).abs() < 1.0e-10);
            }
        }
        assert!(long
            .score(&different_probe(), 12)
            .unwrap()
            .bits_per_byte
            .is_finite());
        assert_eq!(long, snapshot);
        assert_eq!(
            long,
            ElasticNGramLong::train(&train, depth_limits(32)).unwrap()
        );
        let accounting = long.accounting();
        assert_eq!(accounting.training_events, LANES * 39);
        assert_eq!(long.contexts_by_depth().len(), 33);
        assert_eq!(accounting.follower_key_bytes, accounting.follower_pairs);
        assert_eq!(
            accounting.counter_value_bytes,
            4 * (accounting.contexts + accounting.follower_pairs)
        );
        assert_eq!(
            accounting.context_key_bytes,
            long.levels
                .iter()
                .flat_map(|map| map.keys())
                .map(Vec::len)
                .sum::<usize>()
        );
    }

    #[test]
    fn limits_and_invalid_query_fail_closed_without_partial_model() {
        let train = patterned();
        let invalid = [
            (
                LongNGramLimits {
                    max_depth: 33,
                    ..LongNGramLimits::default()
                },
                LongNGramError::InvalidLimits,
            ),
            (
                LongNGramLimits {
                    max_events: 1,
                    ..LongNGramLimits::default()
                },
                LongNGramError::TooManyEvents,
            ),
            (
                LongNGramLimits {
                    max_contexts: 1,
                    ..LongNGramLimits::default()
                },
                LongNGramError::TooManyContexts,
            ),
            (
                LongNGramLimits {
                    max_follower_pairs: 1,
                    ..LongNGramLimits::default()
                },
                LongNGramError::TooManyFollowerPairs,
            ),
            (
                LongNGramLimits {
                    max_context_key_bytes: 1,
                    ..LongNGramLimits::default()
                },
                LongNGramError::TooManyKeyBytes,
            ),
            (
                LongNGramLimits {
                    max_context_key_bytes: 0,
                    ..LongNGramLimits::default()
                },
                LongNGramError::InvalidLimits,
            ),
        ];
        for (limits, error) in invalid {
            assert_eq!(ElasticNGramLong::train(&train, limits), Err(error));
        }
        assert_eq!(
            ElasticNGramLong::train(&[0u8; 64], LongNGramLimits::default()),
            Err(LongNGramError::InvalidCorpus)
        );
        let fitted = ElasticNGramLong::train(&train, depth_limits(16)).unwrap();
        assert_eq!(
            fitted.distribution(&[0; 32], 17),
            Err(LongNGramError::InvalidDepth)
        );
        assert_eq!(
            fitted.classify_depth_transition(16, 17),
            Err(LongNGramError::InvalidDepth)
        );
    }
}
