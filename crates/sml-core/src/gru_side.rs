//! Frozen, single-layer byte GRU CPU reference for BMX experiments.
//!
//! Parameters originate as f32, arithmetic is f64 with fixed four-lane dot
//! reductions. This is NOT bit-exact PyTorch/cuDNN inference or a portable
//! integer codec identity: exp/tanh require same-build replay qualification.
//! No training, future input, external runtime, unsafe code or GPU dependency.

use std::sync::Arc;

use crate::{bit_mixer::SideModel, elastic_ngram_paged_sha::sha256};

#[derive(Debug)]
struct Weights {
    hidden: usize,
    embedding: Vec<f64>,
    input: Vec<f64>,
    recurrent: Vec<f64>,
    input_bias: Vec<f64>,
    recurrent_bias: Vec<f64>,
    output: Vec<f64>,
    output_bias: Vec<f64>,
    identity: String,
}

/// A causal byte predictor; state resets to zero and consumes newline BOS.
#[derive(Clone, Debug)]
pub struct GruSide {
    weights: Arc<Weights>,
    hidden: Vec<f64>,
    cdf: [f64; 257],
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    let mut sum = [0.0; 4];
    let mut chunks = a.chunks_exact(4).zip(b.chunks_exact(4));
    for (x, y) in &mut chunks {
        for k in 0..4 {
            sum[k] += x[k] * y[k];
        }
    }
    let mut value = ((sum[0] + sum[1]) + sum[2]) + sum[3];
    for i in (a.len() / 4 * 4)..a.len() {
        value += a[i] * b[i];
    }
    value
}

fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

impl GruSide {
    /// SMLGRU01 + hidden:u32 + embedding:u32(64) + checkpoint SHA256:32
    /// + seven contiguous row-major f32 tensors, all little endian.
    ///
    /// Bounds, exact lengths, finite and bounded scalars are mandatory.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 48 || &bytes[..8] != b"SMLGRU01" {
            return Err("invalid GRU header".into());
        }
        let h = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        let e = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        if !(1..=512).contains(&h) || e != 64 {
            return Err("unsupported GRU dimensions".into());
        }
        let sizes = [256 * e, 3 * h * e, 3 * h * h, 3 * h, 3 * h, 256 * h, 256];
        if bytes.len() != 48 + sizes.iter().sum::<usize>() * 4 {
            return Err("GRU tensor length mismatch".into());
        }
        let scalars: Vec<f64> = bytes[48..]
            .chunks_exact(4)
            .map(|x| f64::from(f32::from_le_bytes(x.try_into().unwrap())))
            .collect();
        if scalars.iter().any(|x| !x.is_finite() || x.abs() > 64.0) {
            return Err("invalid GRU scalar".into());
        }
        let mut offset = 0;
        let mut take = |n: usize| {
            let out = scalars[offset..offset + n].to_vec();
            offset += n;
            out
        };
        let digest = sha256(bytes);
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        let weights = Weights {
            hidden: h,
            embedding: take(sizes[0]),
            input: take(sizes[1]),
            recurrent: take(sizes[2]),
            input_bias: take(sizes[3]),
            recurrent_bias: take(sizes[4]),
            output: take(sizes[5]),
            output_bias: take(sizes[6]),
            identity: format!("gru-f64-cpu-v1:h={h};e=64;bos=10;weights={hex}"),
        };
        let mut side = Self {
            weights: Arc::new(weights),
            hidden: vec![0.0; h],
            cdf: [0.0; 257],
        };
        side.push_byte(10);
        Ok(side)
    }

    /// Frozen next-byte probabilities, for independent recurrence readouts.
    #[must_use]
    pub fn probabilities(&self) -> [f64; 256] {
        std::array::from_fn(|i| self.cdf[i + 1] - self.cdf[i])
    }
}

impl SideModel for GruSide {
    fn label(&self) -> String {
        self.weights.identity.clone()
    }

    fn push_byte(&mut self, byte: u8) {
        let w = &self.weights;
        let h = w.hidden;
        let x = &w.embedding[usize::from(byte) * 64..(usize::from(byte) + 1) * 64];
        let input: Vec<f64> = w
            .input
            .chunks_exact(64)
            .zip(&w.input_bias)
            .map(|(row, b)| dot(row, x) + b)
            .collect();
        let recurrent: Vec<f64> = w
            .recurrent
            .chunks_exact(h)
            .zip(&w.recurrent_bias)
            .map(|(row, b)| dot(row, &self.hidden) + b)
            .collect();
        for i in 0..h {
            let r = sigmoid(input[i] + recurrent[i]);
            let z = sigmoid(input[h + i] + recurrent[h + i]);
            // PyTorch applies the reset gate AFTER the recurrent affine map.
            let n = (input[2 * h + i] + r * recurrent[2 * h + i]).tanh();
            self.hidden[i] = (1.0 - z) * n + z * self.hidden[i];
        }
        let logits: Vec<f64> = w
            .output
            .chunks_exact(h)
            .zip(&w.output_bias)
            .map(|(row, b)| dot(row, &self.hidden) + b)
            .collect();
        let max = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let mass: Vec<f64> = logits.iter().map(|x| (x - max).exp().max(1e-12)).collect();
        let total: f64 = mass.iter().sum();
        self.cdf[0] = 0.0;
        for (i, p) in mass.iter().enumerate() {
            self.cdf[i + 1] = self.cdf[i] + p / total;
        }
    }

    fn predict(&self, bit_pos: u32, c0: u32) -> (i32, u32) {
        assert!(bit_pos < 8 && (1 << bit_pos..2 << bit_pos).contains(&c0));
        let span = 1usize << (8 - bit_pos);
        let lo = (c0 as usize - (1usize << bit_pos)) * span;
        let mid = lo + span / 2;
        let den = self.cdf[lo + span] - self.cdf[lo];
        let p = (self.cdf[lo + span] - self.cdf[mid]) / den;
        ((p * 4096.0).round().clamp(1.0, 4095.0) as i32, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        bit_mixer::{BitMixer, MixerConfig, SideInputs},
        bool_coder::{decode_with_predictor, encode_with_predictor},
    };

    fn fixture() -> Vec<u8> {
        let mut b = b"SMLGRU01".to_vec();
        b.extend(1u32.to_le_bytes());
        b.extend(64u32.to_le_bytes());
        b.extend([0; 32]);
        b.resize(48 + (256 * 64 + 3 * 64 + 3 + 3 + 3 + 256 + 256) * 4, 0);
        b
    }

    #[test]
    fn zero_model_is_uniform_and_clone_has_independent_history() {
        let s = GruSide::from_bytes(&fixture()).unwrap();
        for p in s.probabilities() {
            assert_eq!(p, 1.0 / 256.0);
        }
        for bit in 0..8 {
            for c0 in (1 << bit)..(2 << bit) {
                assert_eq!(s.predict(bit, c0), (2048, 0));
            }
        }
        let mut clone = s.clone();
        clone.push_byte(71);
        assert_eq!(s.probabilities(), clone.probabilities());
    }

    #[test]
    fn reset_gate_is_applied_after_recurrent_bias_and_replay_is_exact() {
        let mut bytes = fixture();
        let bias = 256 * 64 + 3 * 64 + 3;
        bytes[48 + (bias + 2) * 4..48 + (bias + 3) * 4].copy_from_slice(&0.5f32.to_le_bytes());
        bytes[48 + (bias + 5) * 4..48 + (bias + 6) * 4].copy_from_slice(&1.0f32.to_le_bytes());
        let s = GruSide::from_bytes(&bytes).unwrap();
        assert!((s.hidden[0] - 0.5 * 1.0f64.tanh()).abs() < 1e-15);
        let mut a = s.clone();
        let mut b = s;
        for byte in b"same causal history" {
            a.push_byte(*byte);
            b.push_byte(*byte);
            assert_eq!(a.cdf, b.cdf);
        }
        let config = MixerConfig {
            table_bits: 10,
            match_bits: 10,
            ..MixerConfig::default()
        };
        let build = || {
            BitMixer::try_with_side(
                config,
                config.mask_set.masks(),
                Box::new(a.clone()),
                SideInputs::ProbabilityOnly,
            )
            .unwrap()
        };
        let mut encoder = build();
        let id = encoder.identity();
        let encoded =
            encode_with_predictor(b"frozen gru codec\n", id.as_bytes(), &mut encoder).unwrap();
        assert_eq!(
            decode_with_predictor(&encoded.container, id.as_bytes(), &mut build()).unwrap(),
            b"frozen gru codec\n"
        );
    }

    #[test]
    fn malformed_weights_are_rejected() {
        let good = fixture();
        for n in [0, 47, good.len() - 1] {
            assert!(GruSide::from_bytes(&good[..n]).is_err());
        }
        let mut bad = good.clone();
        bad.push(0);
        assert!(GruSide::from_bytes(&bad).is_err());
        bad = good.clone();
        bad[8..12].copy_from_slice(&513u32.to_le_bytes());
        assert!(GruSide::from_bytes(&bad).is_err());
        bad = good;
        bad[48..52].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(GruSide::from_bytes(&bad).is_err());
    }

    #[test]
    fn nonzero_model_is_causal_and_weights_are_part_of_codec_identity() {
        let mut bytes = fixture();
        // Make observed byte A change candidate state, and output byte B
        // respond to that state. Other embeddings and gates remain zero.
        let set = |bytes: &mut Vec<u8>, index: usize, value: f32| {
            bytes[48 + index * 4..48 + (index + 1) * 4].copy_from_slice(&value.to_le_bytes());
        };
        set(&mut bytes, 65 * 64, 1.0);
        set(&mut bytes, 256 * 64 + 2 * 64, 1.0);
        set(&mut bytes, 256 * 64 + 3 * 64 + 3 + 3 + 3 + 66, 2.0);
        let initial = GruSide::from_bytes(&bytes).unwrap();
        let initial_probabilities = initial.probabilities();
        let mut left = initial.clone();
        let mut right = initial.clone();
        for byte in b"shared A prefix" {
            left.push_byte(*byte);
            right.push_byte(*byte);
            assert_eq!(left.probabilities(), right.probabilities());
        }
        let prefix_probabilities = left.probabilities();
        left.push_byte(65);
        right.push_byte(90);
        assert_ne!(left.probabilities(), right.probabilities());
        assert_eq!(initial.probabilities(), initial_probabilities);
        assert_ne!(left.probabilities(), prefix_probabilities);
        let old_identity = initial.label();
        set(&mut bytes, 65 * 64, 0.5);
        assert_ne!(GruSide::from_bytes(&bytes).unwrap().label(), old_identity);
    }
}
