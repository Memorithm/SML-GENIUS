//! A6-E1: real French UTF-8 next-space prediction, not a synthetic teacher.
//! Four ANF gates are *trained* on the frozen TRAIN split, never supplied
//! from a known teacher. Guards and the policy are chosen without TEST labels.

use sml_core::{
    anf_char::{CharSplits, ChunkLayout, LANES},
    anf_passive_committee::PassiveCommittee,
    anf_scoped_risk::{competing_support, DegreeDesignRank, Hamming16Guard, ScopedPatternGuard},
    anf_wide_sparse::{WideAnfGate, WideAnfLimits},
    char_baselines::NGramModel,
};
use std::error::Error;
use std::time::Instant;

const EVENT: u8 = b' ';
const CONFIDENCES: [f64; 2] = [0.5, 0.9];
const STRENGTHS: [usize; 2] = [2, 3];
const MISSING_LIMITS: [usize; 2] = [0, 1];
const HAMMING_LIMITS: [u8; 4] = [0, 1, 2, 4];
const REQUIRED_COVERAGE: f64 = 0.80;
const REQUIRED_LOGLOSS_ADVANTAGE: f64 = 0.02;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TrainingRow {
    bits: u16,
    truth: bool,
}

#[derive(Clone, Copy, Debug)]
struct EvalRow {
    truth: bool,
    anf_probability: f64,
    anf_decision: bool,
    confidence: f64,
    ngram2_probability: f64,
    ngram4_probability: f64,
    majority_probability: f64,
    missing2: usize,
    missing3: usize,
    distance16: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Policy {
    confidence: f64,
    strength: usize,
    missing_limit: usize,
    hamming_limit: u8,
}

#[derive(Clone, Copy, Debug, Default)]
struct Metrics {
    total: usize,
    accepted: usize,
    accepted_errors: usize,
    accepted_logloss: f64,
    full_fallback_logloss: f64,
    full_fallback_errors: usize,
}

impl Metrics {
    fn coverage(self) -> f64 {
        self.accepted as f64 / self.total as f64
    }

    fn risk(self) -> Option<f64> {
        (self.accepted > 0).then(|| self.accepted_errors as f64 / self.accepted as f64)
    }

    fn fallback_bits(self) -> f64 {
        self.full_fallback_logloss / self.total as f64
    }
}

fn bits16(previous: u8, previous_previous: u8) -> u16 {
    u16::from(previous) | (u16::from(previous_previous) << 8)
}

fn unpack(bits: u16) -> [bool; 16] {
    std::array::from_fn(|bit| bits & (1u16 << bit) != 0)
}

fn address(bits: u16, support: &[usize]) -> usize {
    support.iter().enumerate().fold(0usize, |value, (i, &bit)| {
        value | ((usize::from(bits >> bit) & 1) << i)
    })
}

fn collect_rows(bytes: &[u8]) -> Result<Vec<TrainingRow>, Box<dyn Error>> {
    let layout = ChunkLayout::new(bytes.len())?;
    let mut rows = Vec::new();
    rows.try_reserve_exact(LANES * (layout.chunk - 2))?;
    for lane in 0..LANES {
        let chunk = &bytes[lane * layout.chunk..(lane + 1) * layout.chunk];
        for t in 2..chunk.len() {
            rows.push(TrainingRow {
                bits: bits16(chunk[t - 1], chunk[t - 2]),
                truth: chunk[t] == EVENT,
            });
        }
    }
    Ok(rows)
}

fn counts_for(rows: &[TrainingRow], support: &[usize]) -> Vec<[u32; 2]> {
    let mut counts = vec![[0u32; 2]; 1 << support.len()];
    for row in rows {
        counts[address(row.bits, support)][usize::from(row.truth)] += 1;
    }
    counts
}

/// Exact leave-one-out KT probability: (n_y - 1 + .5) / ((n-1)+1).
fn loo_bits(rows: &[TrainingRow], support: &[usize]) -> f64 {
    let counts = counts_for(rows, support);
    rows.iter()
        .map(|row| {
            let c = counts[address(row.bits, support)];
            let denominator = f64::from(c[0] + c[1]);
            let numerator = f64::from(c[usize::from(row.truth)]) - 0.5;
            -(numerator / denominator).log2()
        })
        .sum::<f64>()
        / rows.len() as f64
}

/// Train only: greedy physical-bit subset selection. k2..k5 are nested,
/// so the live competing-candidate union is bounded by the learned k5 set.
fn greedy_supports(rows: &[TrainingRow]) -> Vec<Vec<usize>> {
    let mut chosen = Vec::<usize>::new();
    let mut out = Vec::new();
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
            let mut canonical = chosen.clone();
            canonical.sort_unstable();
            out.push(canonical);
        }
    }
    out
}

/// Convert a TRAIN majority table to a genuine GF(2) ANF by Möbius inversion.
fn gate_from_counts(support: &[usize], counts: &[[u32; 2]]) -> Result<WideAnfGate, Box<dyn Error>> {
    let k = support.len();
    if counts.len() != 1usize << k {
        return Err("count table does not match selected support".into());
    }
    let mut coefficients = counts.iter().map(|c| c[1] > c[0]).collect::<Vec<_>>();
    let truth = coefficients.clone();
    for bit in 0..k {
        for mask in 0..(1 << k) {
            if mask & (1 << bit) != 0 {
                let lower = coefficients[mask ^ (1 << bit)];
                coefficients[mask] ^= lower;
            }
        }
    }
    let terms = (0..(1 << k))
        .filter(|&mask| coefficients[mask])
        .map(|mask| {
            (0..k)
                .filter(|&bit| mask & (1 << bit) != 0)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let gate = WideAnfGate::from_monomials(
        support.to_vec(),
        terms,
        16,
        WideAnfLimits::new(16, 256, 8192, 8192, 8192),
    )?;
    // Independent table-oracle check on all 2^k Boolean selected assignments.
    for mask in 0..(1 << k) {
        let mut input = [false; 16];
        for (local, &physical) in support.iter().enumerate() {
            input[physical] = mask & (1 << local) != 0;
        }
        if gate.evaluate_scalar(&input)? != truth[mask] {
            return Err("Möbius gate does not reproduce TRAIN majority table".into());
        }
    }
    Ok(gate)
}

fn train_bank(rows: &[TrainingRow]) -> Result<Vec<WideAnfGate>, Box<dyn Error>> {
    let supports = greedy_supports(rows);
    let mut bank = Vec::new();
    for support in supports {
        let counts = counts_for(rows, &support);
        let gate = gate_from_counts(&support, &counts)?;
        println!(
            "A6_E1_LEARNED,fanin={},physical_support={:?},active_monomials={},loo_bits={:.6}",
            support.len(),
            support,
            gate.active_monomials(),
            loo_bits(rows, &support)
        );
        bank.push(gate);
    }
    if bank.len() != 4 {
        return Err("four live trained ANF candidates required".into());
    }
    Ok(bank)
}

fn clip_probability(p: f64) -> f64 {
    p.clamp(1.0e-6, 1.0 - 1.0e-6)
}

fn binary_loss(p: f64, truth: bool) -> f64 {
    let p = clip_probability(p);
    if truth {
        -p.log2()
    } else {
        -(1.0 - p).log2()
    }
}

fn score(
    bytes: &[u8],
    committee: &PassiveCommittee,
    pairs: &ScopedPatternGuard,
    triples: &ScopedPatternGuard,
    nearest: &Hamming16Guard,
    ngram2: &NGramModel,
    ngram4: &NGramModel,
    majority_probability: f64,
) -> Result<Vec<EvalRow>, Box<dyn Error>> {
    let layout = ChunkLayout::new(bytes.len())?;
    let mut out = Vec::new();
    out.try_reserve_exact(LANES * (layout.chunk - 2))?;
    let mut probs2 = [0.0f64; 256];
    let mut probs4 = [0.0f64; 256];
    for lane in 0..LANES {
        let chunk = &bytes[lane * layout.chunk..(lane + 1) * layout.chunk];
        for t in 2..chunk.len() {
            let input = unpack(bits16(chunk[t - 1], chunk[t - 2]));
            let posterior = committee.predict(&input, 0.50)?;
            let pair = pairs.assess(&input)?;
            let triple = triples.assess(&input)?;
            let distance16 = nearest.distance(&input)?;
            ngram2.distribution(&chunk[..t], &mut probs2);
            ngram4.distribution(&chunk[..t], &mut probs4);
            out.push(EvalRow {
                truth: chunk[t] == EVENT,
                anf_probability: posterior.probability_true,
                anf_decision: posterior.map_prediction,
                confidence: posterior.confidence,
                ngram2_probability: probs2[usize::from(EVENT)],
                ngram4_probability: probs4[usize::from(EVENT)],
                majority_probability,
                missing2: pair.missing_patterns,
                missing3: triple.missing_patterns,
                distance16,
            });
        }
    }
    Ok(out)
}

fn policies() -> Vec<Policy> {
    let mut policies = Vec::new();
    for confidence in CONFIDENCES {
        for strength in STRENGTHS {
            for missing_limit in MISSING_LIMITS {
                for hamming_limit in HAMMING_LIMITS {
                    policies.push(Policy {
                        confidence,
                        strength,
                        missing_limit,
                        hamming_limit,
                    });
                }
            }
        }
    }
    policies
}

fn evaluate_policy(rows: &[EvalRow], policy: Policy) -> Metrics {
    let mut out = Metrics::default();
    for row in rows {
        let missing = match policy.strength {
            2 => row.missing2,
            3 => row.missing3,
            _ => 0, // predeclared stand-alone Hamming/no-guard controls only
        };
        let accepted = row.confidence >= policy.confidence
            && missing <= policy.missing_limit
            && row.distance16 <= policy.hamming_limit;
        let p = if accepted {
            row.anf_probability
        } else {
            row.ngram4_probability
        };
        out.total += 1;
        out.accepted += usize::from(accepted);
        out.accepted_errors += usize::from(accepted && row.anf_decision != row.truth);
        out.accepted_logloss += if accepted {
            binary_loss(row.anf_probability, row.truth)
        } else {
            0.0
        };
        out.full_fallback_logloss += binary_loss(p, row.truth);
        out.full_fallback_errors += usize::from((p >= 0.5) != row.truth);
    }
    out
}

fn baseline_score(rows: &[EvalRow], which: &str) -> (f64, usize) {
    let mut logloss = 0.0;
    let mut mistakes = 0usize;
    for row in rows {
        let probability = match which {
            "ngram2" => row.ngram2_probability,
            "ngram4" => row.ngram4_probability,
            "majority" => row.majority_probability,
            "anf" => row.anf_probability,
            _ => unreachable!(),
        };
        logloss += binary_loss(probability, row.truth);
        mistakes += usize::from((probability >= 0.5) != row.truth);
    }
    (logloss / rows.len() as f64, mistakes)
}

fn print_curve(split: &str, index: usize, policy: Policy, metrics: Metrics) {
    let risk = metrics
        .risk()
        .map_or(String::from("NA"), |value| format!("{value:.8}"));
    println!(
        "A6_E1_CURVE,split={},policy={},confidence={:.2},strength={},missing_limit={},hamming_limit={},tests={},accepted={},errors={},coverage={:.8},selective_risk={},accepted_logloss_bits={:.8},full_fallback_bits={:.8},fallback_errors={}",
        split,
        index,
        policy.confidence,
        policy.strength,
        policy.missing_limit,
        policy.hamming_limit,
        metrics.total,
        metrics.accepted,
        metrics.accepted_errors,
        metrics.coverage(),
        risk,
        if metrics.accepted == 0 {
            0.0
        } else {
            metrics.accepted_logloss / metrics.accepted as f64
        },
        metrics.fallback_bits(),
        metrics.full_fallback_errors
    );
}

fn standalone_controls() -> [(&'static str, Policy); 5] {
    [
        (
            "COMMITTEE_ONLY",
            Policy { confidence: 0.5, strength: 0, missing_limit: 0, hamming_limit: 16 },
        ),
        (
            "SCOPED_PAIR_ONLY",
            Policy { confidence: 0.5, strength: 2, missing_limit: 0, hamming_limit: 16 },
        ),
        (
            "SCOPED_TRIPLE_ONLY",
            Policy { confidence: 0.5, strength: 3, missing_limit: 0, hamming_limit: 16 },
        ),
        (
            "HAMMING_EXACT_ONLY",
            Policy { confidence: 0.5, strength: 0, missing_limit: 0, hamming_limit: 0 },
        ),
        (
            "HAMMING_ONE_ONLY",
            Policy { confidence: 0.5, strength: 0, missing_limit: 0, hamming_limit: 1 },
        ),
    ]
}

fn print_controls(split: &str, rows: &[EvalRow]) {
    for (name, policy) in standalone_controls() {
        let metric = evaluate_policy(rows, policy);
        println!(
            "A6_E1_CONTROL,split={},name={},tests={},accepted={},errors={},coverage={:.8},fallback_bits={:.8}",
            split,
            name,
            metric.total,
            metric.accepted,
            metric.accepted_errors,
            metric.coverage(),
            metric.fallback_bits()
        );
    }
}

fn run(path: &str) -> Result<(), Box<dyn Error>> {
    let raw = std::fs::read(path)?;
    if raw.len() != 433_036 {
        return Err("incorrect frozen Jules Verne byte count".into());
    }
    let splits = CharSplits::new(&raw);
    let train_start = Instant::now();
    let training = collect_rows(&splits.train)?;
    let train_positives = training.iter().filter(|row| row.truth).count();
    let majority_probability =
        (train_positives as f64 + 0.5) / (training.len() as f64 + 1.0);
    let gates = train_bank(&training)?;
    let positions = competing_support(&gates, 16)?;
    let mut committee = PassiveCommittee::new_with_source_width(
        gates,
        vec![4.0, 2.0, 2.0, 1.0],
        1.0 / 16.0,
        training.len(),
        16,
    )?;
    let mut pairs = ScopedPatternGuard::new(16, &positions, 2, training.len(), 1024)?;
    let mut triples = ScopedPatternGuard::new(16, &positions, 3, training.len(), 1024)?;
    let mut nearest = Hamming16Guard::new(training.len())?;
    let mut degree1 = DegreeDesignRank::new(16, &positions, 1)?;
    let mut degree2 = DegreeDesignRank::new(16, &positions, 2)?;
    for observation in &training {
        let input = unpack(observation.bits);
        committee.observe(&input, observation.truth)?;
        pairs.observe(&input)?;
        triples.observe(&input)?;
        nearest.observe(&input)?;
        degree1.observe(&input)?;
        degree2.observe(&input)?;
    }
    pairs.seal()?;
    triples.seal()?;
    nearest.seal()?;
    let ngram2 = NGramModel::train(&splits.train, 2);
    let ngram4 = NGramModel::train(&splits.train, 4);
    let train_seconds = train_start.elapsed().as_secs_f64();
    println!(
        "A6_E1_FROZEN,train_samples={},support_size={},pair_tuples={},triple_tuples={},degree1_rank={}/{},degree2_rank={}/{},degree_bound_known=false,hamming16_payload_bytes={},pair_payload_bytes={},triple_payload_bytes={},ngram2_counts={},ngram4_counts={},train_seconds={:.3}",
        training.len(),
        positions.len(),
        pairs.tuple_count(),
        triples.tuple_count(),
        degree1.rank(),
        degree1.monomial_count(),
        degree2.rank(),
        degree2.monomial_count(),
        nearest.logical_payload_bytes(),
        pairs.logical_payload_bytes(),
        triples.logical_payload_bytes(),
        ngram2.parameters(),
        ngram4.parameters(),
        train_seconds
    );

    // TEST is not read or scored until every policy and fallback has been
    // chosen using VAL only. Even the test corpus bytes are accessed below.
    let val_start = Instant::now();
    let validation = score(
        &splits.val,
        &committee,
        &pairs,
        &triples,
        &nearest,
        &ngram2,
        &ngram4,
        majority_probability,
    )?;
    let grid = policies();
    if grid.len() != 32 {
        return Err("predeclared 32-policy count violated".into());
    }
    let mut best: Option<(usize, f64)> = None;
    for (index, &policy) in grid.iter().enumerate() {
        let metric = evaluate_policy(&validation, policy);
        print_curve("VAL", index, policy, metric);
        if metric.coverage() >= REQUIRED_COVERAGE {
            let loss = metric.fallback_bits();
            if best.is_none_or(|(_, current)| loss < current - 1.0e-12) {
                best = Some((index, loss));
            }
        }
    }
    print_controls("VAL", &validation);
    let (val_ng2, _) = baseline_score(&validation, "ngram2");
    let (val_ng4, _) = baseline_score(&validation, "ngram4");
    println!(
        "A6_E1_VAL_BASELINES,tokens={},ngram2_bits={:.8},ngram4_bits={:.8},val_seconds={:.3}",
        validation.len(),
        val_ng2,
        val_ng4,
        val_start.elapsed().as_secs_f64()
    );
    let selected = best.map(|(index, _)| index);
    println!(
        "A6_E1_POLICY_FROZEN,selected={},criterion=minimum_VAL_fallback_bits_given_coverage_ge_0.80,grid=32",
        selected.map_or(String::from("NONE"), |i| i.to_string())
    );

    let test_start = Instant::now();
    let heldout = score(
        &splits.test,
        &committee,
        &pairs,
        &triples,
        &nearest,
        &ngram2,
        &ngram4,
        majority_probability,
    )?;
    let (test_ng2, ng2_errors) = baseline_score(&heldout, "ngram2");
    let (test_ng4, ng4_errors) = baseline_score(&heldout, "ngram4");
    let (test_anf, anf_errors) = baseline_score(&heldout, "anf");
    let (test_majority, majority_errors) = baseline_score(&heldout, "majority");
    println!(
        "A6_E1_TEST_BASELINES,tokens={},ngram2_bits={:.8},ngram2_errors={},ngram4_bits={:.8},ngram4_errors={},majority_bits={:.8},majority_errors={},anf_bits={:.8},anf_errors={}",
        heldout.len(),
        test_ng2,
        ng2_errors,
        test_ng4,
        ng4_errors,
        test_majority,
        majority_errors,
        test_anf,
        anf_errors
    );
    let mut test_selected = None;
    for (index, &policy) in grid.iter().enumerate() {
        let metric = evaluate_policy(&heldout, policy);
        print_curve("TEST", index, policy, metric);
        if Some(index) == selected {
            test_selected = Some(metric);
        }
    }
    print_controls("TEST", &heldout);
    let decision = if test_selected.is_some_and(|metric| {
        metric.coverage() >= REQUIRED_COVERAGE
            && metric.fallback_bits() <= test_ng4 - REQUIRED_LOGLOSS_ADVANTAGE
    }) {
        "RESEARCH_PASS"
    } else {
        "STOP"
    };
    let selected_coverage = test_selected.map_or(0.0, |metric| metric.coverage());
    let selected_bits = test_selected.map_or(test_ng4, |metric| metric.fallback_bits());
    println!(
        "A6_E1_COMPLETE,real_corpus=true,policies=32,val_rows=32,test_rows=32,task=next_space_binary,selected={},test_tokens={},ngram4_bits={:.8},selected_fallback_bits={:.8},selected_coverage={:.8},test_seconds={:.3},decision={},oracle=PASS",
        selected.map_or(String::from("NONE"), |i| i.to_string()),
        heldout.len(),
        test_ng4,
        selected_bits,
        selected_coverage,
        test_start.elapsed().as_secs_f64(),
        decision
    );
    Ok(())
}

fn main() {
    let path = match std::env::args().nth(1) {
        Some(path) => path,
        None => {
            eprintln!("usage: sml-real-byte-scoped-risk-bench <verified-verne-body-path>");
            std::process::exit(2);
        }
    };
    if let Err(error) = run(&path) {
        eprintln!("A6_E1_FAILURE,{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mobius_reconstructs_boolean_majority_gate_exactly() {
        let support = [0usize, 2, 4];
        let mut counts = vec![[0u32; 2]; 8];
        for (mask, slot) in counts.iter_mut().enumerate() {
            slot[usize::from(mask.count_ones() > 1)] = 4;
        }
        let gate = gate_from_counts(&support, &counts).unwrap();
        for mask in 0usize..8 {
            let mut bits = [false; 16];
            for (i, &position) in support.iter().enumerate() {
                bits[position] = mask & (1 << i) != 0;
            }
            assert_eq!(gate.evaluate_scalar(&bits).unwrap(), mask.count_ones() > 1);
        }
    }

    #[test]
    fn predefined_grid_and_risk_denominator_are_exact() {
        assert_eq!(policies().len(), 32);
        let data = [
            EvalRow {
                truth: true,
                anf_probability: 0.9,
                anf_decision: true,
                confidence: 0.9,
                ngram2_probability: 0.6,
                ngram4_probability: 0.6,
                majority_probability: 0.6,
                missing2: 0,
                missing3: 0,
                distance16: 0,
            },
            EvalRow {
                truth: true,
                anf_probability: 0.1,
                anf_decision: false,
                confidence: 0.9,
                ngram2_probability: 0.7,
                ngram4_probability: 0.7,
                majority_probability: 0.7,
                missing2: 0,
                missing3: 1,
                distance16: 1,
            },
        ];
        let strict = evaluate_policy(
            &data,
            Policy {
                confidence: 0.9,
                strength: 3,
                missing_limit: 0,
                hamming_limit: 0,
            },
        );
        assert_eq!(strict.total, 2);
        assert_eq!(strict.accepted, 1);
        assert_eq!(strict.accepted_errors, 0);
        assert_eq!(strict.full_fallback_errors, 0);
        let all = evaluate_policy(
            &data,
            Policy {
                confidence: 0.5,
                strength: 2,
                missing_limit: 1,
                hamming_limit: 4,
            },
        );
        assert_eq!(all.accepted, 2);
        assert_eq!(all.accepted_errors, 1);
    }

    #[test]
    fn real_byte_features_are_strictly_past_only() {
        assert_eq!(bits16(0xab, 0xcd), 0xcdab);
        assert_eq!(unpack(0xcdab)[0], true);
        assert_eq!(unpack(0xcdab)[15], true);
        assert_eq!(address(0b1010, &[1, 3]), 3);
        assert_eq!(address(0b1010, &[0, 2]), 0);
    }
}
