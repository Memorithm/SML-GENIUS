//! A5 toy learning benchmark: local fan-in ANF recurrent model vs baselines.
//!
//! Usage: `sml-learn-toy [seeds] [state_bits] [fan_in] [local|random]`
//! (defaults 5, 32, 6, local).
//! Prints a markdown table (mean ± std over seeds of per-step test accuracy).
//! Accuracy numbers are deterministic per seed; wall-clock times are not.
use sml_core::anf_local::{LocalAnfShape, LocalAnfWiring};
use sml_core::anf_local_learn::{errors, train, LocalAnfLearnConfig, PackedSplit};
use sml_core::toy_baselines::{ConstantBaseline, ElmanRnn, NGramBaseline, RnnConfig};
use sml_core::toy_tasks::{accuracy, ToySplitSpec, ToySplits, ToyTask};
use std::time::Instant;

const NGRAM_WINDOW: usize = 9;

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
    let spec = ToySplitSpec::default();
    let tasks = [
        ToyTask::RunningParity,
        ToyTask::WindowParity(3),
        ToyTask::WindowParity(5),
        ToyTask::DelayedRecall(1),
        ToyTask::DelayedRecall(2),
        ToyTask::DelayedRecall(4),
        ToyTask::DelayedRecall(8),
        ToyTask::WindowMajority(5),
    ];
    println!(
        "Splits: train {}x{}, val {}x{}, test {}x{}, test-long {}x{} (sequences x steps); seeds 0..{}",
        spec.train, spec.len, spec.val, spec.len, spec.test, spec.len, spec.test_long, spec.long_len, seeds
    );
    println!(
        "ANF: N={} state bits, fan-in k={}, readout fan-in {}, wiring {:?}; RNN: Elman H={}; n-gram window {}\n",
        shape.state_bits,
        shape.fan_in,
        shape.readout_fan_in,
        shape.wiring,
        RnnConfig::default().hidden,
        NGRAM_WINDOW
    );
    println!("| Task | Constant | n-gram (w=9) | RNN test | RNN test-long | ANF init+readout only (test) | ANF train | ANF test | ANF test-long | ANF exact runs (test-long 100%) | ANF time/run (s) | RNN time/run (s) |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|");
    let mut learned_bits = 0;
    let mut bits_read = 0;
    let mut rnn_params = 0;
    for task in tasks {
        let mut rows: [Vec<f64>; 9] = Default::default();
        let mut exact = 0;
        let (mut anf_time, mut rnn_time) = (0.0, 0.0);
        for seed in 0..seeds {
            let s = ToySplits::new(task, spec, seed);
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
            rnn_time += start.elapsed().as_secs_f64();
            rnn_params = rnn.parameter_count();
            rows[2].push(accuracy(&s.test, |x| rnn.predict(x)));
            rows[3].push(accuracy(&s.test_long, |x| rnn.predict(x)));
            let packed = |q: &[_]| PackedSplit::new(q);
            let (tr, va, te, tl) = (
                packed(&s.train)?,
                packed(&s.val)?,
                packed(&s.test)?,
                packed(&s.test_long)?,
            );
            // Control: frozen random initial network, readout fitted, no state search.
            let control = train(
                LocalAnfLearnConfig {
                    shape,
                    seed,
                    max_epochs: 0,
                    ..LocalAnfLearnConfig::default()
                },
                &tr,
                &va,
            )?;
            rows[8].push(1.0 - errors(&control.network, &te) as f64 / te.total() as f64);
            let start = Instant::now();
            let report = train(
                LocalAnfLearnConfig {
                    shape,
                    seed,
                    ..LocalAnfLearnConfig::default()
                },
                &tr,
                &va,
            )?;
            anf_time += start.elapsed().as_secs_f64();
            let acc = |p: &PackedSplit| 1.0 - errors(&report.network, p) as f64 / p.total() as f64;
            rows[4].push(acc(&tr));
            rows[5].push(acc(&te));
            rows[6].push(acc(&tl));
            if errors(&report.network, &tl) == 0 {
                exact += 1;
            }
            learned_bits = report.network.learned_bits();
            bits_read = report.network.bits_read_per_token();
            eprintln!(
                "{task} seed={seed}: anf epochs={} evals={} train_err={} val_err={} test={:.3} long={:.3} | rnn test={:.3}",
                report.epochs, report.evaluations, report.train_errors, report.val_errors,
                acc(&te), acc(&tl), rows[2].last().copied().unwrap_or(0.0)
            );
        }
        println!(
            "| {task} | {} | {} | {} | {} | {} | {} | {} | {} | {exact}/{seeds} | {:.1} | {:.1} |",
            pct(&rows[0]),
            pct(&rows[1]),
            pct(&rows[2]),
            pct(&rows[3]),
            pct(&rows[8]),
            pct(&rows[4]),
            pct(&rows[5]),
            pct(&rows[6]),
            anf_time / seeds as f64,
            rnn_time / seeds as f64
        );
    }
    println!(
        "\nANF learned coefficient bits: {learned_bits} ({} bytes); state/input bits read per token: {bits_read}; \
         coefficient bits touched per token by direct ANF evaluation: {learned_bits}. \
         RNN f32 parameters: {rnn_params} ({} bytes). n-gram table: {} entries.",
        learned_bits / 8,
        rnn_params * 4,
        1usize << NGRAM_WINDOW
    );
    Ok(())
}
