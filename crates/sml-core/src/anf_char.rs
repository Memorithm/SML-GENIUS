//! A5 step 3: first character-level task, next-byte prediction with a local
//! fan-in ANF recurrent network.
//!
//! How bytes are predicted:
//!
//! * **Input.** The current byte `x_t` enters as 8 input bits (bit-planes) at
//!   source positions `N..N + 8`.
//! * **State.** `N = F + 8L` state bits:
//!   * free gates `0..F` are searched on the task loss;
//!   * history gates `F..N` are a byte-level history layer. Gate `(j, p)` gets
//!     the auxiliary target "bit `p` of `x_{t+1-j}`" (lagged bit-planes,
//!     `j = 1..L`). It is fitted greedily and exactly, as in step 2, from the
//!     input stream only.
//! * **Output.** 8 chained readouts give a bit-level chain rule over the next
//!   byte `x_{t+1}`, most significant bit first. Readout `b` predicts bit `b`
//!   from the already-known bits `0..b` of the same byte (teacher-forced at
//!   training) plus a few context source bits. Its truth table is fitted
//!   exactly from per-address counts.
//! * **Probability.** `P(x_{t+1}) = prod_b P(bit_b | address_b)`, with
//!   `P(bit = v | a) = (c_v(a) + 1/2) / (n(a) + 1)` (KT estimate from train
//!   counts). This is a proper distribution over the 256 bytes.
//! * **Prediction.** The predicted byte is the exact argmax over all 256 bytes.
//!   The ANF form of readout `b` is its majority truth table, Möbius-converted.
//!   The counts are an extra calibration table (2 x u32 per address), reported
//!   separately.
//! * **Context selection.** The context bits of each readout are chosen by
//!   greedy forward selection on the train leave-one-out log-loss.
//! * **Search.** The free gates and the readout wiring are then improved by the
//!   step-1 local search, with incremental evaluation
//!   ([`crate::anf_local_incremental`]) and validation early stopping on
//!   bits per byte.

use crate::anf_local::{LocalAnfError, LocalAnfGate, LocalAnfNetwork, LOCAL_ANF_MAX_FAN_IN};
use crate::anf_local_incremental::{
    full_loss, loss_term, IncrementalEvaluator, LaneBlock, LaneData, LossKind, ReadoutInput,
    ReadoutSpec, Trajectory,
};
use crate::toy_tasks::SplitMix64;

pub const BYTE_BITS: usize = 8;
pub const LANES: usize = 64;

/// Bit `b` of a byte in chain-rule order (`b = 0` is the most significant bit).
pub fn chain_bit(byte: u8, b: usize) -> u64 {
    u64::from((byte >> (7 - b)) & 1)
}

/// Token layout shared by all models: the stream is cut into 64 contiguous
/// chunks of `chunk` bytes. In lane `l`, the byte at position
/// `l * chunk + t + 1` is predicted from the chunk prefix ending at
/// `l * chunk + t`, for `t = 0..chunk - 1`. The context resets at chunk
/// starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkLayout {
    pub chunk: usize,
}

impl ChunkLayout {
    pub fn new(len: usize) -> Result<Self, LocalAnfError> {
        let chunk = len / LANES;
        if chunk < 2 {
            return Err(LocalAnfError::Width(len));
        }
        Ok(Self { chunk })
    }
    pub fn tokens(&self) -> u64 {
        (LANES * (self.chunk - 1)) as u64
    }
}

/// Packs bytes as one 64-lane block (8 input bits, 8 chain-rule target bits).
pub fn pack_bytes(bytes: &[u8]) -> Result<LaneData, LocalAnfError> {
    let layout = ChunkLayout::new(bytes.len())?;
    let steps = layout.chunk - 1;
    let mut inputs = vec![0u64; steps * BYTE_BITS];
    let mut targets = vec![0u64; steps * BYTE_BITS];
    for lane in 0..LANES {
        let base = lane * layout.chunk;
        for t in 0..steps {
            let x = bytes[base + t];
            let y = bytes[base + t + 1];
            for p in 0..BYTE_BITS {
                inputs[t * BYTE_BITS + p] |= u64::from((x >> p) & 1) << lane;
                targets[t * BYTE_BITS + p] |= chain_bit(y, p) << lane;
            }
        }
    }
    Ok(LaneData {
        input_bits: BYTE_BITS,
        target_bits: BYTE_BITS,
        blocks: vec![LaneBlock {
            steps,
            inputs,
            targets,
            valid: u64::MAX,
        }],
    })
}

/// Contiguous train/val/test split (80/10/10 %).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharSplits {
    pub train: Vec<u8>,
    pub val: Vec<u8>,
    pub test: Vec<u8>,
}

impl CharSplits {
    pub fn new(bytes: &[u8]) -> Self {
        let a = bytes.len() * 8 / 10;
        let b = bytes.len() * 9 / 10;
        Self {
            train: bytes[..a].to_vec(),
            val: bytes[a..b].to_vec(),
            test: bytes[b..].to_vec(),
        }
    }
}

/// Model and search configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharAnfConfig {
    /// History lags `L` (8 gates each).
    pub lags: usize,
    /// Free gates `F` searched on the task loss.
    pub free_gates: usize,
    /// Fan-in of the free gates.
    pub fan_in: usize,
    /// Maximum context source bits per readout.
    pub context_bits: usize,
    /// Maximum total readout fan-in (context bits plus chained target bits).
    pub max_readout_fan_in: usize,
    pub proposals_per_epoch: u32,
    pub max_epochs: u32,
    pub patience: u32,
    pub seed: u64,
    /// Incremental proposal evaluation (identical results, faster).
    pub incremental: bool,
}

impl Default for CharAnfConfig {
    fn default() -> Self {
        Self {
            lags: 4,
            free_gates: 64,
            fan_in: 6,
            context_bits: 12,
            max_readout_fan_in: 16,
            proposals_per_epoch: 64,
            max_epochs: 20,
            patience: 4,
            seed: 0,
            incremental: true,
        }
    }
}

/// A trained byte model: network plus 8 chained readouts with train counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharAnfModel {
    pub network: LocalAnfNetwork,
    pub readouts: Vec<ReadoutSpec>,
    pub counts: Vec<Vec<[u32; 2]>>,
}

impl CharAnfModel {
    /// Coefficient bits of the state gates plus the readout truth tables (`2^k` each).
    pub fn learned_bits(&self) -> usize {
        self.network
            .gates()
            .iter()
            .map(LocalAnfGate::monomial_count)
            .sum::<usize>()
            + self
                .readouts
                .iter()
                .map(|r| 1usize << r.inputs.len())
                .sum::<usize>()
    }
    /// Readout addresses with at least one train count (calibration table entries).
    pub fn calibration_entries(&self) -> usize {
        self.counts
            .iter()
            .map(|t| t.iter().filter(|c| c[0] + c[1] > 0).count())
            .sum()
    }
    /// Source/target bits read per predicted byte (gate fan-ins + readout fan-ins).
    pub fn bits_read_per_token(&self) -> usize {
        self.network
            .gates()
            .iter()
            .map(LocalAnfGate::fan_in)
            .sum::<usize>()
            + self.readouts.iter().map(|r| r.inputs.len()).sum::<usize>()
    }
}

/// History-layer gate position of lag `j` (1-based) and bit-plane `p`.
pub fn history_position(config: &CharAnfConfig, j: usize, p: usize) -> usize {
    config.free_gates + (j - 1) * BYTE_BITS + p
}

/// Seeded initial network: free gates read their own bit, one random input bit
/// and random other source bits; the identity monomial plus about two random
/// monomials. History gates are placeholders until [`fit_history`].
pub fn initial_network(config: &CharAnfConfig) -> Result<LocalAnfNetwork, LocalAnfError> {
    let f = config.free_gates;
    let n = f + BYTE_BITS * config.lags;
    let width = n + BYTE_BITS;
    let k = config.fan_in;
    if n == 0 || !(2..=LOCAL_ANF_MAX_FAN_IN).contains(&k) || k > width {
        return Err(LocalAnfError::FanIn(k));
    }
    let mut rng = SplitMix64::new(config.seed ^ 0xc4a7_0003);
    let mut gates = Vec::with_capacity(n);
    for g in 0..f {
        let mut inputs = vec![g, n + rng.below(BYTE_BITS)];
        while inputs.len() < k {
            let candidate = rng.below(width);
            if !inputs.contains(&candidate) {
                inputs.push(candidate);
            }
        }
        let mut gate = LocalAnfGate::zero(inputs, width)?;
        gate.flip(0b1);
        for m in 0..gate.monomial_count() {
            if rng.below(gate.monomial_count()) < 2 {
                gate.flip(m);
            }
        }
        gates.push(gate);
    }
    for j in 1..=config.lags {
        for p in 0..BYTE_BITS {
            let _ = (j, p);
            gates.push(LocalAnfGate::zero(vec![n + p], width)?);
        }
    }
    let readout = LocalAnfGate::zero(vec![n], width)?;
    LocalAnfNetwork::with_input_bits(n, BYTE_BITS, gates, readout)
}

/// Bit-sliced per-address counts for a small address (`words.len() <= 8`).
fn count_small(words: &[u64], target: u64, valid: u64, counts: &mut [[u32; 2]]) {
    for (address, slot) in counts.iter_mut().enumerate() {
        let mut mask = valid;
        for (j, &w) in words.iter().enumerate() {
            mask &= if (address >> j) & 1 == 1 { w } else { !w };
        }
        slot[1] += (mask & target).count_ones();
        slot[0] += (mask & !target).count_ones();
    }
}

/// Greedy exact fit of the history layer (lagged bit-plane targets), in order
/// `j = 1..L`, `p = 0..8`. Sources are selected by forward selection among the
/// input bits and the already fitted history gates. Returns total train errors.
pub fn fit_history(
    network: &mut LocalAnfNetwork,
    config: &CharAnfConfig,
    train: &LaneData,
) -> Result<u64, LocalAnfError> {
    let n = network.state_bits();
    let width = n + BYTE_BITS;
    // columns[q][block][t] = word of source position q in row t (history gates only).
    let mut columns: Vec<Option<Vec<Vec<u64>>>> = vec![None; n];
    let mut total_errors = 0;
    let max_fan_in = config.fan_in.min(LOCAL_ANF_MAX_FAN_IN);
    for j in 1..=config.lags {
        for p in 0..BYTE_BITS {
            let e = history_position(config, j, p);
            let mut candidates: Vec<usize> = (n..width).collect();
            candidates.extend((config.free_gates..n).filter(|&q| columns[q].is_some()));
            // Value of candidate `c` as seen by a gate at row `t`.
            let source = |c: usize, bi: usize, t: usize| -> u64 {
                let block = &train.blocks[bi];
                if c >= n {
                    block.inputs[t * BYTE_BITS + (c - n)]
                } else if t == 0 {
                    0
                } else {
                    columns[c].as_ref().map_or(0, |col| col[bi][t - 1])
                }
            };
            let target = |bi: usize, t: usize| -> u64 {
                if t + 1 >= j {
                    train.blocks[bi].inputs[(t + 1 - j) * BYTE_BITS + p]
                } else {
                    0
                }
            };
            let fit = |selected: &[usize]| -> (u64, Vec<[u32; 2]>) {
                let mut counts = vec![[0u32; 2]; 1 << selected.len()];
                let mut words = vec![0u64; selected.len()];
                for (bi, block) in train.blocks.iter().enumerate() {
                    for t in 0..block.steps {
                        for (slot, &c) in words.iter_mut().zip(selected) {
                            *slot = source(c, bi, t);
                        }
                        count_small(&words, target(bi, t), block.valid, &mut counts);
                    }
                }
                let errors = counts.iter().map(|c| u64::from(c[0].min(c[1]))).sum();
                (errors, counts)
            };
            let mut selected: Vec<usize> = Vec::new();
            let (mut best, mut best_counts) = fit(&selected);
            while selected.len() < max_fan_in && best > 0 {
                let mut round: Option<(u64, usize, Vec<[u32; 2]>)> = None;
                for &c in candidates.iter().filter(|c| !selected.contains(c)) {
                    let mut trial = selected.clone();
                    trial.push(c);
                    let (errors, counts) = fit(&trial);
                    if round.as_ref().is_none_or(|r| errors < r.0) {
                        round = Some((errors, c, counts));
                    }
                }
                match round {
                    Some((errors, c, counts)) if errors < best => {
                        selected.push(c);
                        best = errors;
                        best_counts = counts;
                    }
                    _ => break,
                }
            }
            if selected.is_empty() {
                // Constant target: read one input bit with a constant table.
                selected.push(n + p);
                let c = best_counts[0];
                best_counts = vec![c, c];
            }
            let table: Vec<bool> = best_counts.iter().map(|c| c[1] > c[0]).collect();
            let gate = LocalAnfGate::from_truth_table(selected.clone(), &table, width)?;
            // Column of the new gate.
            let mut column = Vec::with_capacity(train.blocks.len());
            let mut products = vec![0u64; gate.monomial_count()];
            let mut words = vec![0u64; selected.len()];
            for (bi, block) in train.blocks.iter().enumerate() {
                let mut col = Vec::with_capacity(block.steps);
                for t in 0..block.steps {
                    for (slot, &c) in words.iter_mut().zip(&selected) {
                        *slot = source(c, bi, t);
                    }
                    col.push(gate.evaluate_gathered(&words, &mut products));
                }
                column.push(col);
            }
            network.gates_mut()[e] = gate;
            columns[e] = Some(column);
            total_errors += best;
        }
    }
    Ok(total_errors)
}

/// Per-token view of the training trajectory for greedy readout selection.
struct TokenTable {
    steps: Vec<(usize, usize)>,
}

impl TokenTable {
    fn new(data: &LaneData) -> Self {
        let steps = data
            .blocks
            .iter()
            .enumerate()
            .flat_map(|(bi, b)| (0..b.steps).map(move |t| (bi, t)))
            .collect();
        Self { steps }
    }
}

/// Greedy forward selection of the context bits of the 8 chained readouts on
/// the train leave-one-out log-loss (candidates: every source position).
pub fn select_readouts(
    network: &LocalAnfNetwork,
    config: &CharAnfConfig,
    train: &LaneData,
) -> Result<Vec<ReadoutSpec>, LocalAnfError> {
    let traj = Trajectory::simulate(network, train)?;
    let tokens = TokenTable::new(train);
    let width = traj.width();
    let lanes_of = |bi: usize| train.blocks[bi].valid;
    let mut specs = Vec::with_capacity(BYTE_BITS);
    for b in 0..BYTE_BITS {
        let mut inputs: Vec<ReadoutInput> = (0..b).map(ReadoutInput::Target).collect();
        // Current address per (step, lane) and target bit per (step, lane).
        let mut address: Vec<u32> = Vec::with_capacity(tokens.steps.len() * LANES);
        let mut target: Vec<u8> = Vec::with_capacity(tokens.steps.len() * LANES);
        for &(bi, t) in &tokens.steps {
            let block = &train.blocks[bi];
            let tw = &block.targets[t * BYTE_BITS..(t + 1) * BYTE_BITS];
            for lane in 0..LANES {
                let a = (0..b).fold(0u32, |a, i| a | ((((tw[i] >> lane) & 1) as u32) << i));
                address.push(a);
                target.push(((tw[b] >> lane) & 1) as u8);
            }
        }
        let score = |address: &[u32], extra: Option<usize>, k: usize| -> i64 {
            let mut counts = vec![[0u32; 2]; 1 << (k + usize::from(extra.is_some()))];
            for (s, &(bi, t)) in tokens.steps.iter().enumerate() {
                let word = extra.map_or(0, |c| traj.row(bi, t)[c]);
                let valid = lanes_of(bi);
                for lane in 0..LANES {
                    if (valid >> lane) & 1 == 0 {
                        continue;
                    }
                    let i = s * LANES + lane;
                    let a = address[i] as usize | ((((word >> lane) & 1) as usize) << k);
                    counts[a][usize::from(target[i])] += 1;
                }
            }
            counts
                .iter()
                .map(|&c| loss_term(LossKind::LeaveOneOutBits, c))
                .sum()
        };
        let mut best = score(&address, None, inputs.len());
        let mut context = 0;
        while context < config.context_bits && inputs.len() < config.max_readout_fan_in {
            let k = inputs.len();
            let mut round: Option<(i64, usize)> = None;
            for c in 0..width {
                if inputs.contains(&ReadoutInput::Source(c)) {
                    continue;
                }
                let loss = score(&address, Some(c), k);
                if round.is_none_or(|r| loss < r.0) {
                    round = Some((loss, c));
                }
            }
            match round {
                Some((loss, c)) if loss < best => {
                    best = loss;
                    inputs.push(ReadoutInput::Source(c));
                    context += 1;
                    for (s, &(bi, t)) in tokens.steps.iter().enumerate() {
                        let word = traj.row(bi, t)[c];
                        for lane in 0..LANES {
                            address[s * LANES + lane] |= (((word >> lane) & 1) as u32) << k;
                        }
                    }
                }
                _ => break,
            }
        }
        specs.push(ReadoutSpec { inputs, target: b });
    }
    Ok(specs)
}

/// Evaluation metrics on a split.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharMetrics {
    pub tokens: u64,
    /// Mean `-log2 P(next byte)`.
    pub bits_per_byte: f64,
    /// Fraction of tokens whose argmax byte is correct (`NaN` when not computed).
    pub accuracy: f64,
}

fn kt(c: [u32; 2], v: usize) -> f64 {
    (f64::from(c[v]) + 0.5) / (f64::from(c[0] + c[1]) + 1.0)
}

/// Scores `data` with the model: exact chain-rule probabilities, and the exact
/// argmax over the 256 bytes when `argmax` is set.
pub fn evaluate(
    model: &CharAnfModel,
    data: &LaneData,
    argmax: bool,
) -> Result<CharMetrics, LocalAnfError> {
    let traj = Trajectory::simulate(&model.network, data)?;
    let mut bits = 0.0f64;
    let mut correct = 0u64;
    let mut tokens = 0u64;
    let mut context = [0usize; BYTE_BITS];
    let mut logp = vec![0.0f64; 512];
    for (bi, block) in data.blocks.iter().enumerate() {
        for t in 0..block.steps {
            let row = traj.row(bi, t);
            let tw = &block.targets[t * BYTE_BITS..(t + 1) * BYTE_BITS];
            let mut lanes = block.valid;
            while lanes != 0 {
                let lane = lanes.trailing_zeros();
                lanes &= lanes - 1;
                // Context part of every readout address; target slots are filled per byte.
                for (b, spec) in model.readouts.iter().enumerate() {
                    context[b] = spec
                        .inputs
                        .iter()
                        .enumerate()
                        .fold(0usize, |a, (j, input)| match *input {
                            ReadoutInput::Source(p) => a | ((((row[p] >> lane) & 1) as usize) << j),
                            ReadoutInput::Target(_) => a,
                        });
                }
                let address = |b: usize, prefix: &dyn Fn(usize) -> usize| -> usize {
                    model.readouts[b]
                        .inputs
                        .iter()
                        .enumerate()
                        .fold(context[b], |a, (j, input)| match *input {
                            ReadoutInput::Target(i) => a | (prefix(i) << j),
                            ReadoutInput::Source(_) => a,
                        })
                };
                let actual = |i: usize| ((tw[i] >> lane) & 1) as usize;
                let mut lp = 0.0;
                for b in 0..BYTE_BITS {
                    let c = model.counts[b][address(b, &actual)];
                    lp += kt(c, actual(b)).log2();
                }
                bits -= lp;
                if argmax {
                    // logp over prefix trees: node (b, prefix) at index (1 << b) + prefix.
                    logp[1] = 0.0;
                    for b in 0..BYTE_BITS {
                        for prefix in 0..(1usize << b) {
                            let bit_of = |i: usize| (prefix >> (b - 1 - i)) & 1;
                            let c = model.counts[b][address(b, &bit_of)];
                            let base = logp[(1 << b) + prefix];
                            for v in 0..2 {
                                logp[(2 << b) + (prefix << 1 | v)] = base + kt(c, v).log2();
                            }
                        }
                    }
                    let (best, _) = (0..256usize).fold((0usize, f64::NEG_INFINITY), |acc, y| {
                        let value = logp[256 + y];
                        if value > acc.1 {
                            (y, value)
                        } else {
                            acc
                        }
                    });
                    let truth = (0..BYTE_BITS).fold(0usize, |a, i| a << 1 | actual(i));
                    correct += u64::from(best == truth);
                }
                tokens += 1;
            }
        }
    }
    Ok(CharMetrics {
        tokens,
        bits_per_byte: bits / tokens.max(1) as f64,
        accuracy: if argmax {
            correct as f64 / tokens.max(1) as f64
        } else {
            f64::NAN
        },
    })
}

/// Training report.
#[derive(Debug, Clone, PartialEq)]
pub struct CharTrainReport {
    pub model: CharAnfModel,
    pub history_errors: u64,
    pub val_bits_per_byte: f64,
    pub epochs: u32,
    pub evaluations: u64,
    pub accepted: u64,
    /// Seconds spent in history fit, readout selection and search.
    pub seconds: [f64; 3],
}

fn snapshot(
    network: &LocalAnfNetwork,
    specs: &[ReadoutSpec],
    train: &LaneData,
) -> Result<CharAnfModel, LocalAnfError> {
    let traj = Trajectory::simulate(network, train)?;
    Ok(CharAnfModel {
        network: network.clone(),
        readouts: specs.to_vec(),
        counts: specs
            .iter()
            .map(|s| crate::anf_local_incremental::count_readout(s, &traj, train))
            .collect(),
    })
}

/// Full pipeline: history fit, greedy readout selection, then local search on
/// the free gates and readout wiring (train leave-one-out log-loss, validation
/// bits-per-byte early stopping). `max_epochs = 0` skips the search.
pub fn train(
    config: CharAnfConfig,
    train: &LaneData,
    val: &LaneData,
) -> Result<CharTrainReport, LocalAnfError> {
    let clock = std::time::Instant::now();
    let mut network = initial_network(&config)?;
    let history_errors = fit_history(&mut network, &config, train)?;
    let t_history = clock.elapsed().as_secs_f64();
    let clock = std::time::Instant::now();
    let mut specs = select_readouts(&network, &config, train)?;
    let t_readout = clock.elapsed().as_secs_f64();
    let clock = std::time::Instant::now();
    let kind = LossKind::LeaveOneOutBits;
    let mut evaluator = IncrementalEvaluator::new(&network, train, specs.clone(), kind)?;
    let mut current = evaluator.loss();
    let mut best_model = snapshot(&network, &specs, train)?;
    let mut best_val = evaluate(&best_model, val, false)?.bits_per_byte;
    let mut rng = SplitMix64::new(config.seed ^ 0x5eed_c4a7_0000_0003);
    let (mut epochs, mut evaluations, mut accepted, mut since_best) = (0u32, 0u64, 0u64, 0u32);
    let n = network.state_bits();
    let width = n + BYTE_BITS;
    let f = config.free_gates;
    while epochs < config.max_epochs && since_best < config.patience {
        epochs += 1;
        for _ in 0..config.proposals_per_epoch {
            let roll = if f == 0 { 99 } else { rng.below(100) };
            let loss;
            let mut undo_gate = None;
            if roll >= 85 {
                let r = rng.below(BYTE_BITS);
                let mut spec = specs[r].clone();
                let slots: Vec<usize> = spec
                    .inputs
                    .iter()
                    .enumerate()
                    .filter(|(_, i)| matches!(i, ReadoutInput::Source(_)))
                    .map(|(j, _)| j)
                    .collect();
                if slots.is_empty() {
                    continue;
                }
                let slot = slots[rng.below(slots.len())];
                let candidate = ReadoutInput::Source(rng.below(width));
                if spec.inputs.contains(&candidate) {
                    continue;
                }
                spec.inputs[slot] = candidate;
                loss = if config.incremental {
                    evaluator.try_readout(train, r, spec.clone())?
                } else {
                    let mut all = specs.clone();
                    all[r] = spec.clone();
                    full_loss(&network, train, &all, kind)?
                };
                evaluations += 1;
                if loss <= current {
                    accepted += u64::from(loss < current);
                    current = loss;
                    specs[r] = spec.clone();
                    if config.incremental {
                        evaluator.commit();
                    } else {
                        evaluator.try_readout(train, r, spec)?;
                        evaluator.commit();
                    }
                } else {
                    evaluator.discard();
                }
                continue;
            }
            let g = rng.below(f);
            undo_gate.replace(network.gates()[g].clone());
            mutate_gate(&mut network.gates_mut()[g], roll, &mut rng);
            loss = if config.incremental {
                evaluator.try_gate(&network, train, g)?
            } else {
                full_loss(&network, train, &specs, kind)?
            };
            evaluations += 1;
            if loss <= current {
                accepted += u64::from(loss < current);
                current = loss;
                if !config.incremental {
                    evaluator.try_gate(&network, train, g)?;
                }
                evaluator.commit();
            } else {
                evaluator.discard();
                if let Some(old) = undo_gate {
                    network.gates_mut()[g] = old;
                }
            }
        }
        let model = CharAnfModel {
            network: network.clone(),
            readouts: specs.clone(),
            counts: (0..BYTE_BITS)
                .map(|r| evaluator.counts(r).to_vec())
                .collect(),
        };
        let val_bits = evaluate(&model, val, false)?.bits_per_byte;
        if val_bits < best_val {
            best_val = val_bits;
            best_model = model;
            since_best = 0;
        } else {
            since_best += 1;
        }
    }
    Ok(CharTrainReport {
        model: best_model,
        history_errors,
        val_bits_per_byte: best_val,
        epochs,
        evaluations,
        accepted,
        seconds: [t_history, t_readout, clock.elapsed().as_secs_f64()],
    })
}

/// Step-1 move mix on one gate: one flip (40 %), two flips (20 %), or a random
/// sparse ANF of degree <= 2 (25 %) — `roll` in `0..85`.
fn mutate_gate(gate: &mut LocalAnfGate, roll: usize, rng: &mut SplitMix64) {
    let count = gate.monomial_count();
    let k = gate.fan_in();
    if roll < 60 {
        let first = rng.below(count);
        gate.flip(first);
        if roll >= 40 {
            gate.flip((first + 1 + rng.below(count - 1)) % count);
        }
        return;
    }
    for m in 0..count {
        if gate.coefficient(m) {
            gate.flip(m);
        }
    }
    let terms = 1 + rng.below(2);
    let mut placed = Vec::with_capacity(terms);
    while placed.len() < terms {
        let a = rng.below(k + 1);
        let b = rng.below(k + 1);
        let monomial = [a, b]
            .iter()
            .filter(|&&v| v < k)
            .fold(0usize, |m, &v| m | (1 << v));
        if !placed.contains(&monomial) {
            placed.push(monomial);
            gate.flip(monomial);
        }
    }
}

/// Deterministic French-like synthetic text for tests (no external data).
pub fn synthetic_text(len: usize, seed: u64) -> Vec<u8> {
    const WORDS: [&str; 16] = [
        "le", "la", "chat", "chien", "mange", "voit", "une", "souris", "dans", "jardin", "et",
        "puis", "il", "elle", "court", "dort",
    ];
    let mut rng = SplitMix64::new(seed);
    let mut out = Vec::with_capacity(len + 16);
    while out.len() < len {
        out.extend_from_slice(WORDS[rng.below(WORDS.len())].as_bytes());
        out.push(if rng.below(8) == 0 { b'.' } else { b' ' });
    }
    out.truncate(len);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small_config() -> CharAnfConfig {
        CharAnfConfig {
            lags: 2,
            free_gates: 8,
            fan_in: 4,
            context_bits: 6,
            max_readout_fan_in: 12,
            proposals_per_epoch: 24,
            max_epochs: 3,
            patience: 3,
            seed: 1,
            incremental: true,
        }
    }

    #[test]
    fn packing_uses_chunked_lanes_and_msb_first_targets() {
        let bytes: Vec<u8> = (0..64 * 5).map(|i| (i * 37 % 251) as u8).collect();
        let data = pack_bytes(&bytes).unwrap();
        assert_eq!(data.tokens(), 64 * 4);
        let block = &data.blocks[0];
        for lane in [0usize, 17, 63] {
            for t in 0..4 {
                let x = bytes[lane * 5 + t];
                let y = bytes[lane * 5 + t + 1];
                for p in 0..8 {
                    assert_eq!(
                        (block.inputs[t * 8 + p] >> lane) & 1,
                        u64::from((x >> p) & 1)
                    );
                    assert_eq!((block.targets[t * 8 + p] >> lane) & 1, chain_bit(y, p));
                }
            }
        }
        assert_eq!(chain_bit(0b1000_0000, 0), 1);
        assert!(pack_bytes(&bytes[..100]).is_err());
    }

    #[test]
    fn history_layer_is_an_exact_byte_delay_line() {
        let text = synthetic_text(64 * 60, 3);
        let data = pack_bytes(&text).unwrap();
        let config = small_config();
        let mut network = initial_network(&config).unwrap();
        assert_eq!(fit_history(&mut network, &config, &data).unwrap(), 0);
        let traj = Trajectory::simulate(&network, &data).unwrap();
        for t in 0..data.blocks[0].steps {
            for j in 1..=config.lags {
                for p in 0..8 {
                    let expected = if t + 1 >= j {
                        data.blocks[0].inputs[(t + 1 - j) * 8 + p]
                    } else {
                        0
                    };
                    assert_eq!(traj.row(0, t)[history_position(&config, j, p)], expected);
                }
            }
        }
    }

    #[test]
    fn chained_readouts_define_a_distribution_over_bytes() {
        let text = synthetic_text(64 * 80, 4);
        let data = pack_bytes(&text).unwrap();
        let config = CharAnfConfig {
            max_epochs: 0,
            ..small_config()
        };
        let report = train(config, &data, &data).unwrap();
        let model = &report.model;
        // Sum over the 256 bytes of the chain-rule probability at one context.
        let counts = &model.counts;
        let mut total = 0.0;
        for y in 0..256usize {
            let mut p = 1.0;
            for (b, spec) in model.readouts.iter().enumerate() {
                let address = spec
                    .inputs
                    .iter()
                    .enumerate()
                    .fold(0, |a, (j, i)| match *i {
                        ReadoutInput::Target(t) => a | ((y >> (7 - t)) & 1) << j,
                        ReadoutInput::Source(_) => a,
                    });
                p *= kt(counts[b][address], (y >> (7 - b)) & 1);
            }
            total += p;
        }
        assert!((total - 1.0).abs() < 1e-9);
        let metrics = evaluate(model, &data, true).unwrap();
        // Synthetic text with a 16-word vocabulary is far below 8 bits per byte.
        assert!(metrics.bits_per_byte < 3.0, "{metrics:?}");
        assert!(metrics.accuracy > 0.3, "{metrics:?}");
    }

    #[test]
    fn incremental_char_search_matches_full_resimulation() {
        let text = synthetic_text(64 * 40, 5);
        let (train_bytes, val_bytes) = text.split_at(64 * 30);
        let train_data = pack_bytes(train_bytes).unwrap();
        let val_data = pack_bytes(val_bytes).unwrap();
        let config = small_config();
        let a = train(config, &train_data, &val_data).unwrap();
        let b = train(
            CharAnfConfig {
                incremental: false,
                ..config
            },
            &train_data,
            &val_data,
        )
        .unwrap();
        assert_eq!(a.model, b.model);
        assert_eq!(a.evaluations, b.evaluations);
        assert_eq!(a.accepted, b.accepted);
    }
}
