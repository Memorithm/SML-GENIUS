//! Trainable recurrent two-expert MOR with Boolean ANF features.
//!
//! Research-stage credit assignment: each full sequence backpropagates through
//! the recurrent state, the soft MOR gate, and both expert logits. This is NOT
//! yet the disk-paged 25B implementation; it tests the differentiable core
//! before replacing the small arrays by sparse atlas pages.
#[derive(Clone, Debug)]
pub struct RecurrentMorAnf {
    /// [bias, bit_a, bit_b, bit_a XOR bit_b, bit_a AND bit_b, previous hidden]
    expert: [[f64; 6]; 2],
    router: [f64; 6],
    recurrent: [f64; 3],
}

#[derive(Clone, Copy, Debug)]
struct Step {
    x: [f64; 6],
    h: f64,
    prev_h: f64,
    pred: f64,
    gate: f64,
    expert: [f64; 2],
    label: f64,
}

fn logistic(v: f64) -> f64 {
    1.0 / (1.0 + (-v.clamp(-30.0, 30.0)).exp())
}
fn dot<const N: usize>(a: &[f64; N], b: &[f64; N]) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}
fn check_finite(v: f64) -> Result<f64, &'static str> {
    if v.is_finite() { Ok(v) } else { Err("nonfinite learned gradient or loss") }
}

impl Default for RecurrentMorAnf {
    fn default() -> Self {
        // Symmetry must be broken for the gate to receive nonzero gradient.
        Self {
            expert: [[-0.2, 0.4, -0.3, 0.4, -0.2, 0.1],
                     [0.3, -0.2, 0.4, -0.3, 0.2, -0.1]],
            router: [0.05, 0.2, -0.1, 0.3, -0.3, 0.2],
            recurrent: [0.12, -0.2, 0.3],
        }
    }
}

impl RecurrentMorAnf {
    pub const PARAMETERS: usize = 21;

    fn steps(&self, inputs: &[(bool, bool)], labels: &[bool])
        -> Result<Vec<Step>, &'static str>
    {
        if inputs.len() != labels.len() || inputs.is_empty() || inputs.len() > 4096 {
            return Err("invalid sequence length");
        }
        let mut state = 0.0;
        let mut result = Vec::with_capacity(inputs.len());
        for (&(a, b), &label) in inputs.iter().zip(labels) {
            let previous = state;
            let aa = f64::from(u8::from(a));
            let bb = f64::from(u8::from(b));
            let xor = f64::from(u8::from(a ^ b));
            let and = f64::from(u8::from(a & b));
            // Recurrence retains context through the preceding state.
            state = (self.recurrent[0] * previous
                + self.recurrent[1] * xor + self.recurrent[2] * and).tanh();
            let x = [1.0, aa, bb, xor, and, state];
            let gate = logistic(dot(&self.router, &x));
            let expert = [
                logistic(dot(&self.expert[0], &x)),
                logistic(dot(&self.expert[1], &x)),
            ];
            let pred = gate * expert[0] + (1.0 - gate) * expert[1];
            result.push(Step {
                x, h: state, prev_h: previous, pred, gate, expert,
                label: f64::from(u8::from(label)),
            });
        }
        Ok(result)
    }

    pub fn loss(&self, inputs: &[(bool, bool)], labels: &[bool])
        -> Result<f64, &'static str>
    {
        let steps = self.steps(inputs, labels)?;
        let mut total = 0.0;
        for step in &steps {
            let p = step.pred.clamp(1e-12, 1.0 - 1e-12);
            total -= step.label * p.ln() + (1.0 - step.label) * (1.0 - p).ln();
        }
        check_finite(total / steps.len() as f64)
    }

    /// Full-sequence BPTT, including d(loss)/d(gate) and d(loss)/d(state).
    /// Gradients are normalized by sequence length, then globally norm-clipped.
    pub fn train_sequence(
        &mut self,
        inputs: &[(bool, bool)],
        labels: &[bool],
        learning_rate: f64,
    ) -> Result<f64, &'static str> {
        if !learning_rate.is_finite() || learning_rate <= 0.0 || learning_rate > 1.0 {
            return Err("invalid learning rate");
        }
        let steps = self.steps(inputs, labels)?;
        let mut e_grad = [[0.0; 6]; 2];
        let mut r_grad = [0.0; 6];
        let mut h_grad = [0.0; 3];
        let mut next_state_grad = 0.0;
        let mut loss = 0.0;
        for step in steps.iter().rev() {
            let p = step.pred.clamp(1e-12, 1.0 - 1e-12);
            loss -= step.label * p.ln() + (1.0 - step.label) * (1.0 - p).ln();
            let d_pred = (p - step.label) / (p * (1.0 - p));
            let d_gate = d_pred * (step.expert[0] - step.expert[1])
                * step.gate * (1.0 - step.gate);
            let d_expert = [
                d_pred * step.gate * step.expert[0] * (1.0 - step.expert[0]),
                d_pred * (1.0 - step.gate) * step.expert[1]
                    * (1.0 - step.expert[1]),
            ];
            for (i, &v) in step.x.iter().enumerate() {
                r_grad[i] += d_gate * v;
                for expert in 0..2 { e_grad[expert][i] += d_expert[expert] * v; }
            }
            let mut d_state = next_state_grad + d_gate * self.router[5];
            for expert in 0..2 {
                d_state += d_expert[expert] * self.expert[expert][5];
            }
            let d_pre = d_state * (1.0 - step.h * step.h);
            h_grad[0] += d_pre * step.prev_h;
            h_grad[1] += d_pre * step.x[3];
            h_grad[2] += d_pre * step.x[4];
            next_state_grad = d_pre * self.recurrent[0];
        }
        let denom = steps.len() as f64;
        let norm2: f64 = e_grad.iter().flatten().chain(r_grad.iter())
            .chain(h_grad.iter()).map(|x| (x / denom).powi(2)).sum();
        let scale = 1.0 / norm2.sqrt().max(1.0);
        for i in 0..6 {
            self.router[i] = check_finite(self.router[i] - learning_rate * r_grad[i] / denom * scale)?;
            for expert in 0..2 {
                self.expert[expert][i] = check_finite(
                    self.expert[expert][i] - learning_rate * e_grad[expert][i] / denom * scale
                )?;
            }
        }
        for i in 0..3 {
            self.recurrent[i] = check_finite(
                self.recurrent[i] - learning_rate * h_grad[i] / denom * scale
            )?;
        }
        check_finite(loss / denom)
    }

    pub fn router_weights(&self) -> [f64; 6] { self.router }
    pub fn recurrent_weights(&self) -> [f64; 3] { self.recurrent }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trainable_routing_anf_and_recurrence_receive_gradients() {
        let inputs: Vec<_> = (0..96).map(|n| (n & 1 != 0, n % 3 == 0)).collect();
        let labels: Vec<_> = inputs.iter().map(|&(a, b)| a ^ b).collect();
        let mut model = RecurrentMorAnf::default();
        let before = model.loss(&inputs, &labels).unwrap();
        let old_router = model.router_weights();
        let old_recurrent = model.recurrent_weights();
        for _ in 0..300 {
            model.train_sequence(&inputs, &labels, 0.5).unwrap();
        }
        let after = model.loss(&inputs, &labels).unwrap();
        assert!(after < before * 0.3, "before={before} after={after}");
        assert_ne!(old_router, model.router_weights());
        assert_ne!(old_recurrent, model.recurrent_weights());
    }

    #[test]
    fn invalid_inputs_are_rejected() {
        let mut m = RecurrentMorAnf::default();
        assert!(m.train_sequence(&[], &[], 0.2).is_err());
        assert!(m.train_sequence(&[(true, false)], &[], 0.2).is_err());
        assert!(m.train_sequence(&[(true, false)], &[true], f64::NAN).is_err());
        assert!(m.loss(&[(true, false)], &[true]).unwrap().is_finite());
    }

    #[test]
    fn recurrent_gradient_finite_difference_check() {
        let inputs = [(true, false), (false, true), (true, true), (false, false)];
        let labels = [true, false, true, false];
        let m = RecurrentMorAnf::default();
        let mut changed = m.clone();
        changed.train_sequence(&inputs, &labels, 1e-5).unwrap();
        let actual = (m.recurrent[0] - changed.recurrent[0]) / 1e-5;
        let mut a = m.clone();
        a.recurrent[0] += 1e-5;
        let mut b = m.clone();
        b.recurrent[0] -= 1e-5;
        let numerical = (a.loss(&inputs, &labels).unwrap()
            - b.loss(&inputs, &labels).unwrap()) / 2e-5;
        assert!((actual - numerical).abs() < 1e-7, "{actual} vs {numerical}");
    }
}
