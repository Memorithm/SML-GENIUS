//! Opt-in 256-class extension of the existing binary sparse ANF readout.
//! Witten-Bell interpolation backs off from an unobserved long Boolean
//! signature to observed shorter prefixes. Feature order and all gates are
//! caller-frozen; this module fits TRAIN-only conditional byte counts.
//! It is not a trained ANF gate, recurrent integration, or quality result.

use std::collections::BTreeMap;

pub const MAX_BACKOFF_WIDTH: usize = 16;
pub const MAX_BACKOFF_CONTEXTS: usize = 8192;
pub const MAX_BACKOFF_ROWS: usize = 500_000;
pub const BYTE_CLASSES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ByteBackoffConfig {
    pub width: usize,
    /// Maximum number of distinct (prefix-depth, address) entries in RAM.
    pub max_contexts: usize,
}

impl Default for ByteBackoffConfig {
    fn default() -> Self {
        Self {
            width: 8,
            max_contexts: MAX_BACKOFF_CONTEXTS,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ByteBackoffError {
    InvalidWidth,
    InvalidBudget,
    EmptyTrain,
    TooManyRows,
    SignatureOutsideDomain,
    ContextBudgetExceeded,
    CounterOverflow,
}

impl std::fmt::Display for ByteBackoffError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ByteBackoffError {}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ByteCounts {
    counts: [u32; BYTE_CLASSES],
    total: u32,
    distinct: u16,
}

impl Default for ByteCounts {
    fn default() -> Self {
        Self {
            counts: [0u32; BYTE_CLASSES],
            total: 0,
            distinct: 0,
        }
    }
}

impl ByteCounts {
    fn observe(&mut self, label: u8) -> Result<(), ByteBackoffError> {
        let count = &mut self.counts[usize::from(label)];
        if *count == 0 {
            self.distinct = self
                .distinct
                .checked_add(1)
                .ok_or(ByteBackoffError::CounterOverflow)?;
        }
        *count = count
            .checked_add(1)
            .ok_or(ByteBackoffError::CounterOverflow)?;
        self.total = self
            .total
            .checked_add(1)
            .ok_or(ByteBackoffError::CounterOverflow)?;
        Ok(())
    }
}

fn valid(bits: u64, width: usize) -> bool {
    bits >> width == 0
}

fn prefix(bits: u64, depth: usize) -> u64 {
    if depth == 0 {
        0
    } else {
        bits & ((1u64 << depth) - 1)
    }
}

/// Sparse and immutable after TRAIN. Each observed prefix owns 256 counters.
/// Logical counter payload is not a process-RSS or allocator measurement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SparseByteBackoff {
    width: usize,
    levels: Vec<BTreeMap<u64, ByteCounts>>,
    contexts: usize,
    train_samples: usize,
}

impl SparseByteBackoff {
    pub fn fit(
        train: &[(u64, u8)],
        cfg: ByteBackoffConfig,
    ) -> Result<Self, ByteBackoffError> {
        if cfg.width > MAX_BACKOFF_WIDTH {
            return Err(ByteBackoffError::InvalidWidth);
        }
        if cfg.max_contexts == 0 || cfg.max_contexts > MAX_BACKOFF_CONTEXTS {
            return Err(ByteBackoffError::InvalidBudget);
        }
        if train.is_empty() {
            return Err(ByteBackoffError::EmptyTrain);
        }
        if train.len() > MAX_BACKOFF_ROWS {
            return Err(ByteBackoffError::TooManyRows);
        }

        let mut levels = vec![BTreeMap::<u64, ByteCounts>::new(); cfg.width + 1];
        let mut contexts = 0usize;
        for &(bits, label) in train {
            if !valid(bits, cfg.width) {
                return Err(ByteBackoffError::SignatureOutsideDomain);
            }
            for (depth, level) in levels.iter_mut().enumerate() {
                let key = prefix(bits, depth);
                if !level.contains_key(&key) {
                    if contexts >= cfg.max_contexts {
                        return Err(ByteBackoffError::ContextBudgetExceeded);
                    }
                    level.insert(key, ByteCounts::default());
                    contexts += 1;
                }
                level
                    .get_mut(&key)
                    .expect("just checked/inserted TRAIN address")
                    .observe(label)?;
            }
        }
        Ok(Self {
            width: cfg.width,
            levels,
            contexts,
            train_samples: train.len(),
        })
    }

    /// All 256 next-byte class probabilities, including never-seen signatures.
    /// Start from uniform and apply Witten-Bell at each observed prefix:
    /// P_d(y)=[count_d(y)+T_d*P_(d-1)(y)]/[n_d+T_d].
    pub fn distribution(
        &self,
        signature: u64,
    ) -> Result<[f64; BYTE_CLASSES], ByteBackoffError> {
        if !valid(signature, self.width) {
            return Err(ByteBackoffError::SignatureOutsideDomain);
        }
        let mut probability = [1.0 / BYTE_CLASSES as f64; BYTE_CLASSES];
        for (depth, level) in self.levels.iter().enumerate() {
            let Some(counts) = level.get(&prefix(signature, depth)) else {
                continue;
            };
            let denominator = f64::from(counts.total) + f64::from(counts.distinct);
            let weight = f64::from(counts.distinct) / denominator;
            for (byte, p) in probability.iter_mut().enumerate() {
                *p = f64::from(counts.counts[byte]) / denominator + weight * *p;
            }
        }
        Ok(probability)
    }

    pub fn stored_contexts(&self) -> usize {
        self.contexts
    }

    pub fn trained_rows(&self) -> usize {
        self.train_samples
    }

    /// Includes each prefix's fixed counters and its total/distinct fields,
    /// but excludes BTreeMap nodes/allocations, metadata and overall RSS.
    pub fn logical_context_payload_bytes(&self) -> usize {
        self.contexts * std::mem::size_of::<ByteCounts>()
    }

    pub fn prefix_histogram(&self) -> Vec<usize> {
        self.levels.iter().map(BTreeMap::len).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anf_sparse_readout::SparseBinaryReadout;

    #[test]
    fn unseen_high_order_address_backs_off_to_informative_low_bits() {
        let mut train = Vec::new();
        for _ in 0..128 {
            train.push((0u64, 0u8));
            train.push((1u64, 255u8));
        }
        let model = SparseByteBackoff::fit(
            &train,
            ByteBackoffConfig {
                width: 8,
                max_contexts: 20,
            },
        )
        .unwrap();
        // Bit 7 never occurred in TRAIN; lower prefixes are still observed.
        for (signature, label) in [(128u64, 0usize), (129, 255)] {
            let p = model.distribution(signature).unwrap();
            assert!(p[label] > 0.75);
            assert!(p.iter().all(|v| v.is_finite() && *v > 0.0));
            assert!((p.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        }
        assert_eq!(model.stored_contexts(), 1 + 2 * 8);
        assert_eq!(model.trained_rows(), 256);
        assert!(model.logical_context_payload_bytes() > 0);
    }

    #[test]
    fn fitting_order_does_not_change_counts_or_predictions() {
        let train = [(0, 0), (0, 2), (1, 255), (3, 255), (2, 1)];
        let cfg = ByteBackoffConfig {
            width: 3,
            max_contexts: 64,
        };
        let first = SparseByteBackoff::fit(&train, cfg).unwrap();
        let mut reversed = train;
        reversed.reverse();
        let second = SparseByteBackoff::fit(&reversed, cfg).unwrap();
        assert_eq!(first, second);
        let saved = first.clone();
        for signature in 0..8 {
            let p = first.distribution(signature).unwrap();
            assert!((p.iter().sum::<f64>() - 1.0).abs() < 1e-12);
            assert!(p.iter().all(|v| *v > 0.0 && v.is_finite()));
        }
        assert_eq!(first, saved);
        assert_eq!(first.prefix_histogram().len(), 4);
    }

    #[test]
    fn zero_width_and_legacy_binary_readout_are_unchanged() {
        let train = [(0u64, 1u8), (0, 1), (0, 0)];
        let model = SparseByteBackoff::fit(
            &train,
            ByteBackoffConfig {
                width: 0,
                max_contexts: 1,
            },
        )
        .unwrap();
        assert_eq!(model.prefix_histogram(), [1]);
        assert_eq!(model.stored_contexts(), 1);
        assert!(model.distribution(0).unwrap()[1] > model.distribution(0).unwrap()[0]);
        // Existing binary API remains available and learns its own 2-class task.
        let binary = SparseBinaryReadout::fit(&[0, 1, 1], &[false, true, true], 1).unwrap();
        assert!(binary.predict_one(1).unwrap() > binary.predict_one(0).unwrap());
    }

    #[test]
    fn fails_closed_on_bad_input_and_context_overflow() {
        let cfg = ByteBackoffConfig {
            width: 1,
            max_contexts: 2,
        };
        assert_eq!(
            SparseByteBackoff::fit(&[], cfg),
            Err(ByteBackoffError::EmptyTrain)
        );
        assert_eq!(
            SparseByteBackoff::fit(&[(2, 0)], cfg),
            Err(ByteBackoffError::SignatureOutsideDomain)
        );
        assert_eq!(
            SparseByteBackoff::fit(&[(0, 0), (1, 1)], cfg),
            Err(ByteBackoffError::ContextBudgetExceeded)
        );
        assert_eq!(
            SparseByteBackoff::fit(
                &[(0, 0)],
                ByteBackoffConfig {
                    width: 17,
                    max_contexts: 8
                }
            ),
            Err(ByteBackoffError::InvalidWidth)
        );
        let good = SparseByteBackoff::fit(
            &[(0, 0)],
            ByteBackoffConfig {
                width: 1,
                max_contexts: 8,
            },
        )
        .unwrap();
        assert_eq!(
            good.distribution(2),
            Err(ByteBackoffError::SignatureOutsideDomain)
        );
    }
}
