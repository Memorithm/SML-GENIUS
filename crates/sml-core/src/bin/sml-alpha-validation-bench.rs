//! A5-5: preregistered validation-only search of sparse backoff alpha.
//!
//! The same trained ANF network, wiring and sparse counts are shared between
//! candidate alpha values. All seeds select alpha before ANY test scoring.
//!
//! Usage: sml-alpha-validation-bench <pinned-corpus-path> [seeds:1..10]
//! The baseline alpha=4 and Witten-Bell n-gram-4 are fixed, not tuned on test.
use std::{error::Error, time::Instant};

use sml_core::{
    anf_char::{self, CharAnfConfig, CharAnfModel, CharSplits},
    anf_sparse_byte::SparseByteReadouts,
    char_baselines::NGramModel,
};

const ALPHAS: [f64; 7] = [0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0];
const REFERENCE: f64 = 4.0;
const EPSILON: f64 = 1e-12;

fn better_alpha(candidate: f64, candidate_loss: f64, best: f64, best_loss: f64) -> bool {
    if !best_loss.is_finite() {
        return candidate_loss.is_finite();
    }
    if candidate_loss < best_loss - EPSILON {
        return true;
    }
    if (candidate_loss - best_loss).abs() > EPSILON {
        return false;
    }
    let candidate_distance = (candidate / REFERENCE).log2().abs();
    let best_distance = (best / REFERENCE).log2().abs();
    candidate_distance < best_distance || (candidate_distance == best_distance && candidate < best)
}

struct Selected {
    seed: u64,
    model: CharAnfModel,
    sparse: SparseByteReadouts,
    alpha: f64,
    selected_validation_bits: f64,
    reference_validation_bits: f64,
    train_seconds: f64,
    sparse_fit_seconds: f64,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args.len() > 3 {
        return Err("usage: sml-alpha-validation-bench <corpus> [seeds:1..10]".into());
    }
    let seeds: u64 = args.get(2).map_or(Ok(10), |value| value.parse())?;
    if !(1..=10).contains(&seeds) {
        return Err("seeds must be in 1..=10".into());
    }
    let bytes = std::fs::read(&args[1])?;
    let split = CharSplits::new(&bytes);
    let train = anf_char::pack_bytes(&split.train)?;
    let validation = anf_char::pack_bytes(&split.val)?;
    let test = anf_char::pack_bytes(&split.test)?;

    println!(
        "A5_ALPHA_SETUP,corpus_bytes={},train_tokens={},validation_tokens={},test_tokens={},seeds={seeds}",
        bytes.len(),
        train.tokens(),
        validation.tokens(),
        test.tokens()
    );
    println!(
        "A5_ALPHA_PROTOCOL,train_only_counts=true,validation_only_alpha_selection=true,test_after_all_selections=true,alpha_reference=4,selection_metric=bits_per_byte,shared_prior_test_exposure=true"
    );
    println!("A5_ALPHA_CANDIDATES,0.5,1,2,4,8,16,32");
    println!("A5_ALPHA_VALIDATION,seed,alpha,bits_per_byte");

    // No held-out test prediction is computed until ALL selections are frozen.
    let mut selected_runs = Vec::new();
    for seed in 0..seeds {
        let config = CharAnfConfig {
            seed,
            lags: 4,
            free_gates: 64,
            max_readout_fan_in: 20,
            context_bits: 16,
            ..CharAnfConfig::default()
        };
        let start = Instant::now();
        let report = anf_char::train(config, &train, &validation)?;
        let train_seconds = start.elapsed().as_secs_f64();
        let start = Instant::now();
        let sparse = SparseByteReadouts::fit(&report.model, &train)?;
        let sparse_fit_seconds = start.elapsed().as_secs_f64();

        let mut best = REFERENCE;
        let mut best_loss = f64::INFINITY;
        let mut reference_validation_bits = f64::NAN;
        for alpha in ALPHAS {
            let scored = sparse.evaluate_with_alpha(&report.model, &validation, false, alpha)?;
            if scored.tokens != validation.tokens() || !scored.bits_per_byte.is_finite() {
                return Err("validation scoring invalid".into());
            }
            let loss = scored.bits_per_byte;
            println!("A5_ALPHA_VALIDATION_RESULT,{seed},{alpha:.1},{loss:.8}");
            if alpha == REFERENCE {
                reference_validation_bits = loss;
            }
            if better_alpha(alpha, loss, best, best_loss) {
                best = alpha;
                best_loss = loss;
            }
        }
        if !best_loss.is_finite() || !reference_validation_bits.is_finite() {
            return Err("non-finite validation loss".into());
        }
        let default = sparse.evaluate(&report.model, &validation, false)?;
        let explicit = sparse.evaluate_with_alpha(&report.model, &validation, false, REFERENCE)?;
        if default.bits_per_byte.to_bits() != explicit.bits_per_byte.to_bits() {
            return Err("alpha=4 regression failed".into());
        }
        println!(
            "A5_ALPHA_FROZEN,seed={seed},alpha={best:.1},validation_bits={best_loss:.8},reference_validation_bits={reference_validation_bits:.8},gain_validation_bits={:.8},context_entries={},train_seconds={train_seconds:.2},sparse_fit_seconds={sparse_fit_seconds:.2}",
            reference_validation_bits - best_loss,
            sparse.stored_contexts()
        );
        selected_runs.push(Selected {
            seed,
            model: report.model,
            sparse,
            alpha: best,
            selected_validation_bits: best_loss,
            reference_validation_bits,
            train_seconds,
            sparse_fit_seconds,
        });
    }

    println!("A5_ALPHA_ALL_FROZEN,true");
    println!("A5_ALPHA_TEST,seed,alpha,selected_validation_bits,reference_validation_bits,selected_test_bits,alpha4_test_bits,selected_accuracy_pct,alpha4_accuracy_pct,dense_test_bits,context_entries,train_seconds,sparse_fit_seconds");
    let mut total_selected = 0.0;
    let mut total_reference = 0.0;
    let mut total_accuracy_change = 0.0;
    let mut improved_test_seeds = 0usize;
    for run in &selected_runs {
        let candidate = run
            .sparse
            .evaluate_with_alpha(&run.model, &test, true, run.alpha)?;
        let reference = if run.alpha == REFERENCE {
            candidate
        } else {
            run.sparse.evaluate(&run.model, &test, true)?
        };
        let dense = anf_char::evaluate(&run.model, &test, true)?;
        if candidate.tokens != test.tokens() || reference.tokens != test.tokens() {
            return Err("test token alignment failed".into());
        }
        let improvement = reference.bits_per_byte - candidate.bits_per_byte;
        if improvement > 0.0 {
            improved_test_seeds += 1;
        }
        total_selected += candidate.bits_per_byte;
        total_reference += reference.bits_per_byte;
        total_accuracy_change += 100.0 * (candidate.accuracy - reference.accuracy);
        println!(
            "A5_ALPHA_TEST_RESULT,{},{:.1},{:.8},{:.8},{:.8},{:.8},{:.5},{:.5},{:.8},{},{:.2},{:.2}",
            run.seed,
            run.alpha,
            run.selected_validation_bits,
            run.reference_validation_bits,
            candidate.bits_per_byte,
            reference.bits_per_byte,
            100.0 * candidate.accuracy,
            100.0 * reference.accuracy,
            dense.bits_per_byte,
            run.sparse.stored_contexts(),
            run.train_seconds,
            run.sparse_fit_seconds
        );
    }

    // Fixed n-gram control, evaluated only after the alpha choices were frozen.
    let ngram = NGramModel::train(&split.train, 4);
    let ngram_test = ngram.score(&split.test);
    println!(
        "A5_ALPHA_BASELINE,ngram4_test_bits={:.8},ngram4_test_accuracy_pct={:.5},ngram4_parameters={}",
        ngram_test.bits_per_byte,
        100.0 * ngram_test.accuracy,
        ngram.parameters()
    );
    println!(
        "A5_ALPHA_SUMMARY,seeds={seeds},selected_mean_test_bits={:.8},alpha4_mean_test_bits={:.8},mean_gain_bits={:.8},mean_accuracy_change_pp={:.5},selected_better_test_seeds={improved_test_seeds}",
        total_selected / seeds as f64,
        total_reference / seeds as f64,
        (total_reference - total_selected) / seeds as f64,
        total_accuracy_change / seeds as f64
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_tie_prefers_reference_without_looking_at_test() {
        assert!(better_alpha(4.0, 2.0, 2.0, f64::INFINITY));
        assert!(better_alpha(4.0, 2.0, 2.0, 2.0));
        assert!(!better_alpha(8.0, 2.0, 4.0, 2.0));
        assert!(!better_alpha(1.0, 2.01, 4.0, 2.0));
        assert!(better_alpha(2.0, 1.99, 4.0, 2.0));
    }

    #[test]
    fn frozen_candidates_are_unique_and_contain_reference() {
        assert_eq!(ALPHAS, [0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0]);
        assert_eq!(
            ALPHAS.iter().filter(|&&alpha| alpha == REFERENCE).count(),
            1
        );
        assert!(ALPHAS.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
