//! Matched ANF byte-level dense-KT versus sparse hierarchical-backoff scoring.
//! Both models consume exactly the same frozen recurrent trajectory and readout
//! wiring; sparse fitting sees training targets only.
use crate::{
    anf_char::{CharAnfModel, CharMetrics, BYTE_BITS},
    anf_local::LocalAnfError,
    anf_local_incremental::{LaneData, ReadoutInput, Trajectory},
    anf_sparse_readout::{SparseBinaryReadout, SparseReadoutError},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SparseByteError {
    Network(LocalAnfError),
    Sparse(SparseReadoutError),
    Shape,
}
impl From<LocalAnfError> for SparseByteError {
    fn from(value: LocalAnfError) -> Self { Self::Network(value) }
}
impl From<SparseReadoutError> for SparseByteError {
    fn from(value: SparseReadoutError) -> Self { Self::Sparse(value) }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SparseByteReadouts {
    pub bits: Vec<SparseBinaryReadout>,
}

fn address(
    spec: &crate::anf_local_incremental::ReadoutSpec,
    sources: &[u64],
    lane: u32,
    prefix: &[usize; BYTE_BITS],
) -> u64 {
    spec.inputs.iter().enumerate().fold(0u64, |a, (j, input)| {
        let value = match *input {
            ReadoutInput::Source(index) => (sources[index] >> lane) & 1,
            ReadoutInput::Target(index) => prefix[index] as u64,
        };
        a | (value << j)
    })
}

impl SparseByteReadouts {
    pub fn fit(model: &CharAnfModel, train: &LaneData) -> Result<Self, SparseByteError> {
        if model.readouts.len() != BYTE_BITS || train.target_bits != BYTE_BITS {
            return Err(SparseByteError::Shape);
        }
        let traj = Trajectory::simulate(&model.network, train)?;
        let mut rows = vec![Vec::new(); BYTE_BITS];
        let mut targets = vec![Vec::new(); BYTE_BITS];
        for (bi, block) in train.blocks.iter().enumerate() {
            for t in 0..block.steps {
                let sources = traj.row(bi, t);
                let target_words = &block.targets[t * BYTE_BITS..(t + 1) * BYTE_BITS];
                let mut lanes = block.valid;
                while lanes != 0 {
                    let lane = lanes.trailing_zeros();
                    lanes &= lanes - 1;
                    let mut prefix = [0usize; BYTE_BITS];
                    for b in 0..BYTE_BITS {
                        prefix[b] = ((target_words[b] >> lane) & 1) as usize;
                    }
                    for b in 0..BYTE_BITS {
                        rows[b].push(address(&model.readouts[b], sources, lane, &prefix));
                        targets[b].push(prefix[b] != 0);
                    }
                }
            }
        }
        let mut bits = Vec::with_capacity(BYTE_BITS);
        for b in 0..BYTE_BITS {
            bits.push(SparseBinaryReadout::fit(
                &rows[b], &targets[b], model.readouts[b].inputs.len(),
            )?);
        }
        Ok(Self { bits })
    }

    pub fn stored_contexts(&self) -> usize {
        self.bits.iter().map(SparseBinaryReadout::stored_contexts).sum()
    }

    pub fn evaluate(
        &self, model: &CharAnfModel, data: &LaneData, argmax: bool,
    ) -> Result<CharMetrics, SparseByteError> {
        if self.bits.len() != BYTE_BITS || model.readouts.len() != BYTE_BITS
            || data.target_bits != BYTE_BITS {
            return Err(SparseByteError::Shape);
        }
        let traj = Trajectory::simulate(&model.network, data)?;
        let mut bits = 0.0f64;
        let mut correct = 0u64;
        let mut tokens = 0u64;
        for (bi, block) in data.blocks.iter().enumerate() {
            for t in 0..block.steps {
                let sources = traj.row(bi, t);
                let target_words = &block.targets[t * BYTE_BITS..(t + 1) * BYTE_BITS];
                let mut lanes = block.valid;
                while lanes != 0 {
                    let lane = lanes.trailing_zeros();
                    lanes &= lanes - 1;
                    let mut actual = [0usize; BYTE_BITS];
                    for b in 0..BYTE_BITS {
                        actual[b] = ((target_words[b] >> lane) & 1) as usize;
                        let a = address(&model.readouts[b], sources, lane, &actual);
                        let p = self.bits[b].predict_one(a)?;
                        bits -= if actual[b] == 1 { p } else { 1.0 - p }.log2();
                    }
                    if argmax {
                        let mut best = (0usize, f64::NEG_INFINITY);
                        for candidate in 0..256usize {
                            let mut prefix = [0usize; BYTE_BITS];
                            let mut lp = 0.0;
                            for b in 0..BYTE_BITS {
                                prefix[b] = (candidate >> (7 - b)) & 1;
                                let a = address(&model.readouts[b], sources, lane, &prefix);
                                let p = self.bits[b].predict_one(a)?;
                                lp += if prefix[b] == 1 { p } else { 1.0 - p }.log2();
                            }
                            if lp > best.1 { best = (candidate, lp); }
                        }
                        let truth = actual.iter().fold(0usize, |a, &b| (a << 1) | b);
                        correct += u64::from(best.0 == truth);
                    }
                    tokens += 1;
                }
            }
        }
        Ok(CharMetrics {
            tokens,
            bits_per_byte: bits / tokens.max(1) as f64,
            accuracy: if argmax { correct as f64 / tokens.max(1) as f64 }
                else { f64::NAN },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anf_char::{initial_network, train, CharAnfConfig};
    use crate::anf_local_incremental::LaneBlock;
    #[test]
    fn sparse_byte_scoring_is_finite_deterministic_and_train_only() {
        let config = CharAnfConfig {
            free_gates: 8, lags: 1, fan_in: 4, context_bits: 4,
            max_readout_fan_in: 12, max_epochs: 0, ..CharAnfConfig::default()
        };
        let make = |shift: usize| {
            let steps = 32;
            let mut inputs = vec![0u64; steps * 8];
            let mut targets = vec![0u64; steps * 8];
            for t in 0..steps {
                for lane in 0..8 {
                    let x = ((t + lane + shift) % 7) as u8;
                    let y = ((t + lane + shift + 1) % 7) as u8;
                    for bit in 0..8 {
                        inputs[t * 8 + bit] |= u64::from((x >> bit) & 1) << lane;
                        targets[t * 8 + bit] |= u64::from((y >> (7 - bit)) & 1) << lane;
                    }
                }
            }
            LaneData {
                input_bits: 8, target_bits: 8,
                blocks: vec![LaneBlock { steps, inputs, targets, valid: 255 }],
            }
        };
        let training = make(0);
        let held_out = make(3);
        let report = train(config, &training, &held_out).unwrap();
        let before = report.model.clone();
        let sparse = SparseByteReadouts::fit(&report.model, &training).unwrap();
        let a = sparse.evaluate(&report.model, &held_out, true).unwrap();
        let b = sparse.evaluate(&report.model, &held_out, true).unwrap();
        assert_eq!(a, b);
        assert!(a.bits_per_byte.is_finite());
        assert!(a.bits_per_byte > 0.0);
        assert!(sparse.stored_contexts() > 0);
        assert_eq!(report.model, before);
        assert_eq!(initial_network(&config).unwrap().state_bits(), 16);
    }
}
