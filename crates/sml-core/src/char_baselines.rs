//! Byte-level baselines for the A5 step-3 character task: Witten-Bell
//! interpolated n-grams (unigram = context 0) and a small Elman RNN (f32).
//!
//! Every model is scored on exactly the tokens of [`crate::anf_char::ChunkLayout`]:
//! the split is cut into 64 contiguous chunks, the first byte of each chunk is
//! not predicted and the context resets at chunk starts.

use std::collections::HashMap;

use crate::anf_char::{ChunkLayout, LANES};
use crate::toy_tasks::SplitMix64;

/// Bits per byte and argmax accuracy on a split.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ByteScore {
    pub tokens: u64,
    pub bits_per_byte: f64,
    pub accuracy: f64,
}

fn score_layout<F: FnMut(&[u8], &mut [f64; 256])>(bytes: &[u8], mut predict: F) -> ByteScore {
    let layout = match ChunkLayout::new(bytes.len()) {
        Ok(layout) => layout,
        Err(_) => {
            return ByteScore {
                tokens: 0,
                bits_per_byte: f64::NAN,
                accuracy: f64::NAN,
            }
        }
    };
    let mut probs = [0.0f64; 256];
    let (mut bits, mut correct, mut tokens) = (0.0, 0u64, 0u64);
    for lane in 0..LANES {
        let chunk = &bytes[lane * layout.chunk..(lane + 1) * layout.chunk];
        for t in 1..chunk.len() {
            predict(&chunk[..t], &mut probs);
            let y = usize::from(chunk[t]);
            bits -= probs[y].log2();
            let best = (0..256).fold(0, |b, v| if probs[v] > probs[b] { v } else { b });
            correct += u64::from(best == y);
            tokens += 1;
        }
    }
    ByteScore {
        tokens,
        bits_per_byte: bits / tokens as f64,
        accuracy: correct as f64 / tokens as f64,
    }
}

#[derive(Debug, Clone, Default)]
struct ContextStats {
    total: u32,
    followers: Vec<(u8, u32)>,
}

/// Interpolated Witten-Bell n-gram with context length `order` (0 = unigram),
/// backing off to the uniform distribution over 256 bytes.
#[derive(Debug, Clone)]
pub struct NGramModel {
    order: usize,
    levels: Vec<HashMap<u64, ContextStats>>,
}

fn context_key(history: &[u8], len: usize) -> u64 {
    history[history.len() - len..]
        .iter()
        .fold(0u64, |k, &b| k << 8 | u64::from(b))
}

impl NGramModel {
    /// Counts on the same chunked layout as scoring (no context across chunks).
    pub fn train(bytes: &[u8], order: usize) -> Self {
        assert!(order <= 7);
        let mut raw: Vec<HashMap<u64, HashMap<u8, u32>>> = vec![HashMap::new(); order + 1];
        if let Ok(layout) = ChunkLayout::new(bytes.len()) {
            for lane in 0..LANES {
                let chunk = &bytes[lane * layout.chunk..(lane + 1) * layout.chunk];
                for t in 1..chunk.len() {
                    for (len, level) in raw.iter_mut().enumerate() {
                        if len <= t {
                            *level
                                .entry(context_key(&chunk[..t], len))
                                .or_default()
                                .entry(chunk[t])
                                .or_default() += 1;
                        }
                    }
                }
            }
        }
        let levels = raw
            .into_iter()
            .map(|level| {
                level
                    .into_iter()
                    .map(|(key, followers)| {
                        let mut followers: Vec<(u8, u32)> = followers.into_iter().collect();
                        followers.sort_unstable();
                        let total = followers.iter().map(|f| f.1).sum();
                        (key, ContextStats { total, followers })
                    })
                    .collect()
            })
            .collect();
        Self { order, levels }
    }

    /// Stored (context, byte) counts.
    pub fn parameters(&self) -> usize {
        self.levels
            .iter()
            .flat_map(|l| l.values())
            .map(|s| s.followers.len())
            .sum()
    }

    /// Next-byte distribution after `history` (at least one byte of context).
    pub fn distribution(&self, history: &[u8], probs: &mut [f64; 256]) {
        probs.iter_mut().for_each(|p| *p = 1.0 / 256.0);
        for len in 0..=self.order.min(history.len()) {
            if let Some(stats) = self.levels[len].get(&context_key(history, len)) {
                let n = f64::from(stats.total);
                let distinct = stats.followers.len() as f64;
                let scale = distinct / (n + distinct);
                probs.iter_mut().for_each(|p| *p *= scale);
                for &(byte, count) in &stats.followers {
                    probs[usize::from(byte)] += f64::from(count) / (n + distinct);
                }
            }
        }
    }

    pub fn score(&self, bytes: &[u8]) -> ByteScore {
        score_layout(bytes, |history, probs| self.distribution(history, probs))
    }
}

/// Elman RNN byte model: `h_t = tanh(E[x_t] + W h_{t-1} + b)`, softmax output.
#[derive(Debug, Clone)]
pub struct ByteRnn {
    hidden: usize,
    embed: Vec<f32>,
    recur: Vec<f32>,
    bias: Vec<f32>,
    out: Vec<f32>,
    out_bias: Vec<f32>,
}

/// RNN training configuration (truncated BPTT over the 64 chunk lanes, Adam).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ByteRnnConfig {
    pub hidden: usize,
    pub epochs: usize,
    pub window: usize,
    pub learning_rate: f32,
    pub seed: u64,
}

impl Default for ByteRnnConfig {
    fn default() -> Self {
        Self {
            hidden: 64,
            epochs: 6,
            window: 32,
            learning_rate: 3e-3,
            seed: 0,
        }
    }
}

struct Adam {
    m: Vec<f32>,
    v: Vec<f32>,
}

impl Adam {
    fn new(len: usize) -> Self {
        Self {
            m: vec![0.0; len],
            v: vec![0.0; len],
        }
    }
    fn step(&mut self, params: &mut [f32], grads: &[f32], lr: f32, t: i32) {
        let (b1, b2) = (0.9f32, 0.999f32);
        let c1 = 1.0 - b1.powi(t);
        let c2 = 1.0 - b2.powi(t);
        for i in 0..params.len() {
            let g = grads[i].clamp(-1.0, 1.0);
            self.m[i] = b1 * self.m[i] + (1.0 - b1) * g;
            self.v[i] = b2 * self.v[i] + (1.0 - b2) * g * g;
            params[i] -= lr * (self.m[i] / c1) / ((self.v[i] / c2).sqrt() + 1e-8);
        }
    }
}

impl ByteRnn {
    pub fn new(hidden: usize, seed: u64) -> Self {
        let mut rng = SplitMix64::new(seed ^ 0x0b7e_5eed);
        let scale = 1.0 / (hidden as f32).sqrt();
        let mut init = |len: usize, s: f32| -> Vec<f32> {
            (0..len).map(|_| rng.next_signed_unit() * s).collect()
        };
        Self {
            hidden,
            embed: init(256 * hidden, 0.1),
            recur: init(hidden * hidden, scale),
            bias: vec![0.0; hidden],
            out: init(256 * hidden, scale),
            out_bias: vec![0.0; 256],
        }
    }

    pub fn parameters(&self) -> usize {
        self.embed.len() + self.recur.len() + self.bias.len() + self.out.len() + self.out_bias.len()
    }

    fn step_hidden(&self, x: u8, prev: &[f32], next: &mut [f32]) {
        let h = self.hidden;
        let e = &self.embed[usize::from(x) * h..(usize::from(x) + 1) * h];
        for i in 0..h {
            let row = &self.recur[i * h..(i + 1) * h];
            let mut a = e[i] + self.bias[i];
            for j in 0..h {
                a += row[j] * prev[j];
            }
            next[i] = a.tanh();
        }
    }

    fn logits_to_probs(&self, hidden: &[f32], probs: &mut [f32]) {
        let h = self.hidden;
        let mut max = f32::NEG_INFINITY;
        for (y, p) in probs.iter_mut().enumerate() {
            let row = &self.out[y * h..(y + 1) * h];
            let mut z = self.out_bias[y];
            for j in 0..h {
                z += row[j] * hidden[j];
            }
            *p = z;
            max = max.max(z);
        }
        let mut sum = 0.0;
        for p in probs.iter_mut() {
            *p = (*p - max).exp();
            sum += *p;
        }
        for p in probs.iter_mut() {
            *p /= sum;
        }
    }

    /// Trains on `train` (chunk layout), keeping the parameters with the best
    /// validation bits per byte after each epoch. Returns per-epoch val bpb.
    pub fn train(config: ByteRnnConfig, train: &[u8], val: &[u8]) -> (Self, Vec<f64>) {
        let mut model = Self::new(config.hidden, config.seed);
        let h = config.hidden;
        let Ok(layout) = ChunkLayout::new(train.len()) else {
            return (model, Vec::new());
        };
        let steps = layout.chunk - 1;
        let lanes: Vec<&[u8]> = (0..LANES)
            .map(|l| &train[l * layout.chunk..(l + 1) * layout.chunk])
            .collect();
        let mut adam: Vec<Adam> = [256 * h, h * h, h, 256 * h, 256]
            .iter()
            .map(|&len| Adam::new(len))
            .collect();
        let mut best = (f64::INFINITY, model.clone());
        let mut history = Vec::new();
        let mut t_adam = 0;
        let w = config.window;
        let mut probs = vec![0.0f32; 256];
        for _ in 0..config.epochs {
            let mut state = vec![vec![0.0f32; h]; LANES];
            let mut start = 0;
            while start < steps {
                let end = (start + w).min(steps);
                let len = end - start;
                let mut g_embed = vec![0.0f32; 256 * h];
                let mut g_recur = vec![0.0f32; h * h];
                let mut g_bias = vec![0.0f32; h];
                let mut g_out = vec![0.0f32; 256 * h];
                let mut g_out_bias = vec![0.0f32; 256];
                for (lane, bytes) in lanes.iter().enumerate() {
                    // Forward over the window, storing hidden states.
                    let mut hs = vec![0.0f32; (len + 1) * h];
                    hs[..h].copy_from_slice(&state[lane]);
                    let mut dlogits = vec![0.0f32; len * 256];
                    for s in 0..len {
                        let (prev, next) = hs.split_at_mut((s + 1) * h);
                        model.step_hidden(bytes[start + s], &prev[s * h..], &mut next[..h]);
                        model.logits_to_probs(&next[..h], &mut probs);
                        let y = usize::from(bytes[start + s + 1]);
                        let d = &mut dlogits[s * 256..(s + 1) * 256];
                        d.copy_from_slice(&probs);
                        d[y] -= 1.0;
                    }
                    state[lane].copy_from_slice(&hs[len * h..]);
                    // Backward through time.
                    let mut dh_next = vec![0.0f32; h];
                    let mut da = vec![0.0f32; h];
                    for s in (0..len).rev() {
                        let hcur = &hs[(s + 1) * h..(s + 2) * h];
                        let hprev = &hs[s * h..(s + 1) * h];
                        let d = &dlogits[s * 256..(s + 1) * 256];
                        let mut dh = dh_next.clone();
                        for y in 0..256 {
                            let dy = d[y];
                            g_out_bias[y] += dy;
                            let row = &model.out[y * h..(y + 1) * h];
                            let grow = &mut g_out[y * h..(y + 1) * h];
                            for j in 0..h {
                                grow[j] += dy * hcur[j];
                                dh[j] += dy * row[j];
                            }
                        }
                        for i in 0..h {
                            da[i] = dh[i] * (1.0 - hcur[i] * hcur[i]);
                        }
                        let x = usize::from(lanes[lane][start + s]);
                        for i in 0..h {
                            g_embed[x * h + i] += da[i];
                            g_bias[i] += da[i];
                        }
                        dh_next.iter_mut().for_each(|v| *v = 0.0);
                        for i in 0..h {
                            let row = &model.recur[i * h..(i + 1) * h];
                            let grow = &mut g_recur[i * h..(i + 1) * h];
                            for j in 0..h {
                                grow[j] += da[i] * hprev[j];
                                dh_next[j] += da[i] * row[j];
                            }
                        }
                    }
                }
                let scale = 1.0 / (LANES * len) as f32;
                t_adam += 1;
                let lr = config.learning_rate;
                for (index, (params, grads)) in [
                    (&mut model.embed, &mut g_embed),
                    (&mut model.recur, &mut g_recur),
                    (&mut model.bias, &mut g_bias),
                    (&mut model.out, &mut g_out),
                    (&mut model.out_bias, &mut g_out_bias),
                ]
                .into_iter()
                .enumerate()
                {
                    grads.iter_mut().for_each(|g| *g *= scale);
                    adam[index].step(params, grads, lr, t_adam);
                }
                start = end;
            }
            let val_bits = model.score(val).bits_per_byte;
            history.push(val_bits);
            if val_bits < best.0 {
                best = (val_bits, model.clone());
            }
        }
        (best.1, history)
    }

    pub fn score(&self, bytes: &[u8]) -> ByteScore {
        let h = self.hidden;
        let mut state = vec![0.0f32; h];
        let mut next = vec![0.0f32; h];
        let mut probs32 = vec![0.0f32; 256];
        let mut seen = 0usize;
        score_layout(bytes, |history, probs| {
            // Histories arrive in order within a chunk: extend the state by one byte,
            // or restart at a chunk start.
            if history.len() != seen + 1 {
                state.iter_mut().for_each(|v| *v = 0.0);
                for &b in &history[..history.len() - 1] {
                    self.step_hidden(b, &state, &mut next);
                    state.copy_from_slice(&next);
                }
            }
            self.step_hidden(history[history.len() - 1], &state, &mut next);
            state.copy_from_slice(&next);
            seen = history.len();
            self.logits_to_probs(&state, &mut probs32);
            for (p, &q) in probs.iter_mut().zip(&probs32) {
                *p = f64::from(q.max(1e-12));
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anf_char::synthetic_text;

    #[test]
    fn ngram_distributions_are_normalised_and_order_helps() {
        let text = synthetic_text(64 * 120, 7);
        let (train, test) = text.split_at(64 * 100);
        let mut probs = [0.0; 256];
        let mut previous = f64::INFINITY;
        for order in 0..=3 {
            let model = NGramModel::train(train, order);
            model.distribution(b"le ch", &mut probs);
            assert!((probs.iter().sum::<f64>() - 1.0).abs() < 1e-9);
            let score = model.score(test);
            assert_eq!(score.tokens, 64 * 19);
            assert!(score.bits_per_byte < previous, "order {order}: {score:?}");
            previous = score.bits_per_byte;
        }
        // Unseen bytes keep non-zero mass through the uniform floor.
        NGramModel::train(train, 2).distribution(b"zz", &mut probs);
        assert!(probs[usize::from(b'#')] > 0.0);
    }

    #[test]
    fn rnn_training_lowers_validation_bits() {
        let text = synthetic_text(64 * 40, 8);
        let (train, val) = text.split_at(64 * 30);
        let config = ByteRnnConfig {
            hidden: 12,
            epochs: 3,
            window: 8,
            learning_rate: 1e-2,
            seed: 1,
        };
        let untrained = ByteRnn::new(12, 1).score(val).bits_per_byte;
        let (model, history) = ByteRnn::train(config, train, val);
        assert_eq!(history.len(), 3);
        let trained = model.score(val).bits_per_byte;
        assert!(trained < untrained - 1.0, "{untrained} -> {trained}");
    }
}
