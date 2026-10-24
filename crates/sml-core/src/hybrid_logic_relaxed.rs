//! Relaxed, end-to-end trainable mode of the hybrid Boolean engine (HLE-1).
//!
//! The hard engine in [`crate::hybrid_logic_engine`] routes with integer
//! Hamming distance, executes hard pages and XOR-rewrites a bit workspace.
//! This module is its relaxation, trained jointly by backprop through time:
//!
//! * the workspace is a vector of probabilities in `[0,1]`;
//! * the routing signature is the relaxed XOR `s = x + w - 2xw` of the input
//!   and the workspace (low `W` bits, as in the hard engine);
//! * each page owns key logits; its relaxed key is `q = sigmoid(kappa)` and
//!   its relaxed Hamming distance is `d = sum_j s_j + q_j - 2 s_j q_j`;
//! * routing is a **straight-through top-k**: forward selects the `k`
//!   smallest `d` (ties by `PageId`) as a hard 0/1 gate; backward uses the
//!   surrogate `sigmoid((tau - d) / T)`, where `tau` is the midpoint between
//!   the k-th and (k+1)-th distance (treated as a constant). Distances are
//!   compared per page against a threshold; there is no softmax, no
//!   normalisation across pages, no Q/K/V projection and no KV cache;
//! * every page is a [`DiffLogicNetwork`] evaluated on `input ++ workspace`;
//!   gated outputs `g * y` are folded with the relaxed XOR `a + b - 2ab`
//!   and XOR-applied to the workspace;
//! * exactly `cycles` cycles are unrolled; the loss is masked binary
//!   cross-entropy on the final workspace. The gradient flows through every
//!   cycle into all page parameters and all key logits.
//!
//! [`RelaxedHybridEngine::discretize`] freezes pages (`theta > 0`) and keys
//! (`kappa > 0`) into a hard [`HybridLogicEngine`] with
//! `min_cycles = 1, max_cycles = cycles`. Because a fixed point is absorbing
//! in the hard engine (same workspace, same route, zero delta), early COMMIT
//! does not change the committed workspace relative to unrolling `cycles`.
//!
//! No performance or quality claim is made; tests check gradients, training
//! on a toy task and hard/relaxed agreement only.

use core::fmt;

use crate::{
    diff_logic::{DiffLogicNetwork, ForwardTrace, TrainMode},
    hybrid_logic_engine::{HybridConfig, HybridError, HybridLogicEngine, HybridPage},
    PageId,
};

fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

/// One trainable page: key logits plus a differentiable logic network.
#[derive(Debug, Clone, PartialEq)]
pub struct RelaxedPage {
    pub id: PageId,
    /// One logit per workspace bit; hard key bit `j` is `key_logits[j] > 0`.
    pub key_logits: Vec<f64>,
    pub network: DiffLogicNetwork,
}

/// Relaxed engine configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RelaxedConfig {
    pub input_bits: usize,
    pub workspace_bits: usize,
    pub pages_per_cycle: usize,
    /// Number of unrolled cycles (hard engine: `max_cycles`).
    pub cycles: usize,
    /// Surrogate temperature `T > 0` for the straight-through router.
    pub route_temperature: f64,
    /// Multiplier on the routing surrogate gradient. `0.0` gives the exact
    /// gradient of the piecewise-smooth forward (routing held fixed).
    pub route_grad_scale: f64,
}

/// One training/evaluation example. Bits above the configured widths must
/// be zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelaxedExample {
    pub input: u64,
    pub initial_workspace: u64,
    pub target: u64,
    /// Workspace bits that enter the loss.
    pub target_mask: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RelaxedError {
    InvalidConfig(&'static str),
    DuplicatePage(PageId),
    PageShape(PageId),
    BadBatch,
    Hard(HybridError),
}

impl fmt::Display for RelaxedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(why) => write!(f, "invalid relaxed config: {why}"),
            Self::DuplicatePage(id) => write!(f, "duplicate page {}", id.0),
            Self::PageShape(id) => write!(f, "page {} has an incompatible shape", id.0),
            Self::BadBatch => write!(f, "empty batch or out-of-range example"),
            Self::Hard(e) => write!(f, "discretisation failed: {e}"),
        }
    }
}

impl std::error::Error for RelaxedError {}

struct CycleTrace {
    ws: Vec<f64>,
    sig: Vec<f64>,
    dist: Vec<f64>,
    gate: Vec<f64>,
    tau: f64,
    traces: Vec<ForwardTrace>,
    /// `acc[p]` = XOR fold of gated outputs of pages `0..p`.
    acc: Vec<Vec<f64>>,
    delta: Vec<f64>,
}

/// Relaxed recurrent engine; see the module documentation.
#[derive(Debug, Clone, PartialEq)]
pub struct RelaxedHybridEngine {
    config: RelaxedConfig,
    pages: Vec<RelaxedPage>,
}

fn bits(word: u64, n: usize) -> Vec<f64> {
    (0..n).map(|j| ((word >> j) & 1) as f64).collect()
}

impl RelaxedHybridEngine {
    pub fn new(config: RelaxedConfig, pages: Vec<RelaxedPage>) -> Result<Self, RelaxedError> {
        let c = config;
        if c.input_bits == 0 || c.input_bits > 64 || c.workspace_bits == 0 || c.workspace_bits > 64
        {
            return Err(RelaxedError::InvalidConfig("bit widths must be 1..=64"));
        }
        if c.cycles == 0 {
            return Err(RelaxedError::InvalidConfig("cycles must be >= 1"));
        }
        if c.pages_per_cycle == 0 || c.pages_per_cycle > pages.len() {
            return Err(RelaxedError::InvalidConfig(
                "pages_per_cycle must be 1..=page count",
            ));
        }
        if c.route_temperature.is_nan()
            || c.route_temperature <= 0.0
            || c.route_temperature.is_infinite()
            || !c.route_grad_scale.is_finite()
        {
            return Err(RelaxedError::InvalidConfig(
                "route_temperature must be > 0 and route_grad_scale finite",
            ));
        }
        let mut ids: Vec<u32> = pages.iter().map(|p| p.id.0).collect();
        ids.sort_unstable();
        if let Some(w) = ids.windows(2).find(|w| w[0] == w[1]) {
            return Err(RelaxedError::DuplicatePage(PageId(w[0])));
        }
        for p in &pages {
            if p.key_logits.len() != c.workspace_bits
                || p.network.input_count() != c.input_bits + c.workspace_bits
                || p.network.output_count() != c.workspace_bits
            {
                return Err(RelaxedError::PageShape(p.id));
            }
        }
        Ok(Self { config, pages })
    }

    pub fn config(&self) -> RelaxedConfig {
        self.config
    }

    pub fn pages(&self) -> &[RelaxedPage] {
        &self.pages
    }

    fn page_param_len(p: &RelaxedPage) -> usize {
        p.network.parameter_count() + p.key_logits.len()
    }

    /// Total trainable parameters (per page: network, then key logits).
    pub fn parameter_count(&self) -> usize {
        self.pages.iter().map(Self::page_param_len).sum()
    }

    pub fn parameters(&self) -> Vec<f64> {
        let mut out = Vec::with_capacity(self.parameter_count());
        for p in &self.pages {
            out.extend_from_slice(p.network.parameters());
            out.extend_from_slice(&p.key_logits);
        }
        out
    }

    pub fn set_parameters(&mut self, params: &[f64]) {
        assert_eq!(params.len(), self.parameter_count());
        let mut off = 0;
        for p in &mut self.pages {
            let n = p.network.parameter_count();
            p.network
                .parameters_mut()
                .copy_from_slice(&params[off..off + n]);
            off += n;
            let k = p.key_logits.len();
            p.key_logits.copy_from_slice(&params[off..off + k]);
            off += k;
        }
    }

    fn check(&self, ex: &RelaxedExample) -> bool {
        let c = self.config;
        let ok = |v: u64, n: usize| n >= 64 || v >> n == 0;
        ok(ex.input, c.input_bits)
            && ok(ex.initial_workspace, c.workspace_bits)
            && ok(ex.target, c.workspace_bits)
            && ok(ex.target_mask, c.workspace_bits)
    }

    fn unroll(&self, input: u64, ws0: u64) -> (Vec<f64>, Vec<CycleTrace>) {
        let c = self.config;
        let (ni, nw) = (c.input_bits, c.workspace_bits);
        let x = bits(input, ni);
        let mut ws = bits(ws0, nw);
        let mut cycles = Vec::with_capacity(c.cycles);
        for _ in 0..c.cycles {
            let sig: Vec<f64> = (0..nw)
                .map(|j| {
                    let xj = x.get(j).copied().unwrap_or(0.0);
                    xj + ws[j] - 2.0 * xj * ws[j]
                })
                .collect();
            let dist: Vec<f64> = self
                .pages
                .iter()
                .map(|p| {
                    p.key_logits
                        .iter()
                        .zip(&sig)
                        .map(|(&k, &s)| {
                            let q = sigmoid(k);
                            s + q - 2.0 * s * q
                        })
                        .sum()
                })
                .collect();
            let mut order: Vec<usize> = (0..self.pages.len()).collect();
            order.sort_by(|&a, &b| {
                dist[a]
                    .total_cmp(&dist[b])
                    .then(self.pages[a].id.0.cmp(&self.pages[b].id.0))
            });
            let k = c.pages_per_cycle;
            let mut gate = vec![0.0; self.pages.len()];
            for &i in &order[..k] {
                gate[i] = 1.0;
            }
            let tau = if k < order.len() {
                0.5 * (dist[order[k - 1]] + dist[order[k]])
            } else {
                dist[order[k - 1]] + 1.0
            };
            let mut wires = x.clone();
            wires.extend_from_slice(&ws);
            let mut acc = vec![vec![0.0; nw]];
            let mut traces = Vec::with_capacity(self.pages.len());
            for (p, page) in self.pages.iter().enumerate() {
                let tr = page
                    .network
                    .forward(&wires, TrainMode::Relaxed)
                    .expect("shape validated");
                let prev = acc.last().expect("non-empty");
                let next = prev
                    .iter()
                    .zip(tr.outputs())
                    .map(|(&a, &y)| {
                        let cpy = gate[p] * y;
                        a + cpy - 2.0 * a * cpy
                    })
                    .collect();
                acc.push(next);
                traces.push(tr);
            }
            let delta = acc.pop().expect("non-empty");
            let next: Vec<f64> = ws
                .iter()
                .zip(&delta)
                .map(|(&w, &d)| w + d - 2.0 * w * d)
                .collect();
            cycles.push(CycleTrace {
                ws: std::mem::replace(&mut ws, next),
                sig,
                dist,
                gate,
                tau,
                traces,
                acc,
                delta,
            });
        }
        (ws, cycles)
    }

    /// Final relaxed workspace probabilities after `cycles` cycles.
    pub fn run_relaxed(&self, input: u64, initial_workspace: u64) -> Vec<f64> {
        self.unroll(input, initial_workspace).0
    }

    /// Mean masked BCE over the batch; writes the mean gradient into `grad`
    /// (layout of [`Self::parameters`]) using backprop through time.
    pub fn loss_and_grad(
        &self,
        batch: &[RelaxedExample],
        grad: &mut [f64],
    ) -> Result<f64, RelaxedError> {
        if batch.is_empty() || grad.len() != self.parameter_count() {
            return Err(RelaxedError::BadBatch);
        }
        if !batch.iter().all(|e| self.check(e)) {
            return Err(RelaxedError::BadBatch);
        }
        grad.iter_mut().for_each(|g| *g = 0.0);
        let c = self.config;
        let (ni, nw) = (c.input_bits, c.workspace_bits);
        let offsets: Vec<usize> = self
            .pages
            .iter()
            .scan(0, |o, p| {
                let cur = *o;
                *o += Self::page_param_len(p);
                Some(cur)
            })
            .collect();
        let n = batch.len() as f64;
        const EPS: f64 = 1e-7;
        let mut loss = 0.0;
        for ex in batch {
            let (out, cycles) = self.unroll(ex.input, ex.initial_workspace);
            let x = bits(ex.input, ni);
            let mut d_ws = vec![0.0; nw];
            for j in 0..nw {
                if (ex.target_mask >> j) & 1 == 1 {
                    let t = ((ex.target >> j) & 1) as f64;
                    let y = out[j].clamp(EPS, 1.0 - EPS);
                    loss -= t * y.ln() + (1.0 - t) * (1.0 - y).ln();
                    d_ws[j] = (y - t) / (y * (1.0 - y)) / n;
                }
            }
            for cy in cycles.iter().rev() {
                // ws' = ws XOR delta (relaxed)
                let mut d_prev: Vec<f64> = (0..nw)
                    .map(|j| d_ws[j] * (1.0 - 2.0 * cy.delta[j]))
                    .collect();
                let mut d_acc: Vec<f64> =
                    (0..nw).map(|j| d_ws[j] * (1.0 - 2.0 * cy.ws[j])).collect();
                let mut d_sig = vec![0.0; nw];
                for p in (0..self.pages.len()).rev() {
                    let page = &self.pages[p];
                    let y = cy.traces[p].outputs();
                    let a = &cy.acc[p];
                    let g = cy.gate[p];
                    let mut d_g = 0.0;
                    let mut d_y = vec![0.0; nw];
                    for j in 0..nw {
                        let cj = g * y[j];
                        let d_c = d_acc[j] * (1.0 - 2.0 * a[j]);
                        d_acc[j] *= 1.0 - 2.0 * cj;
                        d_g += d_c * y[j];
                        d_y[j] = g * d_c;
                    }
                    let off = offsets[p];
                    let np = page.network.parameter_count();
                    if g != 0.0 {
                        let d_in = page.network.backward_with_input_grad(
                            &cy.traces[p],
                            &d_y,
                            TrainMode::Relaxed,
                            &mut grad[off..off + np],
                        );
                        for j in 0..nw {
                            d_prev[j] += d_in[ni + j];
                        }
                    }
                    if c.route_grad_scale != 0.0 {
                        // Straight-through: d gate / d dist from the surrogate.
                        let s = sigmoid((cy.tau - cy.dist[p]) / c.route_temperature);
                        let d_dist =
                            -c.route_grad_scale * d_g * s * (1.0 - s) / c.route_temperature;
                        for j in 0..nw {
                            let q = sigmoid(page.key_logits[j]);
                            let sj = cy.sig[j];
                            grad[off + np + j] += d_dist * (1.0 - 2.0 * sj) * q * (1.0 - q);
                            d_sig[j] += d_dist * (1.0 - 2.0 * q);
                        }
                    }
                }
                for j in 0..nw {
                    let xj = x.get(j).copied().unwrap_or(0.0);
                    d_prev[j] += d_sig[j] * (1.0 - 2.0 * xj);
                }
                d_ws = d_prev;
            }
        }
        Ok(loss / n)
    }

    /// Freeze into the hard engine (`min_cycles = 1`, `max_cycles = cycles`).
    pub fn discretize(&self) -> Result<HybridLogicEngine, RelaxedError> {
        let c = self.config;
        let pages = self
            .pages
            .iter()
            .map(|p| HybridPage {
                id: p.id,
                key: p
                    .key_logits
                    .iter()
                    .enumerate()
                    .filter(|(_, &k)| k > 0.0)
                    .fold(0u64, |acc, (j, _)| acc | 1 << j),
                network: p.network.discretize(),
            })
            .collect();
        HybridLogicEngine::new(
            HybridConfig {
                input_bits: c.input_bits,
                workspace_bits: c.workspace_bits,
                pages_per_cycle: c.pages_per_cycle,
                min_cycles: 1,
                max_cycles: c.cycles,
            },
            pages,
        )
        .map_err(RelaxedError::Hard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{diff_logic::Adam, ExecutionCounters};

    fn engine(
        seed: u64,
        pages: usize,
        cfg: RelaxedConfig,
        widths: &[usize],
    ) -> RelaxedHybridEngine {
        let mut rng = seed;
        let pages = (0..pages)
            .map(|i| {
                let net = DiffLogicNetwork::new(
                    cfg.input_bits + cfg.workspace_bits,
                    widths,
                    seed.wrapping_mul(1000).wrapping_add(i as u64),
                    0.5,
                )
                .unwrap();
                let key_logits = (0..cfg.workspace_bits)
                    .map(|_| {
                        rng = rng
                            .wrapping_mul(6364136223846793005)
                            .wrapping_add(1442695040888963407);
                        ((rng >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
                    })
                    .collect();
                RelaxedPage {
                    id: PageId(i as u32),
                    key_logits,
                    network: net,
                }
            })
            .collect();
        RelaxedHybridEngine::new(cfg, pages).unwrap()
    }

    fn cfg(cycles: usize, scale: f64) -> RelaxedConfig {
        RelaxedConfig {
            input_bits: 3,
            workspace_bits: 2,
            pages_per_cycle: 1,
            cycles,
            route_temperature: 0.5,
            route_grad_scale: scale,
        }
    }

    /// Parity of three input bits into workspace bit 0; bit 1 is free.
    fn parity_task() -> Vec<RelaxedExample> {
        (0..8u64)
            .map(|x| RelaxedExample {
                input: x,
                initial_workspace: 0,
                target: u64::from(x.count_ones() % 2),
                target_mask: 1,
            })
            .collect()
    }

    #[test]
    fn bptt_matches_finite_differences_with_routing_held_fixed() {
        let e = engine(3, 3, cfg(3, 0.0), &[4, 2]);
        let batch = parity_task();
        let mut g = vec![0.0; e.parameter_count()];
        e.loss_and_grad(&batch, &mut g).unwrap();
        let base = e.parameters();
        let h = 1e-5;
        let mut scratch = vec![0.0; g.len()];
        let mut checked = 0;
        for i in 0..base.len() {
            let mut probe = |v: f64| {
                let mut e2 = e.clone();
                let mut p = base.clone();
                p[i] = v;
                e2.set_parameters(&p);
                let l = e2.loss_and_grad(&batch, &mut scratch).unwrap();
                (
                    l,
                    e2.unroll(0, 0)
                        .1
                        .iter()
                        .map(|c| c.gate.clone())
                        .collect::<Vec<_>>(),
                )
            };
            let (lp, rp) = probe(base[i] + h);
            let (lm, rm) = probe(base[i] - h);
            if rp != rm {
                continue; // routing flipped inside the probe; piecewise kink
            }
            let fd = (lp - lm) / (2.0 * h);
            assert!(
                (fd - g[i]).abs() < 1e-6 + 1e-4 * fd.abs(),
                "param {i}: fd {fd} vs analytic {}",
                g[i]
            );
            checked += 1;
        }
        assert!(checked > base.len() / 2);
        // Gradient reaches page parameters through more than one cycle.
        assert!(g.iter().any(|v| v.abs() > 1e-9));
    }

    #[test]
    fn routing_surrogate_trains_key_logits() {
        let e = engine(5, 4, cfg(2, 1.0), &[2]);
        let mut g = vec![0.0; e.parameter_count()];
        e.loss_and_grad(&parity_task(), &mut g).unwrap();
        let mut off = 0;
        let mut key_grad = 0.0f64;
        for p in e.pages() {
            off += p.network.parameter_count();
            for j in 0..p.key_logits.len() {
                key_grad += g[off + j].abs();
            }
            off += p.key_logits.len();
        }
        assert!(key_grad > 0.0);
    }

    /// One cycle with one single-layer page can only write a function of two
    /// wires into bit 0, so 3-bit parity needs at least two cycles here.
    #[test]
    fn joint_bptt_learns_two_cycle_parity_and_hard_engine_agrees() {
        let batch = parity_task();
        let mut solved = None;
        'seeds: for seed in 0..64u64 {
            let mut e = engine(seed, 4, cfg(2, 1.0), &[2]);
            let mut opt = Adam::new(e.parameter_count(), 0.1);
            let mut g = vec![0.0; e.parameter_count()];
            let mut p = e.parameters();
            for _ in 0..400 {
                e.loss_and_grad(&batch, &mut g).unwrap();
                opt.step(&mut p, &g);
                e.set_parameters(&p);
            }
            let hard = e.discretize().unwrap();
            for ex in &batch {
                let r = e.run_relaxed(ex.input, 0);
                let h = hard
                    .run(ex.input, 0, &mut ExecutionCounters::default())
                    .unwrap();
                let rounded = r
                    .iter()
                    .enumerate()
                    .fold(0u64, |a, (j, &v)| a | (u64::from(v > 0.5) << j));
                if h.workspace & 1 != ex.target || rounded != h.workspace {
                    continue 'seeds;
                }
            }
            solved = Some((seed, e, hard));
            break;
        }
        let (_, e, hard) = solved.expect("some seed learns 2-cycle parity");
        let mut multi_cycle = false;
        for ex in &batch {
            let h = hard
                .run(ex.input, 0, &mut ExecutionCounters::default())
                .unwrap();
            assert_eq!(h.workspace & 1, ex.target);
            multi_cycle |= h.cycles >= 2 && h.route_trace[0] != h.route_trace[1];
            let r = e.run_relaxed(ex.input, 0);
            assert!((r[0] - ex.target as f64).abs() < 0.5);
        }
        assert!(
            multi_cycle,
            "solution must use distinct pages across cycles"
        );
    }

    #[test]
    fn invalid_configurations_fail_closed() {
        let good = engine(1, 2, cfg(2, 1.0), &[2]);
        let pages = good.pages().to_vec();
        assert!(RelaxedHybridEngine::new(cfg(0, 1.0), pages.clone()).is_err());
        let mut t = cfg(2, 1.0);
        t.route_temperature = 0.0;
        assert!(RelaxedHybridEngine::new(t, pages.clone()).is_err());
        let mut dup = pages.clone();
        dup[1].id = dup[0].id;
        assert_eq!(
            RelaxedHybridEngine::new(cfg(2, 1.0), dup),
            Err(RelaxedError::DuplicatePage(PageId(0)))
        );
        let mut bad = pages;
        bad[0].key_logits.pop();
        assert_eq!(
            RelaxedHybridEngine::new(cfg(2, 1.0), bad),
            Err(RelaxedError::PageShape(PageId(0)))
        );
        let mut g = vec![0.0; good.parameter_count()];
        let oob = RelaxedExample {
            input: 8,
            initial_workspace: 0,
            target: 0,
            target_mask: 1,
        };
        assert_eq!(
            good.loss_and_grad(&[oob], &mut g),
            Err(RelaxedError::BadBatch)
        );
    }
}
