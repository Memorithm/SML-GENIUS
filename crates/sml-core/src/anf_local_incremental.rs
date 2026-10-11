//! Incremental evaluation of local search proposals on a [`LocalAnfNetwork`]
//! (A5 step 3).
//!
//! A search proposal changes one state gate (or one readout wiring). The full
//! evaluator re-simulates every gate at every step and recounts every readout.
//! The incremental evaluator instead keeps the cached trajectory of the current
//! network and, for a proposal on gate `g`, re-simulates over time only the
//! gates that can differ from the cache:
//!
//! * at step `t`, gate `g` itself plus every gate that reads a state bit whose
//!   value differs from the cache at step `t - 1` (in at least one lane);
//! * a recomputed gate whose words equal the cached words stops the
//!   propagation, so the dirty set is the *actual* downstream cone of the
//!   change, which often dies out after a few steps.
//!
//! Readout count tables are updated only at the steps where one of their input
//! bits actually changed (old address count `-1`, new address count `+1`), and
//! the loss is updated term by term. Frozen gates (e.g. the history layer) are
//! never re-simulated: they only appear in the cone if they read a changed bit.
//! A readout rewiring proposal recounts that readout from the cached trajectory
//! without any state re-simulation.
//!
//! Losses are integers (errors, or leave-one-out log-loss in fixed-point
//! micro-bits) so incremental and full evaluation are compared bit-exactly.

use crate::anf_local::{LocalAnfError, LocalAnfNetwork};

/// Up to 64 equal-length sequences packed into lane words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaneBlock {
    pub steps: usize,
    /// `steps * input_bits` words; word `t * input_bits + p` is input bit `p` at step `t`.
    pub inputs: Vec<u64>,
    /// `steps * target_bits` words; word `t * target_bits + b` is target bit `b` at step `t`.
    pub targets: Vec<u64>,
    /// Lanes that carry a sequence.
    pub valid: u64,
}

/// A data split as lane blocks with `input_bits` inputs and `target_bits` targets per step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaneData {
    pub input_bits: usize,
    pub target_bits: usize,
    pub blocks: Vec<LaneBlock>,
}

impl LaneData {
    /// Number of (lane, step) predictions.
    pub fn tokens(&self) -> u64 {
        self.blocks
            .iter()
            .map(|b| b.steps as u64 * u64::from(b.valid.count_ones()))
            .sum()
    }
}

/// One local input of a readout table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadoutInput {
    /// Source position (state bit after the step, or a current input bit).
    Source(usize),
    /// Target bit of the same step (teacher-forced; used for bit-level chain rules).
    Target(usize),
}

/// A readout fitted exactly from per-address counts: `inputs[j]` is address bit `j`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadoutSpec {
    pub inputs: Vec<ReadoutInput>,
    pub target: usize,
}

/// Largest readout fan-in (dense count table of `2^k` entries).
pub const MAX_READOUT_FAN_IN: usize = 20;

/// Per-address loss of a readout fitted exactly on the counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LossKind {
    /// `min(c0, c1)`: errors of the majority readout.
    Errors,
    /// Leave-one-out log-loss of the KT (add-1/2) estimate, in micro-bits:
    /// each count is scored by `-log2((c_v - 1/2) / n)`.
    LeaveOneOutBits,
}

/// Fixed-point scale of [`LossKind::LeaveOneOutBits`].
pub const LOSS_SCALE: f64 = 1_000_000.0;

/// Loss contribution of one address with counts `c = [count(0), count(1)]`.
pub fn loss_term(kind: LossKind, c: [u32; 2]) -> i64 {
    match kind {
        LossKind::Errors => i64::from(c[0].min(c[1])),
        LossKind::LeaveOneOutBits => {
            let n = f64::from(c[0] + c[1]);
            let bits: f64 = c
                .iter()
                .filter(|&&v| v > 0)
                .map(|&v| -f64::from(v) * ((f64::from(v) - 0.5) / n).log2())
                .sum();
            (bits * LOSS_SCALE).round() as i64
        }
    }
}

/// Cached bit-sliced trajectory: row `t` of a block holds the source vector the
/// readout sees at step `t`, i.e. `[s_{t+1}, x_t]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trajectory {
    width: usize,
    rows: Vec<Vec<u64>>,
}

impl Trajectory {
    /// Full simulation of every block from the all-zero state.
    pub fn simulate(network: &LocalAnfNetwork, data: &LaneData) -> Result<Self, LocalAnfError> {
        let n = network.state_bits();
        let w = data.input_bits;
        if network.input_bits() != w {
            return Err(LocalAnfError::Width(w));
        }
        let width = n + w;
        let mut source = vec![0u64; width];
        let mut next = vec![0u64; n];
        let mut products = vec![0u64; network.scratch_len()];
        let mut rows = Vec::with_capacity(data.blocks.len());
        for block in &data.blocks {
            source.iter_mut().for_each(|v| *v = 0);
            let mut out = Vec::with_capacity(block.steps * width);
            for t in 0..block.steps {
                source[n..].copy_from_slice(&block.inputs[t * w..(t + 1) * w]);
                network.step_lanes(&mut source, &mut next, &mut products);
                out.extend_from_slice(&source);
            }
            rows.push(out);
        }
        Ok(Self { width, rows })
    }
    pub fn width(&self) -> usize {
        self.width
    }
    /// Source words at step `t` of `block`.
    pub fn row(&self, block: usize, t: usize) -> &[u64] {
        &self.rows[block][t * self.width..(t + 1) * self.width]
    }
}

fn check_spec(spec: &ReadoutSpec, width: usize, target_bits: usize) -> Result<(), LocalAnfError> {
    if spec.inputs.len() > MAX_READOUT_FAN_IN {
        return Err(LocalAnfError::FanIn(spec.inputs.len()));
    }
    if spec.target >= target_bits {
        return Err(LocalAnfError::Input(spec.target));
    }
    for (j, input) in spec.inputs.iter().enumerate() {
        let ok = match *input {
            ReadoutInput::Source(p) => p < width,
            ReadoutInput::Target(b) => b < target_bits && b != spec.target,
        };
        if !ok || spec.inputs[..j].contains(input) {
            return Err(LocalAnfError::Input(j));
        }
    }
    Ok(())
}

fn gather(spec: &ReadoutSpec, row: &[u64], targets: &[u64], out: &mut [u64]) {
    for (slot, input) in out.iter_mut().zip(&spec.inputs) {
        *slot = match *input {
            ReadoutInput::Source(p) => row[p],
            ReadoutInput::Target(b) => targets[b],
        };
    }
}

fn lane_address(words: &[u64], lane: u32) -> usize {
    words
        .iter()
        .enumerate()
        .fold(0usize, |a, (j, &w)| a | ((((w >> lane) & 1) as usize) << j))
}

/// Per-address `[count(target=0), count(target=1)]` of `spec` on a trajectory.
pub fn count_readout(spec: &ReadoutSpec, traj: &Trajectory, data: &LaneData) -> Vec<[u32; 2]> {
    let mut counts = vec![[0u32; 2]; 1 << spec.inputs.len()];
    let mut words = vec![0u64; spec.inputs.len()];
    let tb = data.target_bits;
    for (bi, block) in data.blocks.iter().enumerate() {
        for t in 0..block.steps {
            let targets = &block.targets[t * tb..(t + 1) * tb];
            gather(spec, traj.row(bi, t), targets, &mut words);
            let tw = targets[spec.target];
            let mut lanes = block.valid;
            while lanes != 0 {
                let lane = lanes.trailing_zeros();
                counts[lane_address(&words, lane)][((tw >> lane) & 1) as usize] += 1;
                lanes &= lanes - 1;
            }
        }
    }
    counts
}

fn table_loss(kind: LossKind, counts: &[[u32; 2]]) -> i64 {
    counts.iter().map(|&c| loss_term(kind, c)).sum()
}

/// Reference: full re-simulation plus full recount of every readout.
pub fn full_loss(
    network: &LocalAnfNetwork,
    data: &LaneData,
    specs: &[ReadoutSpec],
    kind: LossKind,
) -> Result<i64, LocalAnfError> {
    let traj = Trajectory::simulate(network, data)?;
    let mut total = 0;
    for spec in specs {
        check_spec(spec, traj.width, data.target_bits)?;
        total += table_loss(kind, &count_readout(spec, &traj, data));
    }
    Ok(total)
}

#[derive(Debug, Clone, Copy)]
struct Change {
    block: u32,
    t: u32,
    pos: u32,
    value: u64,
}

#[derive(Debug, Clone)]
enum Pending {
    None,
    Gate {
        changes: Vec<Change>,
        /// `(readout, address, target value, delta)` count updates to undo.
        updates: Vec<(u32, u32, u8, i8)>,
        loss: i64,
        readout_loss: Vec<i64>,
    },
    Readout {
        index: usize,
        spec: ReadoutSpec,
        counts: Vec<[u32; 2]>,
        readout_loss: i64,
        loss: i64,
    },
}

/// Cached trajectory + readout count tables of the current network; evaluates
/// one proposal at a time with `try_*`, then [`Self::commit`] or [`Self::discard`].
#[derive(Debug, Clone)]
pub struct IncrementalEvaluator {
    kind: LossKind,
    state_bits: usize,
    specs: Vec<ReadoutSpec>,
    traj: Trajectory,
    counts: Vec<Vec<[u32; 2]>>,
    readout_loss: Vec<i64>,
    loss: i64,
    readers: Vec<Vec<usize>>,
    state_readouts: Vec<Vec<usize>>,
    prev_val: Vec<u64>,
    cur_val: Vec<u64>,
    prev_mark: Vec<u32>,
    cur_mark: Vec<u32>,
    eval_mark: Vec<u32>,
    readout_mark: Vec<u32>,
    stamp: u32,
    products: Vec<u64>,
    pending: Pending,
    /// Gate evaluations performed by `try_gate` (work counter for benchmarks).
    pub gate_evaluations: u64,
}

impl IncrementalEvaluator {
    pub fn new(
        network: &LocalAnfNetwork,
        data: &LaneData,
        specs: Vec<ReadoutSpec>,
        kind: LossKind,
    ) -> Result<Self, LocalAnfError> {
        let traj = Trajectory::simulate(network, data)?;
        let n = network.state_bits();
        let mut counts = Vec::with_capacity(specs.len());
        let mut readout_loss = Vec::with_capacity(specs.len());
        for spec in &specs {
            check_spec(spec, traj.width, data.target_bits)?;
            let table = count_readout(spec, &traj, data);
            readout_loss.push(table_loss(kind, &table));
            counts.push(table);
        }
        let mut readers = vec![Vec::new(); n];
        for (e, gate) in network.gates().iter().enumerate() {
            for &i in gate.inputs() {
                if i < n {
                    readers[i].push(e);
                }
            }
        }
        let mut evaluator = Self {
            kind,
            state_bits: n,
            loss: readout_loss.iter().sum(),
            specs,
            traj,
            counts,
            readout_loss,
            readers,
            state_readouts: Vec::new(),
            prev_val: vec![0; n],
            cur_val: vec![0; n],
            prev_mark: vec![0; n],
            cur_mark: vec![0; n],
            eval_mark: vec![0; n],
            readout_mark: Vec::new(),
            stamp: 0,
            products: vec![0; network.scratch_len().max(1)],
            pending: Pending::None,
            gate_evaluations: 0,
        };
        evaluator.rebuild_readout_index();
        Ok(evaluator)
    }

    fn rebuild_readout_index(&mut self) {
        self.state_readouts = vec![Vec::new(); self.state_bits];
        for (r, spec) in self.specs.iter().enumerate() {
            for input in &spec.inputs {
                if let ReadoutInput::Source(p) = *input {
                    if p < self.state_bits {
                        self.state_readouts[p].push(r);
                    }
                }
            }
        }
        self.readout_mark = vec![0; self.specs.len()];
    }

    /// Total loss of the committed (or currently tried) network.
    pub fn loss(&self) -> i64 {
        self.loss
    }
    pub fn specs(&self) -> &[ReadoutSpec] {
        &self.specs
    }
    pub fn counts(&self, readout: usize) -> &[[u32; 2]] {
        &self.counts[readout]
    }
    pub fn trajectory(&self) -> &Trajectory {
        &self.traj
    }

    fn next_stamp(&mut self) -> u32 {
        if self.stamp >= u32::MAX - 4 {
            self.stamp = 0;
            for marks in [
                &mut self.prev_mark,
                &mut self.cur_mark,
                &mut self.eval_mark,
                &mut self.readout_mark,
            ] {
                marks.iter_mut().for_each(|m| *m = 0);
            }
        }
        self.stamp += 1;
        self.stamp
    }

    /// Loss after replacing state gate `g` by `network.gates()[g]`; every other
    /// gate of `network` must equal the committed network.
    pub fn try_gate(
        &mut self,
        network: &LocalAnfNetwork,
        data: &LaneData,
        g: usize,
    ) -> Result<i64, LocalAnfError> {
        self.discard();
        let n = self.state_bits;
        if g >= n || network.state_bits() != n {
            return Err(LocalAnfError::Input(g));
        }
        let w = data.input_bits;
        let tb = data.target_bits;
        let old_loss = self.loss;
        let old_readout_loss = self.readout_loss.clone();
        let mut changes = Vec::new();
        let mut updates = Vec::new();
        let mut to_eval: Vec<usize> = Vec::new();
        let mut dirty_prev: Vec<usize> = Vec::new();
        let mut dirty_cur: Vec<usize> = Vec::new();
        let mut gathered = [0u64; crate::anf_local::LOCAL_ANF_MAX_FAN_IN];
        let mut old_words = [0u64; MAX_READOUT_FAN_IN];
        let mut new_words = [0u64; MAX_READOUT_FAN_IN];
        for (bi, block) in data.blocks.iter().enumerate() {
            dirty_prev.clear();
            // Invalidate the "previous step" marks of the last block.
            let mut prev_stamp = self.next_stamp();
            for t in 0..block.steps {
                let cur_stamp = self.next_stamp();
                to_eval.clear();
                to_eval.push(g);
                self.eval_mark[g] = cur_stamp;
                for &d in &dirty_prev {
                    for &e in &self.readers[d] {
                        if self.eval_mark[e] != cur_stamp {
                            self.eval_mark[e] = cur_stamp;
                            to_eval.push(e);
                        }
                    }
                }
                dirty_cur.clear();
                let inputs = &block.inputs[t * w..(t + 1) * w];
                for &e in &to_eval {
                    let gate = &network.gates()[e];
                    for (slot, &i) in gathered.iter_mut().zip(gate.inputs()) {
                        *slot = if i >= n {
                            inputs[i - n]
                        } else if self.prev_mark[i] == prev_stamp {
                            self.prev_val[i]
                        } else if t == 0 {
                            0
                        } else {
                            self.traj.row(bi, t - 1)[i]
                        };
                    }
                    self.gate_evaluations += 1;
                    let value = gate.evaluate_gathered(&gathered, &mut self.products);
                    if value != self.traj.row(bi, t)[e] {
                        self.cur_mark[e] = cur_stamp;
                        self.cur_val[e] = value;
                        dirty_cur.push(e);
                        changes.push(Change {
                            block: bi as u32,
                            t: t as u32,
                            pos: e as u32,
                            value,
                        });
                    }
                }
                // Readout count updates at this step.
                if !dirty_cur.is_empty() {
                    let targets = &block.targets[t * tb..(t + 1) * tb];
                    let row = self.traj.row(bi, t);
                    for &d in &dirty_cur {
                        for &r in &self.state_readouts[d] {
                            if self.readout_mark[r] == cur_stamp {
                                continue;
                            }
                            self.readout_mark[r] = cur_stamp;
                            let spec = &self.specs[r];
                            let k = spec.inputs.len();
                            gather(spec, row, targets, &mut old_words[..k]);
                            let mut moved = 0u64;
                            for (j, input) in spec.inputs.iter().enumerate() {
                                new_words[j] = old_words[j];
                                if let ReadoutInput::Source(p) = *input {
                                    if p < n && self.cur_mark[p] == cur_stamp {
                                        new_words[j] = self.cur_val[p];
                                        moved |= old_words[j] ^ new_words[j];
                                    }
                                }
                            }
                            moved &= block.valid;
                            let tw = targets[spec.target];
                            let table = &mut self.counts[r];
                            while moved != 0 {
                                let lane = moved.trailing_zeros();
                                let v = ((tw >> lane) & 1) as usize;
                                let old = lane_address(&old_words[..k], lane);
                                let new = lane_address(&new_words[..k], lane);
                                for (address, delta) in [(old, -1i8), (new, 1i8)] {
                                    let before = loss_term(self.kind, table[address]);
                                    if delta < 0 {
                                        table[address][v] -= 1;
                                    } else {
                                        table[address][v] += 1;
                                    }
                                    let after = loss_term(self.kind, table[address]);
                                    self.readout_loss[r] += after - before;
                                    self.loss += after - before;
                                    updates.push((r as u32, address as u32, v as u8, delta));
                                }
                                moved &= moved - 1;
                            }
                        }
                    }
                }
                core::mem::swap(&mut self.prev_val, &mut self.cur_val);
                core::mem::swap(&mut self.prev_mark, &mut self.cur_mark);
                core::mem::swap(&mut dirty_prev, &mut dirty_cur);
                prev_stamp = cur_stamp;
            }
        }
        self.pending = Pending::Gate {
            changes,
            updates,
            loss: old_loss,
            readout_loss: old_readout_loss,
        };
        Ok(self.loss)
    }

    /// Loss after replacing readout `index` by `spec` (state dynamics unchanged).
    pub fn try_readout(
        &mut self,
        data: &LaneData,
        index: usize,
        spec: ReadoutSpec,
    ) -> Result<i64, LocalAnfError> {
        self.discard();
        if index >= self.specs.len() {
            return Err(LocalAnfError::Input(index));
        }
        check_spec(&spec, self.traj.width, data.target_bits)?;
        let table = count_readout(&spec, &self.traj, data);
        let new_loss = table_loss(self.kind, &table);
        let old_spec = core::mem::replace(&mut self.specs[index], spec);
        let old_counts = core::mem::replace(&mut self.counts[index], table);
        let old_readout_loss = core::mem::replace(&mut self.readout_loss[index], new_loss);
        let old_loss = self.loss;
        self.loss += new_loss - old_readout_loss;
        self.rebuild_readout_index();
        self.pending = Pending::Readout {
            index,
            spec: old_spec,
            counts: old_counts,
            readout_loss: old_readout_loss,
            loss: old_loss,
        };
        Ok(self.loss)
    }

    /// Keeps the tried proposal: patches the cached trajectory.
    pub fn commit(&mut self) {
        if let Pending::Gate { changes, .. } = core::mem::replace(&mut self.pending, Pending::None)
        {
            let width = self.traj.width;
            for c in changes {
                self.traj.rows[c.block as usize][c.t as usize * width + c.pos as usize] = c.value;
            }
        }
    }

    /// Reverts the tried proposal (no-op when nothing is pending).
    pub fn discard(&mut self) {
        match core::mem::replace(&mut self.pending, Pending::None) {
            Pending::None => {}
            Pending::Gate {
                updates,
                loss,
                readout_loss,
                ..
            } => {
                for &(r, address, v, delta) in updates.iter().rev() {
                    let cell = &mut self.counts[r as usize][address as usize][usize::from(v)];
                    if delta < 0 {
                        *cell += 1;
                    } else {
                        *cell -= 1;
                    }
                }
                self.loss = loss;
                self.readout_loss = readout_loss;
            }
            Pending::Readout {
                index,
                spec,
                counts,
                readout_loss,
                loss,
            } => {
                self.specs[index] = spec;
                self.counts[index] = counts;
                self.readout_loss[index] = readout_loss;
                self.loss = loss;
                self.rebuild_readout_index();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anf_local::{LocalAnfGate, LocalAnfShape, LocalAnfWiring};
    use crate::toy_tasks::SplitMix64;

    fn random_data(w: usize, tb: usize, blocks: usize, steps: usize, seed: u64) -> LaneData {
        let mut rng = SplitMix64::new(seed);
        LaneData {
            input_bits: w,
            target_bits: tb,
            blocks: (0..blocks)
                .map(|b| LaneBlock {
                    steps,
                    inputs: (0..steps * w).map(|_| rng.next_u64()).collect(),
                    targets: (0..steps * tb).map(|_| rng.next_u64()).collect(),
                    valid: if b == 0 {
                        u64::MAX
                    } else {
                        0x0000_ffff_00ff_f0f0
                    },
                })
                .collect(),
        }
    }

    /// Random network with `w` input bits (gates read inputs and state bits).
    fn random_network(n: usize, w: usize, k: usize, seed: u64) -> LocalAnfNetwork {
        let base = LocalAnfNetwork::random(
            LocalAnfShape {
                state_bits: n,
                fan_in: k,
                readout_fan_in: 2,
                wiring: LocalAnfWiring::Random,
            },
            seed,
        )
        .unwrap();
        let mut rng = SplitMix64::new(seed ^ 77);
        let gates = base
            .gates()
            .iter()
            .map(|gate| {
                // Remap the single input position `n` to a random input bit.
                let inputs: Vec<usize> = gate
                    .inputs()
                    .iter()
                    .map(|&i| if i == n { n + rng.below(w) } else { i })
                    .collect();
                let table: Vec<bool> = gate.truth_table();
                LocalAnfGate::from_truth_table(inputs, &table, n + w).unwrap()
            })
            .collect();
        let readout = LocalAnfGate::zero(vec![n], n + w).unwrap();
        LocalAnfNetwork::with_input_bits(n, w, gates, readout).unwrap()
    }

    fn random_spec(rng: &mut SplitMix64, width: usize, tb: usize, k: usize) -> ReadoutSpec {
        let target = rng.below(tb);
        let mut inputs = Vec::new();
        if tb > 1 && target > 0 {
            inputs.push(ReadoutInput::Target(rng.below(target)));
        }
        while inputs.len() < k {
            let candidate = ReadoutInput::Source(rng.below(width));
            if !inputs.contains(&candidate) {
                inputs.push(candidate);
            }
        }
        ReadoutSpec { inputs, target }
    }

    fn check_equivalence(w: usize, tb: usize, kind: LossKind, seed: u64) {
        let (n, k) = (24, 4);
        let data = random_data(w, tb, 2, 40, seed);
        let mut network = random_network(n, w, k, seed);
        let mut rng = SplitMix64::new(seed ^ 0xabc);
        let specs: Vec<ReadoutSpec> = (0..3)
            .map(|_| random_spec(&mut rng, n + w, tb, 5))
            .collect();
        let mut evaluator = IncrementalEvaluator::new(&network, &data, specs, kind).unwrap();
        assert_eq!(
            evaluator.loss(),
            full_loss(&network, &data, evaluator.specs(), kind).unwrap()
        );
        for step in 0..300 {
            if rng.below(5) == 0 {
                let index = rng.below(3);
                let fan_in = 1 + rng.below(6);
                let spec = random_spec(&mut rng, n + w, tb, fan_in);
                let mut specs = evaluator.specs().to_vec();
                specs[index] = spec.clone();
                let loss = evaluator.try_readout(&data, index, spec).unwrap();
                assert_eq!(loss, full_loss(&network, &data, &specs, kind).unwrap());
                if rng.next_bool() {
                    evaluator.commit();
                } else {
                    evaluator.discard();
                }
            } else {
                let g = rng.below(n);
                let before = network.gates()[g].clone();
                let gate = &mut network.gates_mut()[g];
                for _ in 0..1 + rng.below(3) {
                    gate.flip(rng.below(gate.monomial_count()));
                }
                let loss = evaluator.try_gate(&network, &data, g).unwrap();
                assert_eq!(
                    loss,
                    full_loss(&network, &data, evaluator.specs(), kind).unwrap(),
                    "step {step}"
                );
                if rng.next_bool() {
                    evaluator.commit();
                } else {
                    evaluator.discard();
                    network.gates_mut()[g] = before;
                }
            }
            // The committed cache is bit-exact with a full re-simulation.
            if step % 25 == 0 {
                assert_eq!(
                    evaluator.trajectory(),
                    &Trajectory::simulate(&network, &data).unwrap()
                );
                let traj = Trajectory::simulate(&network, &data).unwrap();
                for (r, spec) in evaluator.specs().iter().enumerate() {
                    assert_eq!(
                        evaluator.counts(r),
                        count_readout(spec, &traj, &data).as_slice()
                    );
                }
                assert_eq!(
                    evaluator.loss(),
                    full_loss(&network, &data, evaluator.specs(), kind).unwrap()
                );
            }
        }
    }

    #[test]
    fn incremental_matches_full_resimulation_single_input_errors() {
        check_equivalence(1, 1, LossKind::Errors, 1);
    }

    #[test]
    fn incremental_matches_full_resimulation_multi_bit_log_loss() {
        check_equivalence(8, 8, LossKind::LeaveOneOutBits, 2);
        check_equivalence(3, 4, LossKind::Errors, 3);
    }

    #[test]
    fn unchanged_gate_output_costs_one_gate_per_step() {
        let data = random_data(1, 1, 1, 50, 4);
        let network = random_network(32, 1, 4, 4);
        let spec = ReadoutSpec {
            inputs: vec![ReadoutInput::Source(0), ReadoutInput::Source(32)],
            target: 0,
        };
        let mut evaluator =
            IncrementalEvaluator::new(&network, &data, vec![spec], LossKind::Errors).unwrap();
        let before = evaluator.loss();
        // Same network: nothing differs, so only gate 5 is evaluated at each step.
        assert_eq!(evaluator.try_gate(&network, &data, 5).unwrap(), before);
        assert_eq!(evaluator.gate_evaluations, 50);
        evaluator.discard();
        assert_eq!(evaluator.loss(), before);
    }

    #[test]
    fn leave_one_out_term_matches_definition() {
        assert_eq!(loss_term(LossKind::LeaveOneOutBits, [0, 0]), 0);
        // A singleton is predicted with p = 1/2 when left out: one bit.
        assert_eq!(loss_term(LossKind::LeaveOneOutBits, [1, 0]), 1_000_000);
        // [3, 1]: 3 * -log2(2.5/4) + 1 * -log2(0.5/4).
        let expected = 3.0 * -(2.5f64 / 4.0).log2() + -(0.5f64 / 4.0).log2();
        assert_eq!(
            loss_term(LossKind::LeaveOneOutBits, [3, 1]),
            (expected * LOSS_SCALE).round() as i64
        );
        assert_eq!(loss_term(LossKind::Errors, [3, 1]), 1);
    }
}
