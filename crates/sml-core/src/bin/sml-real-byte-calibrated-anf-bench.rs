//! A6-E2: preregistered TRAIN-learned ANF support with calibrated KT readout.
//! This tests REAL next-space log loss, not synthetic source recovery.
//! TEST scoring begins ONLY after all VAL model choices are frozen.

use std::{error::Error, time::Instant};

use sml_core::{
    anf_char::{CharSplits, ChunkLayout, LANES},
    anf_wide_sparse::{WideAnfGate, WideAnfLimits},
    char_baselines::NGramModel,
};

const EVENT: u8 = b' ';
const ALPHAS: [f64; 4] = [0.5, 1.0, 4.0, 16.0];
const LAMBDAS: [f64; 6] = [0.0, 0.05, 0.10, 0.25, 0.50, 1.0];
const REQUIRED_ADVANTAGE: f64 = 0.02;
const CORPUS_BYTES: usize = 433_036;
const EXPECTED_TEST_EVENTS: usize = 43_136;

#[derive(Clone, Copy)]
struct TrainRow {
    bits: u16,
    truth: bool,
}

#[derive(Clone, Copy)]
struct EvalRow {
    bits: u16,
    truth: bool,
    ngram2: f64,
    ngram4: f64,
    majority: f64,
}

struct Expert {
    support: Vec<usize>,
    counts: Vec<[u32; 2]>,
}

#[derive(Clone, Copy)]
struct Metrics {
    bits_per_event: f64,
    mistakes: usize,
}

fn bits16(previous: u8, previous_previous: u8) -> u16 {
    u16::from(previous) | (u16::from(previous_previous) << 8)
}

fn unpack(bits: u16) -> [bool; 16] {
    std::array::from_fn(|i| bits & (1u16 << i) != 0)
}

fn address(bits: u16, support: &[usize]) -> usize {
    support.iter().enumerate().fold(0usize, |acc, (index, &bit)| {
        acc | ((usize::from(bits >> bit) & 1) << index)
    })
}

fn training_rows(bytes: &[u8]) -> Result<Vec<TrainRow>, Box<dyn Error>> {
    let layout = ChunkLayout::new(bytes.len())?;
    let mut rows = Vec::new();
    rows.try_reserve_exact(LANES * (layout.chunk - 2))?;
    for lane in 0..LANES {
        let chunk = &bytes[lane * layout.chunk..(lane + 1) * layout.chunk];
        for t in 2..chunk.len() {
            rows.push(TrainRow {
                bits: bits16(chunk[t - 1], chunk[t - 2]),
                truth: chunk[t] == EVENT,
            });
        }
    }
    Ok(rows)
}

fn counts_for(rows: &[TrainRow], support: &[usize]) -> Vec<[u32; 2]> {
    let mut counts = vec![[0u32; 2]; 1 << support.len()];
    for row in rows {
        counts[address(row.bits, support)][usize::from(row.truth)] += 1;
    }
    counts
}

/// True TRAIN leave-one-out KT loss. No VAL or TEST label accesses.
fn loo_bits(rows: &[TrainRow], support: &[usize]) -> f64 {
    let counts = counts_for(rows, support);
    rows.iter()
        .map(|row| {
            let pair = counts[address(row.bits, support)];
            let denominator = f64::from(pair[0] + pair[1]);
            let numerator = f64::from(pair[usize::from(row.truth)]) - 0.5;
            -(numerator / denominator).log2()
        })
        .sum::<f64>() / rows.len() as f64
}

/// Same deterministic TRAIN-only greedy support selector as A6-E1.
fn greedy_supports(rows: &[TrainRow]) -> Vec<Vec<usize>> {
    let mut chosen = Vec::<usize>::new();
    let mut result = Vec::new();
    for k in 1..=5usize {
        let mut best = (f64::INFINITY, usize::MAX);
        for candidate in 0..16usize {
            if chosen.contains(&candidate) {
                continue;
            }
            let mut trial = chosen.clone();
            trial.push(candidate);
            let loss = loo_bits(rows, &trial);
            if loss < best.0 - 1.0e-12 || ((loss - best.0).abs() <= 1.0e-12 && candidate < best.1) {
                best = (loss, candidate);
            }
        }
        assert!(best.1 < 16);
        chosen.push(best.1);
        if k >= 2 {
            let mut support = chosen.clone();
            support.sort_unstable();
            result.push(support);
        }
    }
    result
}

/// Reconstruct the native ANF from the trained table. This independent
/// Boolean oracle verifies the exact underlying gate; probability prediction
/// deliberately uses the *same learned counts*, not overconfident hard voting.
fn verify_native_anf(support: &[usize], counts: &[[u32; 2]]) -> Result<usize, Box<dyn Error>> {
    let k = support.len();
    let table = counts.iter().map(|pair| pair[1] > pair[0]).collect::<Vec<_>>();
    if table.len() != 1usize << k {
        return Err("invalid ANF table length".into());
    }
    let mut coefficients = table.clone();
    for bit in 0..k {
        for mask in 0..(1 << k) {
            if mask & (1 << bit) != 0 {
                coefficients[mask] ^= coefficients[mask ^ (1 << bit)];
            }
        }
    }
    let monomials = (0..(1 << k))
        .filter(|&mask| coefficients[mask])
        .map(|mask| (0..k).filter(|&bit| mask & (1 << bit) != 0).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let gate = WideAnfGate::from_monomials(
        support.to_vec(),
        monomials,
        16,
        WideAnfLimits::new(16, 256, 8192, 8192, 8192),
    )?;
    for (mask, &expected) in table.iter().enumerate() {
        let mut bits = [false; 16];
        for (local, &physical) in support.iter().enumerate() {
            bits[physical] = mask & (1 << local) != 0;
        }
        if gate.evaluate_scalar(&bits)? != expected {
            return Err("native GF(2) ANF differs from majority truth table".into());
        }
    }
    Ok(gate.active_monomials())
}

fn train_experts(rows: &[TrainRow]) -> Result<Vec<Expert>, Box<dyn Error>> {
    let mut experts = Vec::new();
    for support in greedy_supports(rows) {
        let counts = counts_for(rows, &support);
        let active = verify_native_anf(&support, &counts)?;
        println!(
            "A6_E2_LEARNED,fanin={},support={:?},monomials={},count_cells={},count_payload_bytes={},train_loo_bits={:.8}",
            support.len(), support, active, counts.len(), counts.len() * 2 * std::mem::size_of::<u32>(),
            loo_bits(rows, &support)
        );
        experts.push(Expert { support, counts });
    }
    if experts.len() != 4 {
        return Err("exactly 4 trained ANF experts required".into());
    }
    Ok(experts)
}

fn kt_probability(expert: &Expert, bits: u16, alpha: f64) -> f64 {
    let [neg, pos] = expert.counts[address(bits, &expert.support)];
    (f64::from(pos) + alpha) / (f64::from(pos + neg) + 2.0 * alpha)
}

fn mixture(kt: f64, ngram4: f64, lambda: f64) -> f64 {
    lambda * kt + (1.0 - lambda) * ngram4
}

fn clip_probability(value: f64) -> f64 {
    value.clamp(1.0e-6, 1.0 - 1.0e-6)
}

fn bit_loss(probability: f64, truth: bool) -> f64 {
    let p = clip_probability(probability);
    if truth { -p.log2() } else { -(1.0 - p).log2() }
}

fn score_rows(
    bytes: &[u8],
    ngram2: &NGramModel,
    ngram4: &NGramModel,
    majority: f64,
) -> Result<Vec<EvalRow>, Box<dyn Error>> {
    let layout = ChunkLayout::new(bytes.len())?;
    let mut rows = Vec::new();
    rows.try_reserve_exact(LANES * (layout.chunk - 2))?;
    let mut probs2 = [0.0f64; 256];
    let mut probs4 = [0.0f64; 256];
    for lane in 0..LANES {
        let chunk = &bytes[lane * layout.chunk..(lane + 1) * layout.chunk];
        for t in 2..chunk.len() {
            ngram2.distribution(&chunk[..t], &mut probs2);
            ngram4.distribution(&chunk[..t], &mut probs4);
            rows.push(EvalRow {
                bits: bits16(chunk[t - 1], chunk[t - 2]),
                truth: chunk[t] == EVENT,
                ngram2: probs2[usize::from(EVENT)],
                ngram4: probs4[usize::from(EVENT)],
                majority,
            });
        }
    }
    Ok(rows)
}

fn evaluate(rows: &[EvalRow], expert: Option<(&Expert, f64, f64)>, baseline: &str) -> Metrics {
    let mut errors = 0usize;
    let mut loss = 0.0f64;
    for row in rows {
        let p = match expert {
            Some((model, alpha, lambda)) => mixture(kt_probability(model, row.bits, alpha), row.ngram4, lambda),
            None => match baseline {
                "ngram2" => row.ngram2,
                "ngram4" => row.ngram4,
                "majority" => row.majority,
                _ => unreachable!("unregistered baseline"),
            }
        };
        errors += usize::from((p >= 0.5) != row.truth);
        loss += bit_loss(p, row.truth);
    }
    Metrics { bits_per_event: loss / rows.len() as f64, mistakes: errors }
}

fn candidates() -> [(usize, usize); 16] {
    std::array::from_fn(|index| (index / ALPHAS.len(), index % ALPHAS.len()))
}

fn run(path: &str) -> Result<(), Box<dyn Error>> {
    let raw = std::fs::read(path)?;
    if raw.len() != CORPUS_BYTES {
        return Err("incorrect frozen Project Gutenberg corpus size".into());
    }
    let splits = CharSplits::new(&raw);
    let training_started = Instant::now();
    let train = training_rows(&splits.train)?;
    let positives = train.iter().filter(|row| row.truth).count();
    let majority = (positives as f64 + 0.5) / (train.len() as f64 + 1.0);
    let experts = train_experts(&train)?;
    let ngram2 = NGramModel::train(&splits.train, 2);
    let ngram4 = NGramModel::train(&splits.train, 4);
    println!(
        "A6_E2_TRAIN,events={},positive={},experts={},ngram2_parameters={},ngram4_parameters={},count_payload_bytes={},train_seconds={:.3}",
        train.len(), positives, experts.len(), ngram2.parameters(), ngram4.parameters(),
        experts.iter().map(|e| e.counts.len() * 2 * std::mem::size_of::<u32>()).sum::<usize>(),
        training_started.elapsed().as_secs_f64()
    );

    let validation_started = Instant::now();
    let validation = score_rows(&splits.val, &ngram2, &ngram4, majority)?;
    let val_ngram2 = evaluate(&validation, None, "ngram2");
    let val_ngram4 = evaluate(&validation, None, "ngram4");
    let val_majority = evaluate(&validation, None, "majority");
    println!(
        "A6_E2_VAL_BASELINES,events={},ngram2_bits={:.8},ngram4_bits={:.8},majority_bits={:.8}",
        validation.len(), val_ngram2.bits_per_event, val_ngram4.bits_per_event, val_majority.bits_per_event
    );

    let grid = candidates();
    let mut best_pure: Option<(usize, f64)> = None;
    let mut best_mix: Option<(usize, usize, f64)> = None;
    for (index, &(expert_id, alpha_id)) in grid.iter().enumerate() {
        let model = &experts[expert_id];
        let alpha = ALPHAS[alpha_id];
        let pure = evaluate(&validation, Some((model, alpha, 1.0)), "");
        println!(
            "A6_E2_VAL_PURE,index={},fanin={},alpha={:.2},events={},bits={:.8},errors={}",
            index, model.support.len(), alpha, validation.len(), pure.bits_per_event, pure.mistakes
        );
        if best_pure.is_none_or(|(_, current)| pure.bits_per_event < current - 1.0e-12) {
            best_pure = Some((index, pure.bits_per_event));
        }
        for (lambda_id, &lambda) in LAMBDAS.iter().enumerate() {
            let mixed = evaluate(&validation, Some((model, alpha, lambda)), "");
            println!(
                "A6_E2_VAL_MIX,index={},lambda_index={},fanin={},alpha={:.2},lambda={:.2},events={},bits={:.8},errors={}",
                index, lambda_id, model.support.len(), alpha, lambda, validation.len(), mixed.bits_per_event, mixed.mistakes
            );
            if best_mix.is_none_or(|(_, _, current)| mixed.bits_per_event < current - 1.0e-12) {
                best_mix = Some((index, lambda_id, mixed.bits_per_event));
            }
        }
    }
    let (pure_index, pure_val_bits) = best_pure.ok_or("no KT expert selected")?;
    let (mix_index, lambda_id, mix_val_bits) = best_mix.ok_or("no mixture selected")?;
    println!(
        "A6_E2_ALL_FROZEN,val_events={},pure_candidates=16,mixed_policies=96,selected_pure={},selected_mix={},selected_lambda_index={},selected_lambda={:.2},pure_val_bits={:.8},mix_val_bits={:.8},val_seconds={:.3}",
        validation.len(), pure_index, mix_index, lambda_id, LAMBDAS[lambda_id],
        pure_val_bits, mix_val_bits, validation_started.elapsed().as_secs_f64()
    );

    // Nothing below this line may change supports, alphas, mixtures or baselines.
    let test_started = Instant::now();
    let heldout = score_rows(&splits.test, &ngram2, &ngram4, majority)?;
    if heldout.len() != EXPECTED_TEST_EVENTS {
        return Err("unexpected TEST event count".into());
    }
    let [ng2, ng4, majority_test] = [
        evaluate(&heldout, None, "ngram2"),
        evaluate(&heldout, None, "ngram4"),
        evaluate(&heldout, None, "majority"),
    ];
    let (pure_id, pure_alpha_id) = grid[pure_index];
    let (mix_id, mix_alpha_id) = grid[mix_index];
    let pure = evaluate(&heldout, Some((&experts[pure_id], ALPHAS[pure_alpha_id], 1.0)), "");
    let mixed = evaluate(&heldout, Some((&experts[mix_id], ALPHAS[mix_alpha_id], LAMBDAS[lambda_id])), "");
    println!(
        "A6_E2_TEST_BASELINES,events={},ngram2_bits={:.8},ngram2_errors={},ngram4_bits={:.8},ngram4_errors={},majority_bits={:.8},majority_errors={}",
        heldout.len(), ng2.bits_per_event, ng2.mistakes, ng4.bits_per_event, ng4.mistakes,
        majority_test.bits_per_event, majority_test.mistakes
    );
    println!(
        "A6_E2_TEST_SELECTED,events={},pure_index={},pure_bits={:.8},pure_errors={},mix_index={},lambda_index={},lambda={:.2},mix_bits={:.8},mix_errors={},test_seconds={:.3}",
        heldout.len(), pure_index, pure.bits_per_event, pure.mistakes, mix_index, lambda_id,
        LAMBDAS[lambda_id], mixed.bits_per_event, mixed.mistakes,
        test_started.elapsed().as_secs_f64()
    );
    let decision = if pure.bits_per_event <= ng4.bits_per_event - REQUIRED_ADVANTAGE
        && mixed.bits_per_event <= ng4.bits_per_event - REQUIRED_ADVANTAGE
    {
        "RESEARCH_PASS"
    } else {
        "STOP"
    };
    println!(
        "A6_E2_COMPLETE,real_corpus=true,task=next_space_binary,val_pure=16,val_mix=96,test_events={},test_pure=1,test_mix=1,ngram4_bits={:.8},pure_bits={:.8},mix_bits={:.8},improvement_pure={:.8},improvement_mix={:.8},decision={},oracle=PASS",
        heldout.len(), ng4.bits_per_event, pure.bits_per_event, mixed.bits_per_event,
        ng4.bits_per_event - pure.bits_per_event, ng4.bits_per_event - mixed.bits_per_event, decision
    );
    Ok(())
}

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: sml-real-byte-calibrated-anf-bench <SHA256-verified-verne-body>");
        std::process::exit(2);
    };
    if let Err(error) = run(&path) {
        eprintln!("A6_E2_FAILURE,{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kt_posteriors_and_ngram_only_control_are_normalized() {
        let model = Expert {
            support: vec![0, 4],
            counts: vec![[0, 0], [1, 0], [0, 4], [30, 10]],
        };
        assert_eq!(candidates().len(), 16);
        assert_eq!(ALPHAS.len() * LAMBDAS.len() * 4, 96);
        for alpha in ALPHAS {
            for bits in [0u16, 1, 16, 17] {
                let p = kt_probability(&model, bits, alpha);
                assert!(p > 0.0 && p < 1.0 && p.is_finite());
                assert_eq!(mixture(p, 0.123456, 0.0), 0.123456);
            }
        }
        assert!((kt_probability(&model, 0, 0.5) - 0.5).abs() < 1.0e-12);
    }

    #[test]
    fn native_anf_matches_independent_majority_truth_table() {
        let support = [0usize, 2, 4];
        let mut counts = vec![[0u32; 2]; 8];
        for (mask, slot) in counts.iter_mut().enumerate() {
            slot[usize::from(mask.count_ones() > 1)] = 4;
        }
        assert!(verify_native_anf(&support, &counts).is_ok());
        let train = (0..32)
            .map(|k| TrainRow { bits: (k % 8) as u16, truth: k % 2 == 0 })
            .collect::<Vec<_>>();
        assert_eq!(greedy_supports(&train).len(), 4);
    }

    #[test]
    fn calibration_count_and_test_denominator_are_fixed() {
        assert_eq!(CORPUS_BYTES, 433_036);
        assert_eq!(EXPECTED_TEST_EVENTS, 43_136);
        assert_eq!(REQUIRED_ADVANTAGE, 0.02);
        let row = EvalRow { bits: 0, truth: true, ngram2: 0.4, ngram4: 0.7, majority: 0.6 };
        let a = evaluate(&[row], None, "ngram4");
        let one = Expert { support: vec![0], counts: vec![[7, 3], [7, 3]] };
        let b = evaluate(&[row], Some((&one, 1.0, 0.0)), "");
        assert!((a.bits_per_event - b.bits_per_event).abs() < 1.0e-12);
        assert_eq!(a.mistakes, b.mistakes);
    }
}
