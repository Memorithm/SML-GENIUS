//! ENG-1d/BMX-ANF: the SML recurrent ANF byte model as a side input of the
//! Boolean context mixer.
//!
//! [`AnfCharSide`] wraps a [`CharAnfModel`] (local fan-in ANF recurrent
//! network + 8 chained readouts, trained on a PRIME text with
//! [`anf_char::train`]) and replays it causally, one byte at a time, on a
//! single lane. For each chain bit it reports the readout's KT probability
//! and the readout address (the ANF-selected context bits), which
//! [`crate::bit_mixer::BitMixer`] turns into two mixer inputs: a calibrated
//! probability and an adaptive counter keyed by the address.
//!
//! Determinism: the replay is integer/Boolean only. Training uses `f64`
//! losses; encoder and decoder each retrain from the same PRIME, and the
//! side label carries a SHA-256 fingerprint of the trained model, so any
//! platform-dependent training difference is refused as a model mismatch
//! instead of corrupting the decoded data.

use crate::{
    anf_char::{self, pack_bytes, CharAnfConfig, CharAnfModel, BYTE_BITS},
    anf_local::LocalAnfError,
    anf_local_incremental::ReadoutInput,
    bit_mixer::SideModel,
    elastic_ngram_paged_sha::{hex, Sha256},
};

/// Frozen training configuration of the side model (`anf-char-v1`).
#[must_use]
pub fn side_config() -> CharAnfConfig {
    CharAnfConfig::default()
}

/// Causal single-lane replay of a trained [`CharAnfModel`].
#[derive(Clone)]
pub struct AnfCharSide {
    model: CharAnfModel,
    source: Vec<u64>,
    next: Vec<u64>,
    products: Vec<u64>,
    started: bool,
    label: String,
}

/// Stable SHA-256 fingerprint of a trained model (gates, readouts, counts).
#[must_use]
pub fn model_fingerprint(model: &CharAnfModel) -> String {
    let mut h = Sha256::default();
    for gate in model.network.gates() {
        for &i in gate.inputs() {
            h.update(&(i as u64).to_le_bytes());
        }
        let table: Vec<u8> = gate.truth_table().iter().map(|&b| u8::from(b)).collect();
        h.update(&table);
    }
    for spec in &model.readouts {
        h.update(&(spec.target as u64).to_le_bytes());
        for input in &spec.inputs {
            let (tag, v) = match *input {
                ReadoutInput::Source(p) => (0u8, p),
                ReadoutInput::Target(b) => (1u8, b),
            };
            h.update(&[tag]);
            h.update(&(v as u64).to_le_bytes());
        }
    }
    for table in &model.counts {
        for c in table {
            h.update(&c[0].to_le_bytes());
            h.update(&c[1].to_le_bytes());
        }
    }
    hex(&h.finish())
}

impl AnfCharSide {
    /// Wraps a trained model; `label` should identify it uniquely.
    pub fn new(model: CharAnfModel) -> Result<Self, LocalAnfError> {
        if model.readouts.len() != BYTE_BITS || model.counts.len() != BYTE_BITS {
            return Err(LocalAnfError::Width(model.readouts.len()));
        }
        let n = model.network.state_bits();
        let width = n + model.network.input_bits();
        let products = vec![0u64; model.network.scratch_len()];
        let label = format!("anf-char-v1:{}", model_fingerprint(&model));
        Ok(Self {
            source: vec![0; width],
            next: vec![0; n],
            products,
            started: false,
            label,
            model,
        })
    }

    /// Trains the frozen side configuration on `prime` (90 % fit, last 10 %
    /// for the model's own early stopping) and wraps it.
    pub fn train_on(prime: &[u8]) -> Result<Self, LocalAnfError> {
        let cut = prime.len() * 9 / 10;
        let (fit, val) = prime.split_at(cut);
        let report = anf_char::train(side_config(), &pack_bytes(fit)?, &pack_bytes(val)?)?;
        Self::new(report.model)
    }

    /// The wrapped model.
    #[must_use]
    pub fn model(&self) -> &CharAnfModel {
        &self.model
    }
}

impl SideModel for AnfCharSide {
    fn label(&self) -> String {
        self.label.clone()
    }

    fn push_byte(&mut self, byte: u8) {
        let n = self.model.network.state_bits();
        for p in 0..BYTE_BITS {
            self.source[n + p] = u64::from((byte >> p) & 1);
        }
        self.model
            .network
            .step_lanes(&mut self.source, &mut self.next, &mut self.products);
        self.started = true;
    }

    fn predict(&self, bit_pos: u32, c0: u32) -> (i32, u32) {
        if !self.started {
            return (2048, 0);
        }
        let b = bit_pos as usize;
        let address =
            self.model.readouts[b]
                .inputs
                .iter()
                .enumerate()
                .fold(0usize, |a, (j, input)| {
                    let v = match *input {
                        ReadoutInput::Source(p) => (self.source[p] & 1) as usize,
                        ReadoutInput::Target(i) => ((c0 >> (b - 1 - i)) & 1) as usize,
                    };
                    a | (v << j)
                });
        let [c_zero, c_one] = self.model.counts[b][address];
        // KT estimate (c1 + 1/2) / (n + 1) on the 12-bit scale, in integers.
        let num = (2 * u64::from(c_one) + 1) * 4096;
        let den = 2 * (u64::from(c_zero) + u64::from(c_one)) + 2;
        let p = (num / den).clamp(1, 4095) as i32;
        (p, (address as u32) ^ (bit_pos << 24))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        anf_char::{evaluate, synthetic_text},
        bit_mixer::{BitMixer, MaskSet, MixerConfig, SideInputs},
        bool_coder::{decode_with_predictor, encode_with_predictor},
    };

    fn quick_config() -> CharAnfConfig {
        CharAnfConfig {
            free_gates: 8,
            max_epochs: 2,
            proposals_per_epoch: 8,
            ..CharAnfConfig::default()
        }
    }

    fn trained(text: &[u8]) -> CharAnfModel {
        let cut = text.len() * 9 / 10;
        anf_char::train(
            quick_config(),
            &pack_bytes(&text[..cut]).unwrap(),
            &pack_bytes(&text[cut..]).unwrap(),
        )
        .unwrap()
        .model
    }

    /// The causal single-lane replay gives exactly the batch evaluator's
    /// probabilities: summing its -log2 p over one lane-aligned stream
    /// equals `anf_char::evaluate` restricted to a single lane.
    #[test]
    fn causal_replay_matches_the_batch_evaluator() {
        let text = synthetic_text(40_000, 3);
        let model = trained(&text);
        // Build a probe whose 64 lanes are identical copies of one chunk, so
        // the batch evaluator's per-lane mean equals the single-lane value.
        let chunk: Vec<u8> = synthetic_text(400, 9);
        let probe: Vec<u8> = (0..64).flat_map(|_| chunk.clone()).collect();
        let batch = evaluate(&model, &pack_bytes(&probe).unwrap(), false).unwrap();
        let mut side = AnfCharSide::new(model).unwrap();
        let mut bits = 0.0f64;
        let mut tokens = 0u64;
        for (t, &byte) in chunk.iter().enumerate() {
            if t > 0 {
                let mut c0 = 1u32;
                for b in 0..8u32 {
                    let bit = (byte >> (7 - b)) & 1;
                    let (_, addr) = side.predict(b, c0);
                    let c = side.model.counts[b as usize][(addr & 0x00FF_FFFF) as usize];
                    let p = (f64::from(c[usize::from(bit)]) + 0.5) / (f64::from(c[0] + c[1]) + 1.0);
                    bits -= p.log2();
                    c0 = (c0 << 1) | u32::from(bit);
                }
                tokens += 1;
            }
            side.push_byte(byte);
        }
        let single = bits / tokens as f64;
        assert!(
            (single - batch.bits_per_byte).abs() < 1e-9,
            "{single} vs {}",
            batch.bits_per_byte
        );
    }

    #[test]
    fn side_input_round_trips_and_identity_binds_the_model() {
        let text = synthetic_text(30_000, 5);
        let (prime, data) = text.split_at(24_000);
        let model = trained(prime);
        let small = MixerConfig {
            table_bits: 16,
            match_bits: 14,
            mask_set: MaskSet::V3,
            ..MixerConfig::default()
        };
        let make = |m: &CharAnfModel| {
            let mut mixer = BitMixer::try_with_side(
                small,
                MaskSet::V3.masks(),
                Box::new(AnfCharSide::new(m.clone()).unwrap()),
                SideInputs::Both,
            )
            .unwrap();
            mixer.prime(prime);
            mixer
        };
        let mut enc = make(&model);
        let id = enc.identity();
        assert!(id.contains(";side=anf-char-v1:"));
        let encoded = encode_with_predictor(data, id.as_bytes(), &mut enc).unwrap();
        let mut dec = make(&model);
        assert_eq!(dec.identity(), id);
        assert_eq!(
            decode_with_predictor(&encoded.container, id.as_bytes(), &mut dec).unwrap(),
            data
        );
        // A different ANF model has a different identity.
        let other = trained(&text[6_000..]);
        assert_ne!(model_fingerprint(&other), model_fingerprint(&model));
    }

    #[test]
    fn retraining_from_the_same_prime_is_reproducible() {
        let text = synthetic_text(20_000, 7);
        assert_eq!(
            model_fingerprint(&trained(&text)),
            model_fingerprint(&trained(&text))
        );
    }
}
