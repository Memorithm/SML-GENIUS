//! Matched A5 dense-vs-sparse readout experiment on the same learned ANF state.
//! Usage: cargo run --release -p sml-core --bin sml-sparse-byte-bench -- <corpus> [seeds]
//! No test-set-driven hyperparameter search: the candidate is fixed in source.
use std::{error::Error, time::Instant};

use sml_core::{
    anf_char::{self, CharAnfConfig, CharSplits},
    anf_sparse_byte::SparseByteReadouts,
    char_baselines::NGramModel,
};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    let corpus = args
        .get(1)
        .ok_or("usage: sml-sparse-byte-bench <corpus> [seeds]")?;
    let seeds: u64 = args.get(2).map_or(Ok(3), |value| value.parse())?;
    if seeds == 0 || seeds > 20 {
        return Err("seeds must be between 1 and 20".into());
    }
    let raw = std::fs::read(corpus)?;
    let splits = CharSplits::new(&raw);
    let train = anf_char::pack_bytes(&splits.train)?;
    let validation = anf_char::pack_bytes(&splits.val)?;
    let test = anf_char::pack_bytes(&splits.test)?;
    println!("A5 sparse readout, fixed configuration; train/val/test bytes: {}/{}/{}; scored tokens: {}/{}/{}",
        splits.train.len(), splits.val.len(), splits.test.len(),
        train.tokens(), validation.tokens(), test.tokens());
    println!("Training and readout selection use train/validation only. Test is scored once per fixed seed.");
    println!("| model | seed | validation bits/byte | test bits/byte | test accuracy (%) | observed contexts | fit seconds |");
    println!("|---|---:|---:|---:|---:|---:|---:|");
    let baseline = NGramModel::train(&splits.train, 4);
    let bv = baseline.score(&splits.val);
    let bt = baseline.score(&splits.test);
    println!(
        "| Witten-Bell n-gram 4 | - | {:.5} | {:.5} | {:.3} | {} | - |",
        bv.bits_per_byte,
        bt.bits_per_byte,
        bt.accuracy * 100.0,
        baseline.parameters()
    );
    for seed in 0..seeds {
        let config = CharAnfConfig {
            seed,
            lags: 4,
            free_gates: 64,
            max_readout_fan_in: 20,
            context_bits: 16,
            ..CharAnfConfig::default()
        };
        let report = anf_char::train(config, &train, &validation)?;
        let dense_val = anf_char::evaluate(&report.model, &validation, false)?;
        let dense_test = anf_char::evaluate(&report.model, &test, true)?;
        println!(
            "| ANF dense KT | {seed} | {:.5} | {:.5} | {:.3} | {} | - |",
            dense_val.bits_per_byte,
            dense_test.bits_per_byte,
            dense_test.accuracy * 100.0,
            report.model.calibration_entries()
        );
        let clock = Instant::now();
        let sparse = SparseByteReadouts::fit(&report.model, &train)?;
        let fit_secs = clock.elapsed().as_secs_f64();
        let sparse_val = sparse.evaluate(&report.model, &validation, false)?;
        let sparse_test = sparse.evaluate(&report.model, &test, true)?;
        println!(
            "| ANF sparse backoff | {seed} | {:.5} | {:.5} | {:.3} | {} | {:.3} |",
            sparse_val.bits_per_byte,
            sparse_test.bits_per_byte,
            sparse_test.accuracy * 100.0,
            sparse.stored_contexts(),
            fit_secs
        );
        println!(
            "| sparse - dense delta | {seed} | {:+.5} | {:+.5} | {:+.3} | - | - |",
            sparse_val.bits_per_byte - dense_val.bits_per_byte,
            sparse_test.bits_per_byte - dense_test.bits_per_byte,
            (sparse_test.accuracy - dense_test.accuracy) * 100.0
        );
    }
    Ok(())
}
