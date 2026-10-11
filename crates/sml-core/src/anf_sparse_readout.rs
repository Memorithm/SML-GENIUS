//! Sparse, train-only binary conditional readout with recursive context backoff.
//!
//! Inputs are ordered most-general to most-specific. Each prefix stores only
//! observed addresses. The posterior interpolates the current KT estimate with
//! the shorter prefix using a train-only count-dependent weight.
//! This is a probabilistic readout candidate, not a trained recurrent feature.
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SparseReadoutError {
    Width,
    Shape,
    Empty,
    Overflow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SparseBinaryReadout {
    width: usize,
    /// Prefix-depth -> address -> [zeros, ones].
    counts: Vec<BTreeMap<u64, [u64; 2]>>,
    samples: u64,
}

impl SparseBinaryReadout {
    pub fn fit(rows: &[u64], targets: &[bool], width: usize) -> Result<Self, SparseReadoutError> {
        if width > 63 {
            return Err(SparseReadoutError::Width);
        }
        if rows.is_empty() {
            return Err(SparseReadoutError::Empty);
        }
        if rows.len() != targets.len() {
            return Err(SparseReadoutError::Shape);
        }
        let mut counts = vec![BTreeMap::<u64, [u64; 2]>::new(); width + 1];
        let limit = if width == 0 { 0 } else { (1u64 << width) - 1 };
        for (&row, &target) in rows.iter().zip(targets) {
            if row & !limit != 0 {
                return Err(SparseReadoutError::Width);
            }
            for (depth, level) in counts.iter_mut().enumerate() {
                let mask = if depth == 0 { 0 } else { (1u64 << depth) - 1 };
                let entry = level.entry(row & mask).or_default();
                entry[usize::from(target)] = entry[usize::from(target)]
                    .checked_add(1)
                    .ok_or(SparseReadoutError::Overflow)?;
            }
        }
        Ok(Self {
            width,
            counts,
            samples: rows.len() as u64,
        })
    }

    /// Original A5-4 reference at alpha=4.0; unchanged for existing callers.
    pub fn predict_one(&self, row: u64) -> Result<f64, SparseReadoutError> {
        self.predict_with_alpha(row, 4.0)
    }

    /// Train-frozen interpolation: only alpha changes between predictions.
    pub fn predict_with_alpha(&self, row: u64, alpha: f64) -> Result<f64, SparseReadoutError> {
        if !alpha.is_finite() || alpha <= 0.0 {
            return Err(SparseReadoutError::Shape);
        }
        let limit = if self.width == 0 {
            0
        } else {
            (1u64 << self.width) - 1
        };
        if row & !limit != 0 {
            return Err(SparseReadoutError::Width);
        }
        let base = self.counts[0].get(&0).ok_or(SparseReadoutError::Empty)?;
        let mut probability = (base[1] as f64 + 0.5) / (self.samples as f64 + 1.0);
        for depth in 1..=self.width {
            let mask = (1u64 << depth) - 1;
            if let Some(counts) = self.counts[depth].get(&(row & mask)) {
                let n = (counts[0] + counts[1]) as f64;
                let kt = (counts[1] as f64 + 0.5) / (n + 1.0);
                // Rare contexts back off; frequent contexts dominate.
                let weight = n / (n + alpha);
                probability = weight * kt + (1.0 - weight) * probability;
            }
        }
        Ok(probability)
    }

    pub fn bits_per_target(
        &self,
        rows: &[u64],
        targets: &[bool],
    ) -> Result<f64, SparseReadoutError> {
        if rows.is_empty() {
            return Err(SparseReadoutError::Empty);
        }
        if rows.len() != targets.len() {
            return Err(SparseReadoutError::Shape);
        }
        let mut total = 0.0;
        for (&row, &target) in rows.iter().zip(targets) {
            let p = self.predict_one(row)?;
            total -= if target { p } else { 1.0 - p }.log2();
        }
        Ok(total / rows.len() as f64)
    }

    pub fn stored_contexts(&self) -> usize {
        self.counts.iter().map(BTreeMap::len).sum()
    }

    pub fn training_samples(&self) -> u64 {
        self.samples
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sparse_backoff_is_normalized_and_finite_for_unseen_contexts() {
        let model = SparseBinaryReadout::fit(
            &[0, 0, 1, 1, 1, 2],
            &[false, false, true, true, true, false],
            12,
        )
        .unwrap();
        for row in [0, 1, 2, 3, 1023, 4095] {
            let p = model.predict_one(row).unwrap();
            assert!(p > 0.0 && p < 1.0);
            assert!((p + (1.0 - p) - 1.0).abs() < 1e-14);
        }
        assert!(model.stored_contexts() < 100);
        assert_eq!(model.training_samples(), 6);
    }
    #[test]
    fn train_only_fit_is_deterministic_and_validation_never_mutates_counts() {
        let rows = [0, 1, 3, 7, 7, 15, 15, 15];
        let labels = [false, true, false, true, true, true, false, true];
        let model = SparseBinaryReadout::fit(&rows, &labels, 4).unwrap();
        let before = model.clone();
        let loss = model
            .bits_per_target(&[15, 2, 9], &[true, false, true])
            .unwrap();
        assert!(loss.is_finite());
        assert_eq!(model, before);
        assert_eq!(model, SparseBinaryReadout::fit(&rows, &labels, 4).unwrap());
    }
    #[test]
    fn rejects_bad_shapes_and_out_of_range_addresses() {
        assert_eq!(
            SparseBinaryReadout::fit(&[], &[], 1),
            Err(SparseReadoutError::Empty)
        );
        assert_eq!(
            SparseBinaryReadout::fit(&[0], &[], 1),
            Err(SparseReadoutError::Shape)
        );
        assert_eq!(
            SparseBinaryReadout::fit(&[0], &[true], 64),
            Err(SparseReadoutError::Width)
        );
        assert_eq!(
            SparseBinaryReadout::fit(&[2], &[true], 1),
            Err(SparseReadoutError::Width)
        );
        let model = SparseBinaryReadout::fit(&[0], &[true], 1).unwrap();
        assert_eq!(model.predict_one(2), Err(SparseReadoutError::Width));
    }
    #[test]
    fn learns_simple_conditional_dependency_better_than_global_prior() {
        let mut rows = Vec::new();
        let mut labels = Vec::new();
        for _ in 0..100 {
            rows.extend([0, 1]);
            labels.extend([false, true]);
        }
        let model = SparseBinaryReadout::fit(&rows, &labels, 1).unwrap();
        assert!(model.predict_one(0).unwrap() < 0.1);
        assert!(model.predict_one(1).unwrap() > 0.9);
        assert!(model.bits_per_target(&rows, &labels).unwrap() < 0.2);
    }

    #[test]
    fn alpha_four_is_bit_exact_legacy_oracle_and_extremes_are_valid() {
        let model = SparseBinaryReadout::fit(
            &[0, 1, 1, 2, 2, 2, 3],
            &[false, false, true, false, true, true, true],
            3,
        )
        .unwrap();
        let before = model.clone();
        for row in 0..8u64 {
            let base = model.counts[0].get(&0).unwrap();
            let mut expected = (base[1] as f64 + 0.5) / (model.samples as f64 + 1.0);
            for depth in 1..=model.width {
                let mask = (1u64 << depth) - 1;
                if let Some(counts) = model.counts[depth].get(&(row & mask)) {
                    let n = (counts[0] + counts[1]) as f64;
                    let kt = (counts[1] as f64 + 0.5) / (n + 1.0);
                    let legacy_weight = n / (n + 4.0);
                    expected = legacy_weight * kt + (1.0 - legacy_weight) * expected;
                }
            }
            assert_eq!(model.predict_one(row).unwrap().to_bits(), expected.to_bits());
            assert_eq!(
                model.predict_with_alpha(row, 4.0).unwrap().to_bits(),
                expected.to_bits()
            );
            for alpha in [0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0] {
                let p = model.predict_with_alpha(row, alpha).unwrap();
                assert!(p.is_finite() && p > 0.0 && p < 1.0);
            }
        }
        for alpha in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                model.predict_with_alpha(0, alpha),
                Err(SparseReadoutError::Shape)
            );
        }
        assert_eq!(model, before);
    }
}
