//! Reference baselines for the A5 toy sequence tasks (same data and splits).
//!
//! * [`ConstantBaseline`]: majority training target, ignores the input.
//! * [`NGramBaseline`]: lookup table on the last `w` inputs (zero padded),
//!   majority target per context, unseen contexts fall back to the constant.
//!   Optimal for any task whose target is a function of the last `w` inputs.
//! * [`ElmanRnn`]: tiny f32 recurrent network `h_t = tanh(W x_t + U h_{t-1} + b)`,
//!   `y_t = sigmoid(v . h_t + c)`, trained by full backpropagation through time
//!   with Adam, minibatches and validation early stopping. Deterministic per seed.

use crate::toy_tasks::{accuracy, SplitMix64, ToySequence};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConstantBaseline(pub bool);

impl ConstantBaseline {
    pub fn fit(train: &[ToySequence]) -> Self {
        let (ones, total) = train.iter().fold((0usize, 0usize), |(o, t), s| {
            (
                o + s.targets.iter().filter(|&&b| b).count(),
                t + s.targets.len(),
            )
        });
        Self(ones * 2 > total)
    }
    pub fn predict(&self, inputs: &[bool]) -> Vec<bool> {
        vec![self.0; inputs.len()]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NGramBaseline {
    window: usize,
    table: Vec<Option<bool>>,
    fallback: bool,
}

impl NGramBaseline {
    fn context(inputs: &[bool], t: usize, window: usize) -> usize {
        (0..window).fold(0, |a, back| {
            a | (usize::from(t.checked_sub(back).is_some_and(|i| inputs[i])) << back)
        })
    }
    /// `window` in `1..=20`.
    pub fn fit(train: &[ToySequence], window: usize) -> Self {
        assert!((1..=20).contains(&window));
        let mut counts = vec![[0u32; 2]; 1 << window];
        for s in train {
            for t in 0..s.inputs.len() {
                counts[Self::context(&s.inputs, t, window)][usize::from(s.targets[t])] += 1;
            }
        }
        Self {
            window,
            table: counts
                .iter()
                .map(|c| (c[0] + c[1] > 0).then_some(c[1] > c[0]))
                .collect(),
            fallback: ConstantBaseline::fit(train).0,
        }
    }
    pub fn predict(&self, inputs: &[bool]) -> Vec<bool> {
        (0..inputs.len())
            .map(|t| self.table[Self::context(inputs, t, self.window)].unwrap_or(self.fallback))
            .collect()
    }
    pub fn table_bits(&self) -> usize {
        self.table.len()
    }
}

/// Training budget of the Elman RNN.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RnnConfig {
    pub hidden: usize,
    pub max_epochs: u32,
    pub batch: usize,
    pub learning_rate: f32,
    pub patience: u32,
    pub seed: u64,
}

impl Default for RnnConfig {
    fn default() -> Self {
        Self {
            hidden: 16,
            max_epochs: 300,
            batch: 16,
            learning_rate: 0.01,
            patience: 40,
            seed: 0,
        }
    }
}

/// Parameters, flattened: `w[H] | u[H*H] | b[H] | v[H] | c`.
#[derive(Debug, Clone, PartialEq)]
pub struct ElmanRnn {
    hidden: usize,
    params: Vec<f32>,
}

impl ElmanRnn {
    fn len(hidden: usize) -> usize {
        hidden * hidden + 3 * hidden + 1
    }
    pub fn parameter_count(&self) -> usize {
        self.params.len()
    }
    fn split(&self) -> (&[f32], &[f32], &[f32], &[f32], f32) {
        let h = self.hidden;
        let p = &self.params;
        (
            &p[..h],
            &p[h..h + h * h],
            &p[h + h * h..2 * h + h * h],
            &p[2 * h + h * h..3 * h + h * h],
            p[3 * h + h * h],
        )
    }
    fn forward(&self, inputs: &[bool]) -> (Vec<Vec<f32>>, Vec<f32>) {
        let h = self.hidden;
        let (w, u, b, v, c) = self.split();
        let mut states = vec![vec![0.0f32; h]];
        let mut outputs = Vec::with_capacity(inputs.len());
        for &x in inputs {
            let prev = states.last().expect("initial state");
            let xf = if x { 1.0 } else { 0.0 };
            let next: Vec<f32> = (0..h)
                .map(|i| {
                    let pre = w[i] * xf
                        + b[i]
                        + u[i * h..(i + 1) * h]
                            .iter()
                            .zip(prev)
                            .map(|(a, p)| a * p)
                            .sum::<f32>();
                    pre.tanh()
                })
                .collect();
            let logit = c + v.iter().zip(&next).map(|(a, s)| a * s).sum::<f32>();
            outputs.push(1.0 / (1.0 + (-logit).exp()));
            states.push(next);
        }
        (states, outputs)
    }
    pub fn predict(&self, inputs: &[bool]) -> Vec<bool> {
        self.forward(inputs)
            .1
            .into_iter()
            .map(|p| p > 0.5)
            .collect()
    }
    /// Adds the BCE gradient of one sequence to `grad`.
    fn backward(&self, sequence: &ToySequence, grad: &mut [f32]) {
        let h = self.hidden;
        let (_, u, _, v, _) = self.split();
        let (states, outputs) = self.forward(&sequence.inputs);
        let (ow, ou, ob, ov, oc) = (0, h, h + h * h, 2 * h + h * h, 3 * h + h * h);
        let mut dh_next = vec![0.0f32; h];
        for t in (0..sequence.inputs.len()).rev() {
            let target = if sequence.targets[t] { 1.0 } else { 0.0 };
            let dlogit = outputs[t] - target;
            grad[oc] += dlogit;
            let s = &states[t + 1];
            let prev = &states[t];
            let xf = if sequence.inputs[t] { 1.0 } else { 0.0 };
            let mut dpre = vec![0.0f32; h];
            for i in 0..h {
                grad[ov + i] += dlogit * s[i];
                let dh = dlogit * v[i] + dh_next[i];
                dpre[i] = dh * (1.0 - s[i] * s[i]);
            }
            let mut dprev = vec![0.0f32; h];
            for i in 0..h {
                grad[ow + i] += dpre[i] * xf;
                grad[ob + i] += dpre[i];
                for j in 0..h {
                    grad[ou + i * h + j] += dpre[i] * prev[j];
                    dprev[j] += dpre[i] * u[i * h + j];
                }
            }
            dh_next = dprev;
        }
    }

    /// Trains with Adam on minibatches; keeps the best-validation parameters.
    pub fn train(config: RnnConfig, train: &[ToySequence], val: &[ToySequence]) -> (Self, u32) {
        let h = config.hidden;
        let mut rng = SplitMix64::new(config.seed ^ 0x0e1a_a000_0000_0001);
        let scale = 1.0 / (h as f32).sqrt();
        let mut model = Self {
            hidden: h,
            params: (0..Self::len(h))
                .map(|_| rng.next_signed_unit() * scale)
                .collect(),
        };
        let mut m = vec![0.0f32; model.params.len()];
        let mut s = vec![0.0f32; model.params.len()];
        let (beta1, beta2, eps) = (0.9f32, 0.999f32, 1e-8f32);
        let mut step = 0i32;
        let mut order: Vec<usize> = (0..train.len()).collect();
        let mut best = (accuracy(val, |x| model.predict(x)), model.clone());
        let mut since = 0;
        let mut epochs = 0;
        while epochs < config.max_epochs && since < config.patience && best.0 < 1.0 {
            epochs += 1;
            for i in (1..order.len()).rev() {
                order.swap(i, rng.below(i + 1));
            }
            for batch in order.chunks(config.batch) {
                let mut grad = vec![0.0f32; model.params.len()];
                for &index in batch {
                    model.backward(&train[index], &mut grad);
                }
                let tokens: usize = batch.iter().map(|&i| train[i].inputs.len()).sum();
                let norm = (grad.iter().map(|g| g * g).sum::<f32>()).sqrt() / tokens as f32;
                let clip = if norm > 1.0 { 1.0 / norm } else { 1.0 };
                step += 1;
                for k in 0..grad.len() {
                    let g = grad[k] / tokens as f32 * clip;
                    m[k] = beta1 * m[k] + (1.0 - beta1) * g;
                    s[k] = beta2 * s[k] + (1.0 - beta2) * g * g;
                    let mh = m[k] / (1.0 - beta1.powi(step));
                    let sh = s[k] / (1.0 - beta2.powi(step));
                    model.params[k] -= config.learning_rate * mh / (sh.sqrt() + eps);
                }
            }
            let acc = accuracy(val, |x| model.predict(x));
            if acc > best.0 {
                best = (acc, model.clone());
                since = 0;
            } else {
                since += 1;
            }
        }
        (best.1, epochs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toy_tasks::{ToySplitSpec, ToySplits, ToyTask};

    #[test]
    fn ngram_is_exact_on_window_tasks_and_constant_is_majority() {
        let s = ToySplits::new(ToyTask::WindowParity(3), ToySplitSpec::default(), 1);
        let ngram = NGramBaseline::fit(&s.train, 4);
        assert_eq!(accuracy(&s.test_long, |x| ngram.predict(x)), 1.0);
        let constant = ConstantBaseline::fit(&s.train);
        let acc = accuracy(&s.test, |x| constant.predict(x));
        assert!((0.4..0.6).contains(&acc), "{acc}");
    }

    #[test]
    fn rnn_gradient_matches_finite_differences() {
        let s = ToySplits::new(ToyTask::DelayedRecall(1), ToySplitSpec::default(), 2);
        let model = ElmanRnn {
            hidden: 3,
            params: (0..ElmanRnn::len(3))
                .map(|i| ((i * 37 % 11) as f32 - 5.0) / 10.0)
                .collect(),
        };
        let seq = &s.train[0];
        let loss = |m: &ElmanRnn| -> f64 {
            let (_, out) = m.forward(&seq.inputs);
            out.iter()
                .zip(&seq.targets)
                .map(|(&p, &t)| -f64::from(if t { p } else { 1.0 - p }).ln())
                .sum()
        };
        let mut grad = vec![0.0f32; model.parameter_count()];
        model.backward(seq, &mut grad);
        for (k, &analytic) in grad.iter().enumerate() {
            let mut plus = model.clone();
            plus.params[k] += 1e-2;
            let mut minus = model.clone();
            minus.params[k] -= 1e-2;
            let numeric = (loss(&plus) - loss(&minus)) / 2e-2;
            assert!(
                (numeric - f64::from(analytic)).abs() < 2e-2 * (1.0 + numeric.abs()),
                "k={k} {numeric} {analytic}"
            );
        }
    }

    #[test]
    fn rnn_learns_delay_one_recall() {
        let s = ToySplits::new(ToyTask::DelayedRecall(1), ToySplitSpec::default(), 3);
        let (model, _) = ElmanRnn::train(RnnConfig::default(), &s.train, &s.val);
        assert!(accuracy(&s.test, |x| model.predict(x)) > 0.95);
    }
}
