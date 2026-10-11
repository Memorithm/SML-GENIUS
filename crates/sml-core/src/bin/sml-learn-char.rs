//! A5 step 3, part 2: byte-level next-byte prediction, local ANF vs baselines.
//!
//! Usage: `sml-learn-char <corpus-path> [anf-seeds] [rnn-epochs] [rnn-seeds]`
//! (defaults 3, 6, 1). Fetch the corpus with `scripts/fetch-char-corpus.sh`.
//! Prints a markdown table; per-run details go to stderr.
//!
//! Splits: contiguous 80/10/10 % of the corpus; every model is scored on the
//! same tokens (64 contiguous chunks per split, context reset at chunk starts).

use std::time::Instant;

use sml_core::anf_char::{self, CharAnfConfig, CharSplits};
use sml_core::char_baselines::{ByteRnn, ByteRnnConfig, NGramModel};

fn mean_std(values: &[f64]) -> String {
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    if values.len() < 2 {
        return format!("{mean:.3}");
    }
    let var = values.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / (n - 1.0);
    format!("{mean:.3} ± {:.3}", var.sqrt())
}

fn pct(values: &[f64]) -> String {
    let scaled: Vec<f64> = values.iter().map(|v| v * 100.0).collect();
    let n = scaled.len() as f64;
    let mean = scaled.iter().sum::<f64>() / n;
    if scaled.len() < 2 {
        return format!("{mean:.1}");
    }
    let var = scaled.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / (n - 1.0);
    format!("{mean:.1} ± {:.1}", var.sqrt())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let path = args
        .get(1)
        .ok_or("usage: sml-learn-char <corpus-path> [anf-seeds] [rnn-epochs] [rnn-seeds]")?;
    let anf_seeds: u64 = args.get(2).map_or(Ok(3), |s| s.parse())?;
    let rnn_epochs: usize = args.get(3).map_or(Ok(6), |s| s.parse())?;
    let rnn_seeds: u64 = args.get(4).map_or(Ok(1), |s| s.parse())?;
    let bytes = std::fs::read(path)?;
    let splits = CharSplits::new(&bytes);
    let train = anf_char::pack_bytes(&splits.train)?;
    let val = anf_char::pack_bytes(&splits.val)?;
    let test = anf_char::pack_bytes(&splits.test)?;
    println!(
        "Corpus: {} bytes; splits train/val/test {}/{}/{} bytes; scored tokens train/val/test {}/{}/{} (64 contiguous chunks, context reset per chunk)",
        bytes.len(),
        splits.train.len(),
        splits.val.len(),
        splits.test.len(),
        train.tokens(),
        val.tokens(),
        test.tokens()
    );
    println!();
    println!("| Model | seeds | val bits/byte | test bits/byte | test accuracy (%) | parameters | bits read per token | train time (s) | inference tokens/s |");
    println!("|---|---|---|---|---|---|---|---|---|");
    for order in 0..=4 {
        let clock = Instant::now();
        let model = NGramModel::train(&splits.train, order);
        let train_secs = clock.elapsed().as_secs_f64();
        let v = model.score(&splits.val);
        let clock = Instant::now();
        let t = model.score(&splits.test);
        let rate = t.tokens as f64 / clock.elapsed().as_secs_f64();
        let name = if order == 0 {
            "unigram (Witten-Bell to uniform)".to_string()
        } else {
            format!("n-gram, context {order} byte(s), Witten-Bell")
        };
        println!(
            "| {name} | - | {:.3} | {:.3} | {:.1} | {} stored counts | {} byte(s) of context | {train_secs:.2} | {rate:.0} |",
            v.bits_per_byte,
            t.bits_per_byte,
            t.accuracy * 100.0,
            model.parameters(),
            order
        );
    }
    {
        let (mut vb, mut tb, mut acc, mut secs, mut rate) = (vec![], vec![], vec![], 0.0, 0.0);
        let mut params = 0;
        let config = ByteRnnConfig {
            epochs: rnn_epochs,
            ..ByteRnnConfig::default()
        };
        for seed in 0..rnn_seeds {
            let clock = Instant::now();
            let (model, history) =
                ByteRnn::train(ByteRnnConfig { seed, ..config }, &splits.train, &splits.val);
            secs += clock.elapsed().as_secs_f64();
            let v = model.score(&splits.val);
            let clock = Instant::now();
            let t = model.score(&splits.test);
            rate += t.tokens as f64 / clock.elapsed().as_secs_f64();
            eprintln!("rnn seed={seed}: val history {history:?} test {t:?}");
            vb.push(v.bits_per_byte);
            tb.push(t.bits_per_byte);
            acc.push(t.accuracy);
            params = model.parameters();
        }
        let n = rnn_seeds as f64;
        println!(
            "| Elman RNN H={} (f32, BPTT {} steps, Adam, {} epochs) | {rnn_seeds} | {} | {} | {} | {params} f32 | {} f32 MACs | {:.1} | {:.0} |",
            config.hidden,
            config.window,
            rnn_epochs,
            mean_std(&vb),
            mean_std(&tb),
            pct(&acc),
            config.hidden * config.hidden + 256 * config.hidden,
            secs / n,
            rate / n
        );
    }
    let base = CharAnfConfig::default();
    let variants: Vec<(String, CharAnfConfig)> = vec![
        (
            "ANF, no history layer (L=0, F=64), readout selection + search".into(),
            CharAnfConfig { lags: 0, ..base },
        ),
        (
            "ANF, no history layer (L=0, F=64), readout selection only".into(),
            CharAnfConfig {
                lags: 0,
                max_epochs: 0,
                ..base
            },
        ),
        (
            "ANF, history L=2 (F=64), readout selection only".into(),
            CharAnfConfig {
                lags: 2,
                max_epochs: 0,
                ..base
            },
        ),
        (
            "ANF, history L=4 (F=64), readout selection only".into(),
            CharAnfConfig {
                max_epochs: 0,
                ..base
            },
        ),
        (
            "ANF, history L=4 (F=64), readout selection + search".into(),
            base,
        ),
        (
            "ANF, history L=4 (F=64), readout fan-in <= 20, readout selection + search".into(),
            CharAnfConfig {
                max_readout_fan_in: 20,
                context_bits: 16,
                ..base
            },
        ),
        (
            "ANF, history L=4 (F=992, N=1024), readout selection + search".into(),
            CharAnfConfig {
                free_gates: 992,
                ..base
            },
        ),
    ];
    for (name, config) in variants {
        let (mut vb, mut tb, mut acc, mut secs, mut rate) = (vec![], vec![], vec![], 0.0, 0.0);
        let mut size = (0, 0, 0);
        for seed in 0..anf_seeds {
            let clock = Instant::now();
            let report = anf_char::train(CharAnfConfig { seed, ..config }, &train, &val)?;
            secs += clock.elapsed().as_secs_f64();
            let clock = Instant::now();
            let t = anf_char::evaluate(&report.model, &test, true)?;
            rate += t.tokens as f64 / clock.elapsed().as_secs_f64();
            let model = &report.model;
            eprintln!(
                "{name} seed={seed}: history_errors={} epochs={} evals={} accepted={} val={:.4} test={:.4} acc={:.4} times(hist,readout,search)={:.2?} readouts={:?}",
                report.history_errors,
                report.epochs,
                report.evaluations,
                report.accepted,
                report.val_bits_per_byte,
                t.bits_per_byte,
                t.accuracy,
                report.seconds,
                model.readouts.iter().map(|r| r.inputs.len()).collect::<Vec<_>>()
            );
            vb.push(report.val_bits_per_byte);
            tb.push(t.bits_per_byte);
            acc.push(t.accuracy);
            size = (
                model.learned_bits(),
                model.calibration_entries(),
                model.bits_read_per_token(),
            );
        }
        let n = anf_seeds as f64;
        println!(
            "| {name} | {anf_seeds} | {} | {} | {} | {} learned bits + {} count entries | {} bits | {:.1} | {:.0} |",
            mean_std(&vb),
            mean_std(&tb),
            pct(&acc),
            size.0,
            size.1,
            size.2,
            secs / n,
            rate / n
        );
    }
    Ok(())
}
