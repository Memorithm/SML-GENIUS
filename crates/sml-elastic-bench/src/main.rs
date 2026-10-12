//! Runs the EL-NG1 protocol on a corpus file and prints the report.
//!
//! Usage: `sml-elastic-ngram-bench <corpus-file> [--allow-other-corpus]`.
//! The default refuses any file that is not exactly the frozen Candide body
//! size; `--allow-other-corpus` is for smoke runs and is flagged in the report.

use std::process::ExitCode;

use sml_elastic_bench::{frozen_splits, render, run, Params, FROZEN_CORPUS_BYTES};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let allow_other = args.iter().any(|a| a == "--allow-other-corpus");
    let Some(path) = args.iter().find(|a| !a.starts_with("--")) else {
        eprintln!("usage: sml-elastic-ngram-bench <corpus-file> [--allow-other-corpus]");
        return ExitCode::from(2);
    };
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("cannot read {path}: {error}");
            return ExitCode::from(2);
        }
    };
    let frozen = bytes.len() == FROZEN_CORPUS_BYTES;
    if !frozen && !allow_other {
        eprintln!(
            "refusing {} bytes: the frozen corpus has {FROZEN_CORPUS_BYTES} (use scripts/fetch-candide-corpus.sh)",
            bytes.len()
        );
        return ExitCode::from(2);
    }
    let (train, val) = frozen_splits(&bytes);
    match run(&train, &val, &Params::frozen()) {
        Ok(report) => {
            print!("{}", render(&report, frozen));
            if report.all_passed() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            eprintln!("EL-NG1 run failed: {error}");
            ExitCode::FAILURE
        }
    }
}
