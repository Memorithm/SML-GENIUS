//! RTRL-3: bounded-memory elastic online eligibility, not a trained 25B model.
//!
//! A diagonal trainable recurrent cell + a fixed rank-r cross-state operator.
//! The exact recurrent Jacobian has H*(3H) entries. We instead maintain
//! unbiased rank-one UORO factors for that Jacobian, costing O(K*H) f64.
//! This is a stochastic approximation, not exact dense RTRL for H > 1.
//!
//! No Transformer attention, NVIDIA dependency, unsafe Rust, or dense atlas.
use std::error::Error;

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const BITS: usize = 8;
const LOCAL: usize = 3;
const EPS: f64 = 1e-12;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rtrl3Config {
    pub hidden: usize,
    pub eligibility_rank: usize,
    pub mixing_rank: usize,
    pub learning_rate: f64,
    pub recurrent_clip_norm: f64,
    pub seed: u64,
}

impl Rtrl3Config {
    pub fn validate(self) -> Result<Self> {
        if !(1..=64).contains(&self.hidden)
            || !(1..=16).contains(&self.eligibility_rank)
            || self.mixing_rank > 8
            || !(0.0..=0.05).contains(&self.learning_rate)
            || !self.learning_rate.is_finite()
            || self.learning_rate == 0.0
            || !self.recurrent_clip_norm.is_finite()
            || self.recurrent_clip_norm <= 0.0
            || self.seed == 0
        {
            return Err("invalid RTRL-3 bounded elastic configuration".into());
        }
        Ok(self)
    }
}

#[derive(Clone, Debug)]
struct Rng(u64);

impl Rng {
    fn next_sign(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        if x.wrapping_mul(0x2545_f491_4f6c_dd1d) & 1 == 0 {
            -1.0
        } else {
            1.0
        }
    }
}

#[derive(Clone, Debug)]
struct Factor {
    // J approximately equals the average outer product u_k v_k^T.
    u: Vec<f64>,
    v: Vec<f64>,
    rng: Rng,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rtrl3Metrics {
    pub trained_bytes: u64,
    pub trained_bits: u64,
    pub logloss_bits: f64,
    pub clipped_steps: u64,
    pub maximum_gradient_norm: f64,
}

/// Diagnostics for the shared physical MOR / ANF RTRL-3 recurrent update.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MorRtrl3Update {
    pub changed_parameters: u64,
    pub clipped: bool,
    pub preclip_gradient_norm: f64,
}

#[derive(Clone, Debug)]
pub struct ElasticRtrl3 {
    cfg: Rtrl3Config,
    local: Vec<[f64; LOCAL]>,
    // Fixed low-rank cross-state recurrence U V / H.
    coupling_u: Vec<f64>,
    coupling_v: Vec<f64>,
    h: Vec<f64>,
    exact_scalar: [f64; LOCAL],
    factors: Vec<Factor>,
    head_gain: [f64; BITS],
    head_bias: [f64; BITS],
}

fn signal(value: u8) -> f64 {
    (f64::from(value.count_ones()) - 4.0) / 4.0
}

fn anf(a: u8, b: u8) -> f64 {
    if (a & b).count_ones() % 2 == 0 {
        -1.0
    } else {
        1.0
    }
}

fn logistic(x: f64) -> f64 {
    1.0 / (1.0 + (-x.clamp(-16.0, 16.0)).exp())
}

fn vector_norm(v: &[f64]) -> f64 {
    let scale = v.iter().fold(0.0f64, |peak, &x| peak.max(x.abs()));
    if scale == 0.0 {
        0.0
    } else {
        scale * v.iter().map(|x| (x / scale).powi(2)).sum::<f64>().sqrt()
    }
}

impl ElasticRtrl3 {
    pub fn new(cfg: Rtrl3Config) -> Result<Self> {
        let cfg = cfg.validate()?;
        let mut local = Vec::with_capacity(cfg.hidden);
        for index in 0..cfg.hidden {
            local.push([0.55 + 0.25 * index as f64 / cfg.hidden as f64, 0.3, 0.2]);
        }
        // Cross coupling is fixed in this prototype. Its gradient is not
        // claimed to be learned by the local three-coefficient eligibility.
        let mut coupling_u = vec![0.0; cfg.hidden * cfg.mixing_rank];
        let mut coupling_v = vec![0.0; cfg.mixing_rank * cfg.hidden];
        for (i, u) in coupling_u.iter_mut().enumerate() {
            *u = if i % 3 == 0 { 0.04 } else { -0.04 };
        }
        for (i, v) in coupling_v.iter_mut().enumerate() {
            *v = if (i.wrapping_mul(17) + 3) % 5 < 2 {
                1.0
            } else {
                -1.0
            };
        }
        let mut factors = Vec::with_capacity(cfg.eligibility_rank);
        for index in 0..cfg.eligibility_rank {
            let salt = (index as u64 + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15);
            factors.push(Factor {
                u: vec![0.0; cfg.hidden],
                v: vec![0.0; LOCAL * cfg.hidden],
                rng: Rng(cfg.seed ^ salt),
            });
        }
        Ok(Self {
            cfg,
            local,
            coupling_u,
            coupling_v,
            h: vec![0.0; cfg.hidden],
            exact_scalar: [0.0; LOCAL],
            factors,
            head_gain: std::array::from_fn(|i| if i % 2 == 0 { 0.15 } else { -0.15 }),
            head_bias: [0.0; BITS],
        })
    }

    pub fn config(&self) -> Rtrl3Config {
        self.cfg
    }

    /// Instantiate recurrent state for the original physical paged MOR head.
    /// Identical local [0.65, 0.3, 0.2] coefficients make H=1,M=0
    /// an exact control of the existing RTRL-2 scalar trajectory.
    pub fn for_mor(cfg: Rtrl3Config) -> Result<Self> {
        let mut model = Self::new(cfg)?;
        for weights in &mut model.local {
            *weights = [0.65, 0.3, 0.2];
        }
        Ok(model)
    }

    /// State and eligibility evolve before each of the 8 MOR target bits.
    /// This method never changes either expert's physical page weights.
    pub fn advance_mor(&mut self, a: u8, b: u8) -> Result<f64> {
        self.advance(a, b)?;
        let mean = self.mean_state();
        if !mean.is_finite() {
            return Err("nonfinite RTRL-3 MOR recurrence".into());
        }
        Ok(mean)
    }

    /// Apply the recurrent credit produced by the *same physical* MOR router
    /// loss. Expert and router weight updates occur in the caller exactly
    /// as in RTRL-2; no unrelated standalone logistic readout is trained.
    pub fn learn_mor_credit(&mut self, credit: f64) -> Result<MorRtrl3Update> {
        if !credit.is_finite() {
            return Err("nonfinite MOR recurrent credit".into());
        }
        let h = self.cfg.hidden;
        let mut gradient = vec![0.0f64; LOCAL * h];
        if h == 1 && self.cfg.mixing_rank == 0 {
            for (p, value) in gradient.iter_mut().enumerate() {
                *value = credit * self.exact_scalar[p];
            }
        } else {
            for factor in &self.factors {
                let mean_u = factor.u.iter().sum::<f64>() / h as f64;
                for (p, value) in gradient.iter_mut().enumerate() {
                    *value += credit * mean_u * factor.v[p] / self.cfg.eligibility_rank as f64;
                }
            }
        }
        let norm = vector_norm(&gradient);
        if !norm.is_finite() {
            return Err("nonfinite RTRL-3 MOR norm".into());
        }
        let clipped = norm > self.cfg.recurrent_clip_norm;
        let scale = if clipped {
            self.cfg.recurrent_clip_norm / norm
        } else {
            1.0
        };
        let mut updated = 0u64;
        for (j, weights) in self.local.iter_mut().enumerate() {
            for (p, weight) in weights.iter_mut().enumerate() {
                let before = *weight;
                let after = if h == 1 && self.cfg.mixing_rank == 0 && !clipped {
                    // Exact RTRL-2 left-associative recurrent SGD arithmetic.
                    (before - self.cfg.learning_rate * credit * self.exact_scalar[p])
                        .clamp(-2.0, 2.0)
                } else {
                    (before - self.cfg.learning_rate * gradient[LOCAL * j + p] * scale)
                        .clamp(-2.0, 2.0)
                };
                if !after.is_finite() {
                    return Err("nonfinite RTRL-3 MOR recurrent parameter".into());
                }
                if after != before {
                    updated += 1;
                }
                *weight = after;
            }
        }
        Ok(MorRtrl3Update {
            changed_parameters: updated,
            clipped,
            preclip_gradient_norm: norm,
        })
    }

    /// Read-only inference path: advance only the recurrent state, not
    /// eligibility sketches, trained weights or random-sign generators.
    /// Caller uses a reset clone per independent document.
    pub fn advance_mor_inference(&mut self, a: u8, b: u8) -> Result<f64> {
        let h = self.cfg.hidden;
        let r = self.cfg.mixing_rank;
        let previous = self.h.clone();
        let mut projected = vec![0.0; r];
        for (k, value) in projected.iter_mut().enumerate() {
            for (j, &state) in previous.iter().enumerate() {
                *value += self.coupling_v[k * h + j] * state / h as f64;
            }
        }
        for (j, state) in self.h.iter_mut().enumerate() {
            let mut z = self.local[j][0] * previous[j]
                + self.local[j][1] * signal(b)
                + self.local[j][2] * anf(a, b);
            for (k, &value) in projected.iter().enumerate() {
                z += self.coupling_u[j * r + k] * value;
            }
            *state = z.tanh();
        }
        let mean = self.mean_state();
        if !mean.is_finite() {
            return Err("nonfinite RTRL-3 MOR inference state".into());
        }
        Ok(mean)
    }

    /// Persist the actual local recurrent weights and configuration as
    /// an auditable sidecar without storing volatile eligibility sketches.
    /// The fixed U,V recurrence coupling is deterministically recreated.
    pub fn write_mor_weights(&self, path: &std::path::Path) -> Result<()> {
        use std::io::Write;
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        writeln!(
            output,
            "RTRL3_MOR_LOCAL_V1\t{}\t{}\t{}\t{}\t{}\t{}",
            self.cfg.hidden,
            self.cfg.eligibility_rank,
            self.cfg.mixing_rank,
            self.cfg.learning_rate,
            self.cfg.recurrent_clip_norm,
            self.cfg.seed
        )?;
        writeln!(output, "index\trecurrent\tsignal\tanf")?;
        for (j, weights) in self.local.iter().enumerate() {
            writeln!(
                output,
                "{j}\t{:.17e}\t{:.17e}\t{:.17e}",
                weights[0], weights[1], weights[2]
            )?;
        }
        output.sync_all()?;
        Ok(())
    }

    /// Actual f64 payload, excluding Vec metadata; O(K*H + H*r), never H*3H.
    pub fn resident_payload_bytes(&self) -> usize {
        let h = self.cfg.hidden;
        let k = self.cfg.eligibility_rank;
        let r = self.cfg.mixing_rank;
        // Weights + state + scalar trace + fixed low-rank mixing + UORO.
        (3 * h + h + 3 + 2 * h * r + k * 4 * h + 16) * size_of::<f64>()
    }

    pub fn recurrent_trainable_parameters(&self) -> usize {
        self.cfg.hidden * LOCAL
    }

    pub fn reset_state(&mut self) {
        self.h.fill(0.0);
        self.exact_scalar.fill(0.0);
        for factor in &mut self.factors {
            factor.u.fill(0.0);
            factor.v.fill(0.0);
        }
    }

    // Return local input features and d(tanh) for this step, retaining
    // previous recurrent state for the direct parameter derivatives.
    fn advance(&mut self, a: u8, b: u8) -> Result<()> {
        let h = self.cfg.hidden;
        let r = self.cfg.mixing_rank;
        let previous = self.h.clone();
        let features = [signal(b), anf(a, b)];
        let mut projected = vec![0.0; r];
        for (k, item) in projected.iter_mut().enumerate() {
            for (j, &state) in previous.iter().enumerate() {
                *item += self.coupling_v[k * h + j] * state / h as f64;
            }
        }
        let mut derivatives = vec![0.0; h];
        for (j, derivative) in derivatives.iter_mut().enumerate() {
            let mut sum = self.local[j][0] * previous[j]
                + self.local[j][1] * features[0]
                + self.local[j][2] * features[1];
            for (k, &value) in projected.iter().enumerate() {
                sum += self.coupling_u[j * r + k] * value;
            }
            self.h[j] = sum.tanh();
            *derivative = 1.0 - self.h[j] * self.h[j];
        }
        if h == 1 && r == 0 {
            for (p, sensitivity) in self.exact_scalar.iter_mut().enumerate() {
                let direct = [previous[0], features[0], features[1]][p];
                *sensitivity = derivatives[0] * (direct + self.local[0][0] * *sensitivity);
            }
            return Ok(());
        }
        for factor in &mut self.factors {
            let mut projection = vec![0.0; r];
            for (k, item) in projection.iter_mut().enumerate() {
                for (j, &u) in factor.u.iter().enumerate() {
                    *item += self.coupling_v[k * h + j] * u / h as f64;
                }
            }
            let mut propagated = vec![0.0; h];
            for (j, value) in propagated.iter_mut().enumerate() {
                *value = self.local[j][0] * factor.u[j];
                for (k, &projected_u) in projection.iter().enumerate() {
                    *value += self.coupling_u[j * r + k] * projected_u;
                }
                *value *= derivatives[j];
            }
            // Rademacher sketch: E[nu nu^T] = I. B has three
            // nonzero derivatives per hidden row, so build nu^T B in O(H).
            let mut nu = vec![0.0; h];
            let mut v_b = vec![0.0; LOCAL * h];
            for (j, sign) in nu.iter_mut().enumerate() {
                *sign = factor.rng.next_sign();
                let direct = [previous[j], features[0], features[1]];
                for p in 0..LOCAL {
                    v_b[LOCAL * j + p] = *sign * derivatives[j] * direct[p];
                }
            }
            let sign = factor.rng.next_sign();
            let au_norm = vector_norm(&propagated);
            let old_v_norm = vector_norm(&factor.v);
            let alpha = if au_norm < EPS || old_v_norm < EPS {
                1.0
            } else {
                (old_v_norm / au_norm).sqrt().clamp(0.125, 8.0)
            };
            let beta = (vector_norm(&v_b) / vector_norm(&nu).max(EPS))
                .sqrt()
                .clamp(0.125, 8.0);
            for (j, value) in factor.u.iter_mut().enumerate() {
                *value = alpha * propagated[j] + sign * beta * nu[j];
            }
            for (p, value) in factor.v.iter_mut().enumerate() {
                *value = *value / alpha + sign * v_b[p] / beta;
            }
            if !factor
                .u
                .iter()
                .chain(factor.v.iter())
                .all(|value| value.is_finite())
            {
                return Err("nonfinite RTRL-3 stochastic eligibility".into());
            }
        }
        Ok(())
    }

    fn mean_state(&self) -> f64 {
        self.h.iter().sum::<f64>() / self.cfg.hidden as f64
    }

    /// Produce online predictions, then update 8-bit readout and recurrent
    /// parameters. Inherited eligibility across weight updates is approximate.
    pub fn train_step(&mut self, a: u8, b: u8, target: u8) -> Result<(f64, bool, f64)> {
        self.advance(a, b)?;
        let mean = self.mean_state();
        let mut credit = 0.0;
        let mut loss = 0.0;
        for bit in 0..BITS {
            let y = f64::from((target >> bit) & 1);
            let p =
                logistic(self.head_gain[bit] * mean + self.head_bias[bit]).clamp(EPS, 1.0 - EPS);
            loss -= y * p.log2() + (1.0 - y) * (1.0 - p).log2();
            let delta = p - y;
            // Read credit before changing the head's parameters.
            credit += delta * self.head_gain[bit];
            self.head_gain[bit] =
                (self.head_gain[bit] - self.cfg.learning_rate * delta * mean).clamp(-4.0, 4.0);
            self.head_bias[bit] =
                (self.head_bias[bit] - self.cfg.learning_rate * delta).clamp(-4.0, 4.0);
        }
        let mut gradient = vec![0.0; LOCAL * self.cfg.hidden];
        if self.cfg.hidden == 1 && self.cfg.mixing_rank == 0 {
            for (p, value) in gradient.iter_mut().enumerate() {
                *value = credit * self.exact_scalar[p];
            }
        } else {
            for factor in &self.factors {
                let mean_u = factor.u.iter().sum::<f64>() / self.cfg.hidden as f64;
                for (p, value) in gradient.iter_mut().enumerate() {
                    *value += credit * mean_u * factor.v[p] / self.cfg.eligibility_rank as f64;
                }
            }
        }
        let norm = vector_norm(&gradient);
        if !loss.is_finite() || !norm.is_finite() {
            return Err("nonfinite RTRL-3 loss or recurrent gradient".into());
        }
        let clipped = norm > self.cfg.recurrent_clip_norm;
        let scale = if clipped {
            self.cfg.recurrent_clip_norm / norm
        } else {
            1.0
        };
        for (j, parameters) in self.local.iter_mut().enumerate() {
            for (p, parameter) in parameters.iter_mut().enumerate() {
                *parameter = (*parameter
                    - self.cfg.learning_rate * gradient[LOCAL * j + p] * scale)
                    .clamp(-2.0, 2.0);
            }
        }
        Ok((loss, clipped, norm))
    }

    pub fn train_document(&mut self, text: &[u8]) -> Result<Rtrl3Metrics> {
        if text.len() < 3 {
            return Err("RTRL-3 requires two causal bytes and a target".into());
        }
        self.reset_state();
        let mut metric = Rtrl3Metrics::default();
        for sample in text.windows(3) {
            let (loss, clipped, norm) = self.train_step(sample[0], sample[1], sample[2])?;
            metric.maximum_gradient_norm = metric.maximum_gradient_norm.max(norm);
            metric.logloss_bits += loss;
            metric.trained_bytes += 1;
            metric.trained_bits += BITS as u64;
            if clipped {
                metric.clipped_steps += 1;
            }
        }
        if !metric.logloss_bits.is_finite() {
            return Err("nonfinite RTRL-3 document loss".into());
        }
        Ok(metric)
    }

    /// Strictly read-only evaluation, fresh causal state, no RNG or optimizer.
    pub fn evaluate_document(&self, text: &[u8]) -> Result<(f64, u64)> {
        if text.len() < 3 {
            return Err("RTRL-3 evaluation requires three bytes".into());
        }
        let h = self.cfg.hidden;
        let r = self.cfg.mixing_rank;
        let mut state = vec![0.0; h];
        let mut loss = 0.0;
        for [a, b, target] in text.windows(3).map(|x| [x[0], x[1], x[2]]) {
            let mut projection = vec![0.0; r];
            for (k, item) in projection.iter_mut().enumerate() {
                for (j, &v) in state.iter().enumerate() {
                    *item += self.coupling_v[k * h + j] * v / h as f64;
                }
            }
            let mut next = vec![0.0; h];
            for (j, value) in next.iter_mut().enumerate() {
                let mut z = self.local[j][0] * state[j]
                    + self.local[j][1] * signal(b)
                    + self.local[j][2] * anf(a, b);
                for (k, &projected) in projection.iter().enumerate() {
                    z += self.coupling_u[j * r + k] * projected;
                }
                *value = z.tanh();
            }
            state = next;
            let mean = state.iter().sum::<f64>() / h as f64;
            for bit in 0..BITS {
                let y = f64::from((target >> bit) & 1);
                let p = logistic(self.head_gain[bit] * mean + self.head_bias[bit])
                    .clamp(EPS, 1.0 - EPS);
                loss -= y * p.log2() + (1.0 - y) * (1.0 - p).log2();
            }
        }
        if !loss.is_finite() {
            return Err("nonfinite RTRL-3 evaluation".into());
        }
        Ok((loss, (text.len() - 2) as u64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(hidden: usize, mixing_rank: usize, rank: usize, seed: u64) -> Rtrl3Config {
        Rtrl3Config {
            hidden,
            mixing_rank,
            eligibility_rank: rank,
            learning_rate: 0.005,
            recurrent_clip_norm: 1.0,
            seed,
        }
    }

    #[test]
    fn scalar_exact_eligibility_matches_rtrl2_analytical_trace() {
        let mut model = ElasticRtrl3::new(config(1, 0, 1, 42)).unwrap();
        model.local[0] = [0.65, 0.3, 0.2];
        let mut expected_h = 0.0;
        let mut expected = [0.0; 3];
        for (a, b) in [(b'a', b'b'), (b'c', b'd'), (b'e', b'f')] {
            let previous = expected_h;
            expected_h = (0.65 * previous + 0.3 * signal(b) + 0.2 * anf(a, b)).tanh();
            let d = 1.0 - expected_h * expected_h;
            let features = [previous, signal(b), anf(a, b)];
            for (p, sensitivity) in expected.iter_mut().enumerate() {
                *sensitivity = d * (features[p] + 0.65 * *sensitivity);
            }
            model.advance(a, b).unwrap();
            assert!((model.h[0] - expected_h).abs() < 1e-14);
            for (actual, wanted) in model.exact_scalar.iter().zip(expected) {
                assert!((actual - wanted).abs() < 1e-14);
            }
        }
    }

    fn terminal_mean(mut model: ElasticRtrl3, inputs: &[(u8, u8)]) -> f64 {
        model.reset_state();
        for &(a, b) in inputs {
            model.advance(a, b).unwrap();
        }
        model.mean_state()
    }

    #[test]
    fn two_state_low_rank_coupling_exact_reference_matches_finite_differences() {
        let model = ElasticRtrl3::new(config(2, 1, 1, 13)).unwrap();
        let inputs = [(b'a', b'b'), (b'c', b'd'), (b'e', b'f'), (b'g', b'h')];
        let h = model.cfg.hidden;
        let p = LOCAL * h;
        let mut state = vec![0.0; h];
        let mut jac = vec![vec![0.0; p]; h];
        for &(a, b) in &inputs {
            let previous = state.clone();
            let old = jac.clone();
            let mut projection = vec![0.0; model.cfg.mixing_rank];
            for (k, out) in projection.iter_mut().enumerate() {
                for (j, &prev) in previous.iter().enumerate() {
                    *out += model.coupling_v[k * h + j] * prev / h as f64;
                }
            }
            for (i, value) in state.iter_mut().enumerate() {
                let mut z = model.local[i][0] * previous[i]
                    + model.local[i][1] * signal(b)
                    + model.local[i][2] * anf(a, b);
                for (k, &projected) in projection.iter().enumerate() {
                    z += model.coupling_u[i * model.cfg.mixing_rank + k] * projected;
                }
                *value = z.tanh();
                let d = 1.0 - *value * *value;
                for parameter in 0..p {
                    let mut slope = model.local[i][0] * old[i][parameter];
                    for k in 0..model.cfg.mixing_rank {
                        for (j, item) in old.iter().enumerate() {
                            slope += model.coupling_u[i * model.cfg.mixing_rank + k]
                                * model.coupling_v[k * h + j]
                                * item[parameter]
                                / h as f64;
                        }
                    }
                    if parameter / LOCAL == i {
                        slope += [previous[i], signal(b), anf(a, b)][parameter % LOCAL];
                    }
                    jac[i][parameter] = d * slope;
                }
            }
        }
        for parameter in 0..p {
            let mut plus = model.clone();
            let mut minus = model.clone();
            plus.local[parameter / LOCAL][parameter % LOCAL] += 1e-6;
            minus.local[parameter / LOCAL][parameter % LOCAL] -= 1e-6;
            let numeric = (terminal_mean(plus, &inputs) - terminal_mean(minus, &inputs)) / 2e-6;
            let exact = jac.iter().map(|row| row[parameter]).sum::<f64>() / h as f64;
            assert!(
                (exact - numeric).abs() < 1e-7,
                "{parameter}: {exact}, {numeric}"
            );
        }
    }

    #[test]
    fn low_rank_uoro_is_unbiased_in_seed_ensemble_for_one_step() {
        let mut sums = [0.0; 6];
        let count = 4096usize;
        for seed in 1..=count {
            let mut model = ElasticRtrl3::new(config(2, 1, 1, seed as u64)).unwrap();
            model.advance(b'a', b'z').unwrap();
            let factor = &model.factors[0];
            let mean_u = factor.u.iter().sum::<f64>() / 2.0;
            for (p, value) in sums.iter_mut().enumerate() {
                *value += mean_u * factor.v[p] / count as f64;
            }
        }
        let reference = ElasticRtrl3::new(config(2, 1, 1, 1)).unwrap();
        let mut z = [0.0; 2];
        for (i, value) in z.iter_mut().enumerate() {
            *value = (reference.local[i][1] * signal(b'z')
                + reference.local[i][2] * anf(b'a', b'z'))
            .tanh();
        }
        for (p, &estimate) in sums.iter().enumerate() {
            let j = p / LOCAL;
            let direct = [0.0, signal(b'z'), anf(b'a', b'z')][p % LOCAL];
            let exact = (1.0 - z[j] * z[j]) * direct / 2.0;
            assert!(
                (estimate - exact).abs() < 0.075,
                "{p}: {estimate} vs {exact}"
            );
        }
    }

    #[test]
    fn bounded_memory_and_training_have_finite_read_only_evaluation() {
        for (h, r, k) in [(1, 0, 1), (8, 2, 2), (16, 4, 4), (32, 4, 8)] {
            let cfg = config(h, r, k, 99);
            let mut model = ElasticRtrl3::new(cfg).unwrap();
            assert_eq!(model.recurrent_trainable_parameters(), 3 * h);
            let bound = (4 * h + 3 + 2 * h * r + k * 4 * h + 16) * 8;
            assert_eq!(model.resident_payload_bytes(), bound);
            let training = model.train_document(b"aabbccddeeffaabbccddeeff").unwrap();
            assert_eq!(training.trained_bits, training.trained_bytes * 8);
            assert!(training.logloss_bits.is_finite());
            assert!(training.maximum_gradient_norm.is_finite());
            assert!(training.maximum_gradient_norm >= 0.0);
            assert!(training.clipped_steps <= training.trained_bytes);
            let (before, events) = model.evaluate_document(b"abacabadabacaba").unwrap();
            let (after, after_events) = model.evaluate_document(b"abacabadabacaba").unwrap();
            assert_eq!(events, after_events);
            assert_eq!(before, after);
        }
    }

    #[test]
    fn mor_scalar_exact_online_gradient_and_inference_match_rtrl2_equations() {
        let mut model = ElasticRtrl3::for_mor(config(1, 0, 1, 1)).unwrap();
        model.cfg.recurrent_clip_norm = 1e30;
        let mut reference = [0.65, 0.3, 0.2];
        let mut state = 0.0;
        let mut sensitivity = [0.0f64; 3];
        for (a, b) in [(b'a', b'z'), (b'c', b'd'), (b'e', b'f'), (b'g', b'h')] {
            let prev = state;
            state =
                (reference[0] * prev + reference[1] * signal(b) + reference[2] * anf(a, b)).tanh();
            let d = 1.0 - state * state;
            let direct = [prev, signal(b), anf(a, b)];
            for (p, value) in sensitivity.iter_mut().enumerate() {
                *value = d * (direct[p] + reference[0] * *value);
            }
            let actual = model.advance_mor(a, b).unwrap();
            assert_eq!(actual, state);
            let credit = -0.25;
            let update = model.learn_mor_credit(credit).unwrap();
            assert!(!update.clipped);
            for (p, value) in reference.iter_mut().enumerate() {
                *value = (*value - 0.005 * credit * sensitivity[p]).clamp(-2.0, 2.0);
            }
            assert_eq!(model.local[0], reference);
        }
        let mut inference = model.clone();
        inference.reset_state();
        let mut training_trace = model.clone();
        training_trace.reset_state();
        for (a, b) in [(b'a', b'b'), (b'c', b'd')] {
            let actual = inference.advance_mor_inference(a, b).unwrap();
            let expect = training_trace.advance_mor(a, b).unwrap();
            assert_eq!(actual, expect);
        }
    }

    #[test]
    fn mor_multi_state_updates_are_bounded_and_reject_nonfinite_credit() {
        for (h, m, k) in [(8, 2, 2), (16, 2, 4)] {
            let mut model = ElasticRtrl3::for_mor(config(h, m, k, 100)).unwrap();
            assert_eq!(model.recurrent_trainable_parameters(), 3 * h);
            for _ in 0..20 {
                assert!(model.advance_mor(b'a', b'b').unwrap().is_finite());
                let update = model.learn_mor_credit(0.3).unwrap();
                assert!(update.preclip_gradient_norm.is_finite());
            }
            assert!(model.learn_mor_credit(f64::NAN).is_err());
        }
    }

    #[test]
    fn rejects_invalid_dimensions_and_norms_without_allocation() {
        for (h, r, k) in [(0, 0, 1), (65, 0, 1), (2, 9, 1), (2, 1, 0), (2, 1, 17)] {
            assert!(ElasticRtrl3::new(config(h, r, k, 1)).is_err());
        }
        let mut bad = config(2, 1, 1, 1);
        bad.recurrent_clip_norm = f64::NAN;
        assert!(ElasticRtrl3::new(bad).is_err());
        bad.recurrent_clip_norm = 1.0;
        bad.seed = 0;
        assert!(ElasticRtrl3::new(bad).is_err());
    }
}
