//! Gradient-free learning for [`LocalAnfNetwork`] on toy sequence tasks.
//!
//! * State-gate coefficients: seeded stochastic local search. Each proposal XORs
//!   one coefficient bit (or two bits of the same gate, half of the time); it is
//!   kept when the training error does not increase (ties are accepted so the
//!   search can cross plateaus), otherwise reverted. This is the hard-bit
//!   `delta(M) = L(base) - L(base XOR M)` intervention of `boolean_integral`,
//!   applied with an immediate accept/reject decision.
//! * Readout: for the current state dynamics the readout truth table is fitted
//!   exactly (majority target per readout address on the training split, ties
//!   to `0`) and converted to ANF coefficients by the Möbius transform. The
//!   training loss of a proposal is therefore the error of the best possible
//!   readout for that state dynamics.
//! * Validation: after every epoch the validation error is measured; the best
//!   validation network is kept and training stops after `patience` epochs
//!   without improvement or when validation error reaches zero.
//!
//! Everything is integer and deterministic given `seed`.

use crate::anf_local::{
    LocalAnfError, LocalAnfGate, LocalAnfNetwork, LocalAnfShape, LocalAnfWiring,
};
use crate::toy_tasks::{SplitMix64, ToySequence};

/// Up to 64 equal-length sequences packed into lane words.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LaneGroup {
    inputs: Vec<u64>,
    targets: Vec<u64>,
    valid: u64,
}

/// A split packed for bit-sliced evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackedSplit {
    groups: Vec<LaneGroup>,
    total: u64,
}

impl PackedSplit {
    /// Packs sequences 64 at a time. All sequences must share one length.
    pub fn new(sequences: &[ToySequence]) -> Result<Self, LocalAnfError> {
        let len = sequences.first().map_or(0, |s| s.inputs.len());
        let mut groups = Vec::new();
        let mut total = 0u64;
        for chunk in sequences.chunks(64) {
            let mut group = LaneGroup {
                inputs: vec![0; len],
                targets: vec![0; len],
                valid: 0,
            };
            for (lane, sequence) in chunk.iter().enumerate() {
                if sequence.inputs.len() != len || sequence.targets.len() != len {
                    return Err(LocalAnfError::Width(sequence.inputs.len()));
                }
                group.valid |= 1 << lane;
                for t in 0..len {
                    group.inputs[t] |= u64::from(sequence.inputs[t]) << lane;
                    group.targets[t] |= u64::from(sequence.targets[t]) << lane;
                }
            }
            total += chunk.len() as u64 * len as u64;
            groups.push(group);
        }
        Ok(Self { groups, total })
    }
    /// Number of (sequence, step) predictions in the split.
    pub fn total(&self) -> u64 {
        self.total
    }
}

/// Per-address `[count(target=0), count(target=1)]` for the readout inputs.
fn readout_counts(
    network: &LocalAnfNetwork,
    split: &PackedSplit,
    scratch: &mut Scratch,
) -> Vec<[u32; 2]> {
    let readout = network.readout();
    let mut counts = vec![[0u32; 2]; readout.monomial_count()];
    let n = network.state_bits();
    for group in &split.groups {
        scratch.source.iter_mut().for_each(|w| *w = 0);
        for (t, &x) in group.inputs.iter().enumerate() {
            scratch.source[n] = x;
            network.step_lanes(
                &mut scratch.source,
                &mut scratch.next,
                &mut scratch.products,
            );
            let target = group.targets[t];
            // Split lanes by readout address with bit-sliced partitioning.
            let mut remaining = group.valid;
            while remaining != 0 {
                let lane = remaining.trailing_zeros();
                let address = readout
                    .inputs()
                    .iter()
                    .enumerate()
                    .fold(0usize, |a, (j, &i)| {
                        a | ((((scratch.source[i] >> lane) & 1) as usize) << j)
                    });
                counts[address][((target >> lane) & 1) as usize] += 1;
                remaining &= remaining - 1;
            }
        }
    }
    counts
}

struct Scratch {
    source: Vec<u64>,
    next: Vec<u64>,
    products: Vec<u64>,
}

impl Scratch {
    fn new(network: &LocalAnfNetwork) -> Self {
        Self {
            source: vec![0; network.state_bits() + 1],
            next: vec![0; network.state_bits()],
            products: vec![0; network.scratch_len()],
        }
    }
}

/// Training error under the best readout for the current state dynamics.
fn best_readout_errors(
    network: &LocalAnfNetwork,
    split: &PackedSplit,
    scratch: &mut Scratch,
) -> u64 {
    readout_counts(network, split, scratch)
        .iter()
        .map(|c| u64::from(c[0].min(c[1])))
        .sum()
}

/// Replaces the readout with the exact majority fit on `split`.
pub fn fit_readout(
    network: &mut LocalAnfNetwork,
    split: &PackedSplit,
) -> Result<(), LocalAnfError> {
    let mut scratch = Scratch::new(network);
    let table: Vec<bool> = readout_counts(network, split, &mut scratch)
        .iter()
        .map(|c| c[1] > c[0])
        .collect();
    let readout = LocalAnfGate::from_truth_table(
        network.readout().inputs().to_vec(),
        &table,
        network.state_bits() + 1,
    )?;
    network.set_readout(readout)
}

/// Wrong predictions of the network (with its current readout) on `split`.
pub fn errors(network: &LocalAnfNetwork, split: &PackedSplit) -> u64 {
    let mut scratch = Scratch::new(network);
    let n = network.state_bits();
    let mut wrong = 0u64;
    for group in &split.groups {
        scratch.source.iter_mut().for_each(|w| *w = 0);
        for (t, &x) in group.inputs.iter().enumerate() {
            scratch.source[n] = x;
            network.step_lanes(
                &mut scratch.source,
                &mut scratch.next,
                &mut scratch.products,
            );
            let y = network
                .readout()
                .evaluate_lanes(&scratch.source, &mut scratch.products);
            wrong += u64::from(((y ^ group.targets[t]) & group.valid).count_ones());
        }
    }
    wrong
}

/// Search budget and wiring for one training run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalAnfLearnConfig {
    pub shape: LocalAnfShape,
    pub max_epochs: u32,
    pub proposals_per_epoch: u32,
    pub patience: u32,
    pub seed: u64,
}

impl Default for LocalAnfLearnConfig {
    fn default() -> Self {
        Self {
            shape: LocalAnfShape {
                state_bits: 32,
                fan_in: 6,
                readout_fan_in: 6,
                wiring: LocalAnfWiring::Local,
            },
            max_epochs: 300,
            proposals_per_epoch: 256,
            patience: 60,
            seed: 0,
        }
    }
}

/// Result of [`train`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalAnfLearnReport {
    /// Best-validation network, readout fitted on train.
    pub network: LocalAnfNetwork,
    pub train_errors: u64,
    pub val_errors: u64,
    pub epochs: u32,
    pub evaluations: u64,
    pub accepted: u64,
}

/// Applies one random move; returns whether an equal-loss result may be kept.
///
/// Move mix: 40% one coefficient flip, 20% two coefficient flips in one gate,
/// 25% replace a gate by a random sparse low-degree ANF (one or two monomials
/// of degree <= 2), 15% rewire one non-input readout position to a random
/// state bit. Neutral (equal-loss) moves are kept so plateaus can be crossed.
fn propose(network: &mut LocalAnfNetwork, rng: &mut SplitMix64) -> Result<bool, LocalAnfError> {
    let n = network.state_bits();
    let roll = rng.below(100);
    if roll >= 85 && network.readout().fan_in() > 1 {
        let readout = network.readout();
        let slot = 1 + rng.below(readout.fan_in() - 1);
        let candidate = rng.below(n);
        if readout.inputs().contains(&candidate) {
            return Ok(false);
        }
        let mut inputs = readout.inputs().to_vec();
        inputs[slot] = candidate;
        let gate = LocalAnfGate::zero(inputs, n + 1)?;
        network.set_readout(gate)?;
        return Ok(true);
    }
    let g = rng.below(n);
    let gate = &mut network.gates_mut()[g];
    let count = gate.monomial_count();
    let k = gate.fan_in();
    if roll < 60 {
        let first = rng.below(count);
        gate.flip(first);
        if roll >= 40 {
            gate.flip((first + 1 + rng.below(count - 1)) % count);
        }
    } else {
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
    Ok(true)
}

/// Learns state-gate coefficients by local search and fits the readout exactly.
pub fn train(
    config: LocalAnfLearnConfig,
    train_split: &PackedSplit,
    val_split: &PackedSplit,
) -> Result<LocalAnfLearnReport, LocalAnfError> {
    let mut network = LocalAnfNetwork::random(config.shape, config.seed)?;
    let mut rng = SplitMix64::new(config.seed ^ 0x5eed_a5a5_0000_0001);
    let mut scratch = Scratch::new(&network);
    let mut current = best_readout_errors(&network, train_split, &mut scratch);
    let mut evaluations = 1u64;
    let mut accepted = 0u64;
    fit_readout(&mut network, train_split)?;
    let mut best = (errors(&network, val_split), current, network.clone());
    let mut since_best = 0u32;
    let mut epochs = 0u32;
    while epochs < config.max_epochs && best.0 > 0 && since_best < config.patience {
        epochs += 1;
        for _ in 0..config.proposals_per_epoch {
            if current == 0 {
                break;
            }
            let before = network.clone();
            let neutral_allowed = propose(&mut network, &mut rng)?;
            let loss = best_readout_errors(&network, train_split, &mut scratch);
            evaluations += 1;
            if loss < current || (loss == current && neutral_allowed) {
                if loss < current {
                    accepted += 1;
                }
                current = loss;
            } else {
                network = before;
            }
        }
        fit_readout(&mut network, train_split)?;
        let val = errors(&network, val_split);
        if val < best.0 {
            best = (val, current, network.clone());
            since_best = 0;
        } else {
            since_best += 1;
        }
    }
    Ok(LocalAnfLearnReport {
        network: best.2,
        train_errors: best.1,
        val_errors: best.0,
        epochs,
        evaluations,
        accepted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toy_tasks::{ToySplitSpec, ToySplits, ToyTask};

    fn splits(task: ToyTask, seed: u64) -> (ToySplits, PackedSplit, PackedSplit) {
        let s = ToySplits::new(task, ToySplitSpec::default(), seed);
        let train = PackedSplit::new(&s.train).unwrap();
        let val = PackedSplit::new(&s.val).unwrap();
        (s, train, val)
    }

    #[test]
    fn packed_errors_match_scalar_reference() {
        let (s, train, _) = splits(ToyTask::WindowMajority(3), 2);
        let mut net = LocalAnfNetwork::random(LocalAnfLearnConfig::default().shape, 8).unwrap();
        fit_readout(&mut net, &train).unwrap();
        let wrong: usize = s
            .train
            .iter()
            .map(|q| {
                net.run(&q.inputs)
                    .iter()
                    .zip(&q.targets)
                    .filter(|(a, b)| a != b)
                    .count()
            })
            .sum();
        assert_eq!(errors(&net, &train), wrong as u64);
        assert_eq!(train.total(), 128 * 32);
    }

    #[test]
    fn learner_exactly_fits_a_known_small_recurrent_anf_target() {
        // Target generated by a known local ANF network: hidden gate 0 computes
        // s0' = s0 XOR x (running parity) and the task reads it out.
        let task = ToyTask::RunningParity;
        let (s, train, val) = splits(task, 3);
        let config = LocalAnfLearnConfig {
            shape: LocalAnfShape {
                state_bits: 8,
                fan_in: 4,
                readout_fan_in: 3,
                wiring: LocalAnfWiring::Random,
            },
            seed: 1,
            ..LocalAnfLearnConfig::default()
        };
        let report = super::train(config, &train, &val).unwrap();
        assert_eq!(report.train_errors, 0);
        assert_eq!(report.val_errors, 0);
        let test = PackedSplit::new(&s.test).unwrap();
        let long = PackedSplit::new(&s.test_long).unwrap();
        assert_eq!(errors(&report.network, &test), 0);
        assert_eq!(errors(&report.network, &long), 0);
        // The learned network is an exact running-parity machine on any input.
        let inputs: Vec<bool> = (0..300).map(|t| (t * t + 3 * t) % 7 < 3).collect();
        assert_eq!(report.network.run(&inputs), task.targets(&inputs));
    }

    #[test]
    fn training_is_deterministic_per_seed() {
        let (_, train, val) = splits(ToyTask::DelayedRecall(2), 4);
        let config = LocalAnfLearnConfig {
            max_epochs: 3,
            proposals_per_epoch: 64,
            seed: 9,
            ..LocalAnfLearnConfig::default()
        };
        let a = super::train(config, &train, &val).unwrap();
        assert_eq!(a, super::train(config, &train, &val).unwrap());
        let other = LocalAnfLearnConfig { seed: 10, ..config };
        assert_ne!(
            a.network,
            super::train(other, &train, &val).unwrap().network
        );
    }

    #[test]
    fn fitted_readout_is_the_exact_minimum_error_readout() {
        let (_, train, _) = splits(ToyTask::WindowParity(3), 5);
        let mut net = LocalAnfNetwork::random(LocalAnfLearnConfig::default().shape, 6).unwrap();
        let mut scratch = Scratch::new(&net);
        let bound = best_readout_errors(&net, &train, &mut scratch);
        fit_readout(&mut net, &train).unwrap();
        assert_eq!(errors(&net, &train), bound);
        // Flipping any single readout coefficient cannot do better.
        for m in 0..net.readout().monomial_count() {
            let mut readout = net.readout().clone();
            readout.flip(m);
            let mut other = net.clone();
            other.set_readout(readout).unwrap();
            assert!(errors(&other, &train) >= bound);
        }
    }
}
