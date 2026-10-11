//! A5 step 3, part 1: proposal throughput of full re-simulation vs incremental
//! evaluation (cached trajectory, downstream cone only).
//!
//! Usage: `sml-anf-throughput [corpus-path]` (prints a markdown table).
//! Without a corpus the byte setting uses deterministic synthetic text.
//!
//! Settings:
//! * `toy`: delayed recall d=8 train split (128 sequences x 32 steps), random
//!   local wiring, fan-in 6, one readout of fan-in 6, error loss; every state
//!   gate is mutable.
//! * `byte`: 64 lanes x 1023 steps of text, 8 input bits; 32 frozen history
//!   gates (L=4, fitted greedily) and `N - 32` free gates, fan-in 6; 8 chained
//!   readouts reading 6 history/input bits; leave-one-out log-loss.
//!   Only the free gates are mutated.
//!
//! The proposal is one or two coefficient flips in a random mutable gate,
//! accepted when the loss does not increase. Both evaluators run the same
//! proposal sequence and must reach the same loss (checked).

use std::time::Instant;

use sml_core::anf_char::{self, CharAnfConfig, BYTE_BITS};
use sml_core::anf_local::{LocalAnfNetwork, LocalAnfShape, LocalAnfWiring};
use sml_core::anf_local_incremental::{
    full_loss, IncrementalEvaluator, LaneData, LossKind, ReadoutInput, ReadoutSpec,
};
use sml_core::anf_local_learn::PackedSplit;
use sml_core::toy_tasks::{SplitMix64, ToySplitSpec, ToySplits, ToyTask};

struct Setting {
    network: LocalAnfNetwork,
    data: LaneData,
    specs: Vec<ReadoutSpec>,
    kind: LossKind,
    mutable: std::ops::Range<usize>,
}

/// Runs `proposals` proposals (or until `budget` seconds); returns (count, seconds, loss, gate evals).
fn run(
    setting: &Setting,
    incremental: bool,
    proposals: usize,
    budget: f64,
) -> (usize, f64, i64, u64) {
    let mut network = setting.network.clone();
    let mut evaluator =
        IncrementalEvaluator::new(&network, &setting.data, setting.specs.clone(), setting.kind)
            .expect("evaluator");
    let mut current = evaluator.loss();
    let mut rng = SplitMix64::new(0x7470_7574);
    let span = setting.mutable.end - setting.mutable.start;
    let clock = Instant::now();
    let mut done = 0;
    let mut gate_evals = 0u64;
    while done < proposals && clock.elapsed().as_secs_f64() < budget {
        let g = setting.mutable.start + rng.below(span);
        let before = network.gates()[g].clone();
        let gate = &mut network.gates_mut()[g];
        let count = gate.monomial_count();
        let first = rng.below(count);
        gate.flip(first);
        if rng.below(3) == 0 {
            gate.flip((first + 1 + rng.below(count - 1)) % count);
        }
        let loss = if incremental {
            evaluator.try_gate(&network, &setting.data, g).expect("try")
        } else {
            gate_evals += (setting.network.state_bits() as u64)
                * setting
                    .data
                    .blocks
                    .iter()
                    .map(|b| b.steps as u64)
                    .sum::<u64>();
            full_loss(&network, &setting.data, &setting.specs, setting.kind).expect("full")
        };
        if loss <= current {
            current = loss;
            if incremental {
                evaluator.commit();
            }
        } else {
            if incremental {
                evaluator.discard();
            }
            network.gates_mut()[g] = before;
        }
        done += 1;
    }
    if incremental {
        gate_evals = evaluator.gate_evaluations;
    }
    (done, clock.elapsed().as_secs_f64(), current, gate_evals)
}

fn toy_setting(n: usize) -> Setting {
    let splits = ToySplits::new(ToyTask::DelayedRecall(8), ToySplitSpec::default(), 0);
    let data = PackedSplit::new(&splits.train).expect("pack").lane_data();
    let network = LocalAnfNetwork::random(
        LocalAnfShape {
            state_bits: n,
            fan_in: 6,
            readout_fan_in: 6,
            wiring: LocalAnfWiring::Local,
        },
        1,
    )
    .expect("network");
    let specs = vec![ReadoutSpec {
        inputs: network
            .readout()
            .inputs()
            .iter()
            .map(|&p| ReadoutInput::Source(p))
            .collect(),
        target: 0,
    }];
    Setting {
        network,
        data,
        specs,
        kind: LossKind::Errors,
        mutable: 0..n,
    }
}

fn byte_setting(n: usize, text: &[u8]) -> Setting {
    let config = CharAnfConfig {
        lags: 4,
        free_gates: n - 4 * BYTE_BITS,
        fan_in: 6,
        seed: 1,
        ..CharAnfConfig::default()
    };
    let data = anf_char::pack_bytes(&text[..64 * 1024]).expect("pack");
    let mut network = anf_char::initial_network(&config).expect("network");
    anf_char::fit_history(&mut network, &config, &data).expect("history");
    let specs = (0..BYTE_BITS)
        .map(|b| {
            let mut inputs: Vec<ReadoutInput> = (0..b).map(ReadoutInput::Target).collect();
            inputs.extend((0..6).map(|j| ReadoutInput::Source(config.free_gates + 8 + j)));
            ReadoutSpec { inputs, target: b }
        })
        .collect();
    Setting {
        network,
        data,
        specs,
        kind: LossKind::LeaveOneOutBits,
        mutable: 0..config.free_gates,
    }
}

fn main() {
    let text = match std::env::args().nth(1) {
        Some(path) => std::fs::read(&path).expect("read corpus"),
        None => anf_char::synthetic_text(64 * 1100, 1),
    };
    println!("| Setting | N | steps x lanes | proposals | full (proposals/s) | incremental (proposals/s) | speedup | gate evals per proposal (full / incr.) | same final loss | long run: proposals | long run: incr. proposals/s | long run: gate evals per proposal | long-run speedup |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|---|");
    for (name, sizes) in [
        ("toy d=8", [32usize, 256, 1024, 4096]),
        ("byte", [64, 256, 1024, 4096]),
    ] {
        for n in sizes {
            let setting = if name == "byte" {
                byte_setting(n, &text)
            } else {
                toy_setting(n)
            };
            let steps: usize = setting.data.blocks.iter().map(|b| b.steps).sum();
            let (count, full_secs, full_loss_value, full_evals) = run(&setting, false, 400, 20.0);
            let (count_i, incr_secs, incr_loss, incr_evals) =
                run(&setting, true, count, f64::INFINITY);
            assert_eq!(count_i, count);
            let full_rate = count as f64 / full_secs;
            let incr_rate = count as f64 / incr_secs;
            // Longer incremental run (up to 20000 proposals or 10 s): accepted
            // flips accumulate and the downstream cone of later proposals grows.
            let (long_count, long_secs, _, long_evals) = run(&setting, true, 20_000, 10.0);
            let long_rate = long_count as f64 / long_secs;
            println!(
                "| {name} | {n} | {steps} x 64 | {count} | {full_rate:.1} | {incr_rate:.1} | {:.1}x | {:.0} / {:.0} | {} | {long_count} | {long_rate:.1} | {:.0} | {:.1}x |",
                incr_rate / full_rate,
                full_evals as f64 / count as f64,
                incr_evals as f64 / count as f64,
                if full_loss_value == incr_loss { "yes" } else { "NO" },
                long_evals as f64 / long_count as f64,
                long_rate / full_rate,
            );
        }
    }
}
