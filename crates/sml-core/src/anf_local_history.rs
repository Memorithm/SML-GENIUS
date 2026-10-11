//! A5 step 2: self-supervised "history encoding" layer for [`LocalAnfNetwork`].
//!
//! Problem addressed (see `docs/RESULTS-A5-TOY.md`): with only the task loss,
//! the intermediate gates of a long memory chain receive no credit, so local
//! search cannot build e.g. an 8-step delay.
//!
//! Scheme (task-agnostic, uses the **input stream only**, never task targets):
//! `L` history gates `h_1..h_L` occupy the last `L` state positions. Gate `h_j`
//! gets the auxiliary target "the input seen `j-1` steps before the current
//! one", i.e. `h_j(t+1) = x_{t+1-j}` (`h_1` copies `x_t`, `h_2` holds `x_{t-1}`,
//! ...; zero before the sequence start). Gates are fitted greedily in order
//! `j = 1..L`, each one exactly:
//!
//! 1. *Source selection*: forward selection over the current input `x` and the
//!    already-fitted history gates (never the gate itself, never free gates),
//!    adding the source that most reduces the best achievable error, up to the
//!    fan-in `k`, stopping when the error is zero or stops decreasing.
//! 2. *Exact truth-table fit*: majority target per address on the training
//!    split (ties to 0), Möbius-converted to ANF coefficients, like the readout.
//!
//! Knowledge injected: "the last `L` inputs are useful features" (a learned
//! tapped delay line with horizon `L`). Nothing about which lag, window or
//! function a task needs: the remaining free gates and the readout are then
//! learned on the task loss with the unchanged local search, and the readout
//! must itself discover which history bits to read.

use crate::anf_local::{LocalAnfError, LocalAnfGate, LocalAnfNetwork};
use crate::anf_local_learn::{search, LocalAnfLearnConfig, LocalAnfLearnReport, PackedSplit};

/// Source of a history gate input: the current input bit or a fitted history gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistorySource {
    Input,
    /// Index `j - 1` of history gate `h_j` (already fitted).
    History(usize),
}

/// Outcome of fitting one history gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryGateFit {
    /// Lag `j` (target `x_{t+1-j}` for `s_{t+1}`).
    pub lag: usize,
    /// State position of the gate.
    pub position: usize,
    pub sources: Vec<HistorySource>,
    /// Training errors of the fitted gate on its auxiliary target.
    pub train_errors: u64,
}

/// Fits `lags` history gates into the last `lags` positions of `network`.
///
/// `max_fan_in` bounds the number of selected sources (1..=8).
pub fn fit_history_layer(
    network: &mut LocalAnfNetwork,
    lags: usize,
    max_fan_in: usize,
    train: &PackedSplit,
) -> Result<Vec<HistoryGateFit>, LocalAnfError> {
    let n = network.state_bits();
    if lags == 0 || lags > n {
        return Err(LocalAnfError::Width(lags));
    }
    if max_fan_in == 0 || max_fan_in > crate::anf_local::LOCAL_ANF_MAX_FAN_IN {
        return Err(LocalAnfError::FanIn(max_fan_in));
    }
    let first = n - lags;
    // trajectories[g][h][t] = value of history gate h in state s_t, group g.
    let mut trajectories: Vec<Vec<Vec<u64>>> = vec![Vec::new(); train.groups.len()];
    let mut fits = Vec::with_capacity(lags);
    for j in 1..=lags {
        let candidates: Vec<HistorySource> = core::iter::once(HistorySource::Input)
            .chain((0..j - 1).map(HistorySource::History))
            .collect();
        let mut selected: Vec<HistorySource> = Vec::new();
        let mut best_error = counts_error(&counts(train, &trajectories, &selected, j));
        while selected.len() < max_fan_in && best_error > 0 {
            let mut choice: Option<(u64, HistorySource)> = None;
            for &candidate in &candidates {
                if selected.contains(&candidate) {
                    continue;
                }
                let mut trial = selected.clone();
                trial.push(candidate);
                let error = counts_error(&counts(train, &trajectories, &trial, j));
                if choice.is_none_or(|(e, _)| error < e) {
                    choice = Some((error, candidate));
                }
            }
            match choice {
                Some((error, candidate)) if error < best_error => {
                    selected.push(candidate);
                    best_error = error;
                }
                _ => break,
            }
        }
        if selected.is_empty() {
            // Degenerate target (e.g. constant on train): read `x` with a zero table.
            selected.push(HistorySource::Input);
        }
        let table: Vec<bool> = counts(train, &trajectories, &selected, j)
            .iter()
            .map(|c| c[1] > c[0])
            .collect();
        let position = first + j - 1;
        let inputs: Vec<usize> = selected
            .iter()
            .map(|s| match s {
                HistorySource::Input => n,
                HistorySource::History(h) => first + h,
            })
            .collect();
        let gate = LocalAnfGate::from_truth_table(inputs, &table, n + 1)?;
        // Record the new gate's trajectory s_0..s_T on every group.
        for (g, group) in train.groups.iter().enumerate() {
            let len = group.inputs.len();
            let mut trajectory = vec![0u64; len + 1];
            for t in 0..len {
                let mut word = 0u64;
                for (address, &value) in table.iter().enumerate() {
                    if !value {
                        continue;
                    }
                    let mut lanes = u64::MAX;
                    for (bit, source) in selected.iter().enumerate() {
                        let w = source_word(group.inputs[t], &trajectories[g], *source, t);
                        lanes &= if (address >> bit) & 1 == 1 { w } else { !w };
                    }
                    word |= lanes;
                }
                trajectory[t + 1] = word;
            }
            trajectories[g].push(trajectory);
        }
        network.gates_mut()[position] = gate;
        fits.push(HistoryGateFit {
            lag: j,
            position,
            sources: selected,
            train_errors: best_error,
        });
    }
    Ok(fits)
}

fn source_word(input: u64, history: &[Vec<u64>], source: HistorySource, t: usize) -> u64 {
    match source {
        HistorySource::Input => input,
        HistorySource::History(h) => history[h][t],
    }
}

/// Auxiliary target word for lag `j` at step `t`: `x_{t+1-j}` (0 before start).
fn target_word(inputs: &[u64], j: usize, t: usize) -> u64 {
    (t + 1).checked_sub(j).map_or(0, |i| inputs[i])
}

fn counts(
    train: &PackedSplit,
    trajectories: &[Vec<Vec<u64>>],
    sources: &[HistorySource],
    j: usize,
) -> Vec<[u64; 2]> {
    let mut counts = vec![[0u64; 2]; 1 << sources.len()];
    for (g, group) in train.groups.iter().enumerate() {
        for t in 0..group.inputs.len() {
            let target = target_word(&group.inputs, j, t);
            let words: Vec<u64> = sources
                .iter()
                .map(|&s| source_word(group.inputs[t], &trajectories[g], s, t))
                .collect();
            for (address, slot) in counts.iter_mut().enumerate() {
                let mut lanes = group.valid;
                for (bit, &w) in words.iter().enumerate() {
                    lanes &= if (address >> bit) & 1 == 1 { w } else { !w };
                }
                slot[1] += u64::from((lanes & target).count_ones());
                slot[0] += u64::from((lanes & !target).count_ones());
            }
        }
    }
    counts
}

fn counts_error(counts: &[[u64; 2]]) -> u64 {
    counts.iter().map(|c| c[0].min(c[1])).sum()
}

/// Report of [`train_with_history`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryLearnReport {
    pub history: Vec<HistoryGateFit>,
    /// Task-loss search over the free gates and the readout (history frozen).
    pub search: LocalAnfLearnReport,
}

/// Fits the history layer, then runs the unchanged task-loss local search on
/// the `N - lags` free gates (and readout); `search_free_gates = false` keeps
/// the free gates at their random initialization and only rewires/fits the
/// readout (ablation).
pub fn train_with_history(
    config: LocalAnfLearnConfig,
    lags: usize,
    search_free_gates: bool,
    train: &PackedSplit,
    val: &PackedSplit,
) -> Result<HistoryLearnReport, LocalAnfError> {
    let mut network = LocalAnfNetwork::random(config.shape, config.seed)?;
    let history = fit_history_layer(&mut network, lags, config.shape.fan_in, train)?;
    let mutable = if search_free_gates {
        network.state_bits() - lags
    } else {
        0
    };
    let search = search(network, mutable, config, train, val)?;
    Ok(HistoryLearnReport { history, search })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anf_local::{LocalAnfShape, LocalAnfWiring};
    use crate::anf_local_learn::errors;
    use crate::toy_tasks::{ToySplitSpec, ToySplits, ToyTask};

    fn config(seed: u64) -> LocalAnfLearnConfig {
        LocalAnfLearnConfig {
            shape: LocalAnfShape {
                state_bits: 32,
                fan_in: 6,
                readout_fan_in: 6,
                wiring: LocalAnfWiring::Local,
            },
            seed,
            ..LocalAnfLearnConfig::default()
        }
    }

    #[test]
    fn greedy_fit_builds_an_exact_delay_line_from_inputs_only() {
        // Task targets are irrelevant to the history layer: fit on a parity task.
        let s = ToySplits::new(ToyTask::RunningParity, ToySplitSpec::default(), 1);
        let train = PackedSplit::new(&s.train).unwrap();
        let mut net = LocalAnfNetwork::random(config(1).shape, 1).unwrap();
        let fits = fit_history_layer(&mut net, 12, 6, &train).unwrap();
        assert_eq!(fits.len(), 12);
        assert_eq!(fits[0].sources, vec![HistorySource::Input]);
        for (j, fit) in fits.iter().enumerate() {
            assert_eq!(fit.train_errors, 0, "lag {}", fit.lag);
            assert_eq!(fit.position, 20 + j);
            if j > 0 {
                // Source selection finds the previous lag on its own.
                assert_eq!(fit.sources, vec![HistorySource::History(j - 1)]);
            }
        }
        // The fitted gates hold the past inputs on unseen, longer sequences.
        for q in &s.test_long {
            let mut state = vec![false; 32];
            for (t, &x) in q.inputs.iter().enumerate() {
                state = net.step(&state, x).0;
                for j in 1..=12 {
                    let expected = (t + 1).checked_sub(j).is_some_and(|i| q.inputs[i]);
                    assert_eq!(state[20 + j - 1], expected);
                }
            }
        }
    }

    #[test]
    fn history_layer_does_not_depend_on_task_targets() {
        let a = ToySplits::new(ToyTask::RunningParity, ToySplitSpec::default(), 4);
        let b = ToySplits::new(ToyTask::DelayedRecall(3), ToySplitSpec::default(), 4);
        assert_eq!(
            a.train.iter().map(|q| &q.inputs).collect::<Vec<_>>(),
            b.train.iter().map(|q| &q.inputs).collect::<Vec<_>>()
        );
        let mut na = LocalAnfNetwork::random(config(4).shape, 4).unwrap();
        let mut nb = na.clone();
        let fa = fit_history_layer(&mut na, 8, 6, &PackedSplit::new(&a.train).unwrap()).unwrap();
        let fb = fit_history_layer(&mut nb, 8, 6, &PackedSplit::new(&b.train).unwrap()).unwrap();
        assert_eq!(fa, fb);
        assert_eq!(na, nb);
    }

    #[test]
    fn search_keeps_history_gates_frozen_and_learns_delay_eight() {
        let s = ToySplits::new(ToyTask::DelayedRecall(8), ToySplitSpec::default(), 0);
        let train = PackedSplit::new(&s.train).unwrap();
        let val = PackedSplit::new(&s.val).unwrap();
        let report = train_with_history(config(0), 16, true, &train, &val).unwrap();
        let mut fitted = LocalAnfNetwork::random(config(0).shape, 0).unwrap();
        fit_history_layer(&mut fitted, 16, 6, &train).unwrap();
        assert_eq!(&report.search.network.gates()[16..], &fitted.gates()[16..]);
        let long = PackedSplit::new(&s.test_long).unwrap();
        assert_eq!(errors(&report.search.network, &long), 0);
        assert!(report.search.network.readout().inputs().contains(&(16 + 8)));
    }

    #[test]
    fn invalid_history_sizes_are_rejected() {
        let s = ToySplits::new(ToyTask::RunningParity, ToySplitSpec::default(), 1);
        let train = PackedSplit::new(&s.train).unwrap();
        let mut net = LocalAnfNetwork::random(config(1).shape, 1).unwrap();
        assert_eq!(
            fit_history_layer(&mut net, 0, 6, &train),
            Err(LocalAnfError::Width(0))
        );
        assert_eq!(
            fit_history_layer(&mut net, 33, 6, &train),
            Err(LocalAnfError::Width(33))
        );
        assert_eq!(
            fit_history_layer(&mut net, 4, 9, &train),
            Err(LocalAnfError::FanIn(9))
        );
    }
}
