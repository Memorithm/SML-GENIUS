//! A5 toy learning benchmark: local fan-in ANF recurrent model vs baselines.
//!
//! Usage: `sml-learn-toy [seeds] [state_bits] [fan_in] [local|random] [history_lags]`
//! (defaults 5, 32, 6, local, 16). Prints a markdown table (mean ± std over
//! seeds of per-step accuracy). Accuracy numbers are deterministic per seed;
//! wall-clock times are not.
//!
//! Learners: "ANF v1" is the step-1 learner (task-loss local search only).
//! "ANF+hist" first fits a self-supervised history layer (A5 step 2, see
//! `anf_local_history`), then runs the same search on the free gates.
//! "hist+readout" is the ablation without free-gate search.
use sml_core::anf_local::{LocalAnfShape, LocalAnfWiring};
use sml_core::anf_local_history::{fit_history_layer, train_with_history};
use sml_core::anf_local_learn::{errors, train, LocalAnfLearnConfig, PackedSplit};
use sml_core::toy_baselines::{ConstantBaseline, ElmanRnn, NGramBaseline, RnnConfig};
use sml_core::toy_tasks::{accuracy, generate, SplitMix64, ToySplitSpec, ToySplits, ToyTask};
use std::time::Instant;

const NGRAM_WINDOW: usize = 9;
const XLONG_LEN: usize = 512;

fn stats(values: &[f64]) -> (f64, f64) {
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let var = values.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / n;
    (mean, var.sqrt())
}

fn pct(values: &[f64]) -> String {
    let (m, s) = stats(values);
    format!("{:.1} ± {:.1}", 100.0 * m, 100.0 * s)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize, d: usize| -> Result<usize, Box<dyn std::error::Error>> {
        Ok(args.get(i).map(|a| a.parse()).transpose()?.unwrap_or(d))
    };
    let seeds = arg(0, 5)? as u64;
    let shape = LocalAnfShape {
        state_bits: arg(1, 32)?,
        fan_in: arg(2, 6)?,
        readout_fan_in: 6,
        wiring: match args.get(3).map(String::as_str) {
            None | Some("local") => LocalAnfWiring::Local,
            Some("random") => LocalAnfWiring::Random,
            Some(other) => return Err(format!("unknown wiring {other}").into()),
        },
    };
    let lags = arg(4, 16)?;
    let spec = ToySplitSpec::default();
    let tasks = [
        ToyTask::RunningParity,
        ToyTask::WindowParity(3),
        ToyTask::WindowParity(5),
        ToyTask::DelayedRecall(1),
        ToyTask::DelayedRecall(2),
        ToyTask::DelayedRecall(4),
        ToyTask::DelayedRecall(8),
        ToyTask::DelayedRecall(12),
        ToyTask::DelayedRecall(16),
        ToyTask::WindowMajority(5),
    ];
    println!(
        "Splits: train {}x{}, val {}x{}, test {}x{}, test-long {}x{}, test-xlong 64x{XLONG_LEN} (sequences x steps); seeds 0..{seeds}",
        spec.train, spec.len, spec.val, spec.len, spec.test, spec.len, spec.test_long, spec.long_len
    );
    println!(
        "ANF: N={} state bits, fan-in k={}, readout fan-in {}, wiring {:?}, history lags L={lags} (free gates {}); RNN: Elman H={}; n-gram window {NGRAM_WINDOW}\n",
        shape.state_bits,
        shape.fan_in,
        shape.readout_fan_in,
        shape.wiring,
        shape.state_bits - lags,
        RnnConfig::default().hidden,
    );
    println!("| Task | Constant | n-gram (w=9) | RNN test-long | ANF v1 test-long (exact) | hist+readout test-long (exact) | ANF+hist train | ANF+hist test | ANF+hist test-long (exact) | ANF+hist test-xlong | RNN test-xlong | time v1 (s) | time hist fit (ms) | time ANF+hist total (s) | time RNN (s) |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|");
    let mut summary = String::new();
    for task in tasks {
        let mut rows: [Vec<f64>; 10] = Default::default();
        let mut exact = [0u32; 3];
        let mut times = [0.0f64; 4];
        for seed in 0..seeds {
            let s = ToySplits::new(task, spec, seed);
            let xlong = generate(
                task,
                64,
                XLONG_LEN,
                SplitMix64::new(seed ^ 0x0516).next_u64(),
            );
            let constant = ConstantBaseline::fit(&s.train);
            rows[0].push(accuracy(&s.test, |x| constant.predict(x)));
            let ngram = NGramBaseline::fit(&s.train, NGRAM_WINDOW);
            rows[1].push(accuracy(&s.test, |x| ngram.predict(x)));
            let start = Instant::now();
            let (rnn, _) = ElmanRnn::train(
                RnnConfig {
                    seed,
                    ..RnnConfig::default()
                },
                &s.train,
                &s.val,
            );
            times[3] += start.elapsed().as_secs_f64();
            rows[2].push(accuracy(&s.test_long, |x| rnn.predict(x)));
            rows[9].push(accuracy(&xlong, |x| rnn.predict(x)));
            let packed = |q: &[_]| PackedSplit::new(q);
            let (tr, va, te, tl, xl) = (
                packed(&s.train)?,
                packed(&s.val)?,
                packed(&s.test)?,
                packed(&s.test_long)?,
                packed(&xlong)?,
            );
            let acc = |net: &_, p: &PackedSplit| 1.0 - errors(net, p) as f64 / p.total() as f64;
            let config = LocalAnfLearnConfig {
                shape,
                seed,
                ..LocalAnfLearnConfig::default()
            };
            // Step-1 learner (baseline column).
            let start = Instant::now();
            let v1 = train(config, &tr, &va)?;
            times[0] += start.elapsed().as_secs_f64();
            rows[3].push(acc(&v1.network, &tl));
            exact[0] += u32::from(errors(&v1.network, &tl) == 0);
            // Ablation: history layer + readout search only (free gates frozen at init).
            let ablation = train_with_history(config, lags, false, &tr, &va)?;
            rows[4].push(acc(&ablation.search.network, &tl));
            exact[1] += u32::from(errors(&ablation.search.network, &tl) == 0);
            // History-layer fit cost alone.
            let start = Instant::now();
            let mut probe = sml_core::anf_local::LocalAnfNetwork::random(shape, seed)?;
            fit_history_layer(&mut probe, lags, shape.fan_in, &tr)?;
            times[1] += start.elapsed().as_secs_f64();
            // Full step-2 learner.
            let start = Instant::now();
            let full = train_with_history(config, lags, true, &tr, &va)?;
            times[2] += start.elapsed().as_secs_f64();
            let net = &full.search.network;
            rows[5].push(acc(net, &tr));
            rows[6].push(acc(net, &te));
            rows[7].push(acc(net, &tl));
            rows[8].push(acc(net, &xl));
            exact[2] += u32::from(errors(net, &tl) == 0);
            eprintln!(
                "{task} seed={seed}: v1 long={:.3} | hist+readout long={:.3} | hist epochs={} evals={} train_err={} val_err={} test={:.3} long={:.3} xlong={:.3} hist_fit_errors={} | rnn long={:.3}",
                rows[3].last().copied().unwrap_or(0.0),
                rows[4].last().copied().unwrap_or(0.0),
                full.search.epochs,
                full.search.evaluations,
                full.search.train_errors,
                full.search.val_errors,
                acc(net, &te),
                acc(net, &tl),
                acc(net, &xl),
                full.history.iter().map(|h| h.train_errors).sum::<u64>(),
                rows[2].last().copied().unwrap_or(0.0),
            );
            if summary.is_empty() {
                summary = format!(
                    "ANF learned coefficient bits: {} ({} history-layer gates fitted greedily, {} free gates searched); state/input bits read per token: {}. RNN f32 parameters: {}.",
                    net.learned_bits(),
                    lags,
                    shape.state_bits - lags,
                    net.bits_read_per_token(),
                    rnn.parameter_count()
                );
            }
        }
        let n = seeds as f64;
        println!(
            "| {task} | {} | {} | {} | {} ({}/{seeds}) | {} ({}/{seeds}) | {} | {} | {} ({}/{seeds}) | {} | {} | {:.1} | {:.2} | {:.2} | {:.1} |",
            pct(&rows[0]),
            pct(&rows[1]),
            pct(&rows[2]),
            pct(&rows[3]),
            exact[0],
            pct(&rows[4]),
            exact[1],
            pct(&rows[5]),
            pct(&rows[6]),
            pct(&rows[7]),
            exact[2],
            pct(&rows[8]),
            pct(&rows[9]),
            times[0] / n,
            times[1] / n * 1000.0,
            times[2] / n,
            times[3] / n,
        );
    }
    println!("\n{summary}");
    Ok(())
}
