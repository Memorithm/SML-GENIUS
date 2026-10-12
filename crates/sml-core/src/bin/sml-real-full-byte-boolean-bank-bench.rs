//! A6-E3: preregistered next-byte (256 classes) real-data Boolean gate-bank.
//! All 128 gate functions are PUBLIC and fixed independently of training data.
//! TRAIN learns only gate subset selection and conditional byte probabilities.
//! VAL selects policies; TEST is not read or scored until the choices are frozen.

use std::{
    collections::BTreeSet,
    error::Error,
    time::Instant,
};

use sml_core::{
    anf_char::{CharSplits, ChunkLayout, LANES},
    anf_wide_sparse::{WideAnfGate, WideAnfLimits},
    char_baselines::NGramModel,
};

const EXPECTED_BYTES: usize = 433_036;
const TRAIN_EVENTS: usize = 346_112;
const VAL_EVENTS: usize = 43_008;
const TEST_EVENTS: usize = 43_008;
const BYTE_CLASSES: usize = 256;
const MAX_GATES: usize = 8;
const CANDIDATES: usize = 128;
const BETA: f64 = 128.0;
const K_CHOICES: [usize; 3] = [4, 6, 8];
const ALPHAS: [f64; 2] = [16.0, 128.0];
const LAMBDAS: [f64; 5] = [0.0, 0.1, 0.25, 0.5, 1.0];
const REQUIRED_GAIN: f64 = 0.02;
const REQUIRED_ANF_RAW_ADVANTAGE: f64 = 0.01;
const TIE_EPSILON: f64 = 1.0e-9;
const PAIRS: [(usize, usize); 4] = [(0, 1), (1, 2), (2, 3), (0, 3)];
const TRIPLES: [(usize, usize, usize); 4] =
    [(0, 1, 2), (1, 2, 3), (0, 2, 3), (0, 1, 3)];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arm {
    Raw32,
    Anf128,
}

impl Arm {
    const ALL: [Self; 2] = [Self::Raw32, Self::Anf128];

    fn name(self) -> &'static str {
        match self {
            Self::Raw32 => "RAW32",
            Self::Anf128 => "ANF128",
        }
    }

    fn cap(self) -> usize {
        match self {
            Self::Raw32 => 32,
            Self::Anf128 => CANDIDATES,
        }
    }
}

#[derive(Clone, Copy)]
struct TrainRow {
    context: u32,
    truth: u8,
}

#[derive(Clone)]
struct Table {
    arm: Arm,
    ids: Vec<usize>,
    counts: Vec<u32>,
    totals: Vec<u32>,
    occupied: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Policy {
    bank: usize,
    alpha_index: usize,
    lambda_index: usize,
}

#[derive(Clone, Copy, Default)]
struct Metrics {
    n: usize,
    loss: f64,
    correct: usize,
}

impl Metrics {
    fn add(&mut self, probability: f64, correct: bool) {
        self.loss -= probability.clamp(1.0e-12, 1.0).log2();
        self.n += 1;
        self.correct += usize::from(correct);
    }

    fn bits(self) -> f64 {
        self.loss / self.n as f64
    }

    fn accuracy(self) -> f64 {
        self.correct as f64 / self.n as f64
    }
}

struct Scored {
    baselines: [Metrics; 3],
    policies: Vec<Metrics>,
    unseen: [usize; 6],
    observations: usize,
}

fn raw_context(bytes: &[u8], t: usize) -> u32 {
    u32::from(bytes[t - 1])
        | (u32::from(bytes[t - 2]) << 8)
        | (u32::from(bytes[t - 3]) << 16)
        | (u32::from(bytes[t - 4]) << 24)
}

fn gate_value(context: u32, id: usize) -> bool {
    assert!(id < CANDIDATES);
    let bit = |slot: usize| (context >> slot) & 1 != 0;
    match id {
        0..=31 => bit(id),
        32..=63 => {
            let offset = id - 32;
            let (a, b) = PAIRS[offset / 8];
            bit(8 * a + offset % 8) ^ bit(8 * b + offset % 8)
        }
        64..=95 => {
            let offset = id - 64;
            let (a, b) = PAIRS[offset / 8];
            bit(8 * a + offset % 8) & bit(8 * b + offset % 8)
        }
        _ => {
            let offset = id - 96;
            let (a, b, c) = TRIPLES[offset / 8];
            bit(8 * a + offset % 8)
                ^ bit(8 * b + offset % 8)
                ^ bit(8 * c + offset % 8)
        }
    }
}

fn gate_definition(id: usize) -> (Vec<usize>, Vec<Vec<usize>>) {
    match id {
        0..=31 => (vec![id], vec![vec![0]]),
        32..=63 => {
            let offset = id - 32;
            let (a, b) = PAIRS[offset / 8];
            (
                vec![8 * a + offset % 8, 8 * b + offset % 8],
                vec![vec![0], vec![1]],
            )
        }
        64..=95 => {
            let offset = id - 64;
            let (a, b) = PAIRS[offset / 8];
            (
                vec![8 * a + offset % 8, 8 * b + offset % 8],
                vec![vec![0, 1]],
            )
        }
        96..=127 => {
            let offset = id - 96;
            let (a, b, c) = TRIPLES[offset / 8];
            (
                vec![8 * a + offset % 8, 8 * b + offset % 8, 8 * c + offset % 8],
                vec![vec![0], vec![1], vec![2]],
            )
        }
        _ => panic!("unregistered public gate"),
    }
}

fn verify_native_gate(id: usize) -> Result<(), Box<dyn Error>> {
    let (positions, monomials) = gate_definition(id);
    let width = positions.len();
    let native = WideAnfGate::from_monomials(
        positions.clone(),
        monomials,
        32,
        WideAnfLimits::new(3, 8, 32, 32, 32),
    )?;
    for local_bits in 0..(1usize << width) {
        let mut source = [false; 32];
        let mut packed = 0u32;
        for (local, &physical) in positions.iter().enumerate() {
            let value = local_bits & (1 << local) != 0;
            source[physical] = value;
            packed |= u32::from(value) << physical;
        }
        if native.evaluate_scalar(&source)? != gate_value(packed, id) {
            return Err(format!("native ANF truth mismatch for public gate {id}").into());
        }
    }
    Ok(())
}

fn training_rows(bytes: &[u8]) -> Result<Vec<TrainRow>, Box<dyn Error>> {
    let layout = ChunkLayout::new(bytes.len())?;
    let mut rows = Vec::new();
    rows.try_reserve_exact(LANES * (layout.chunk - 4))?;
    for lane in 0..LANES {
        let chunk = &bytes[lane * layout.chunk..(lane + 1) * layout.chunk];
        for t in 4..chunk.len() {
            rows.push(TrainRow {
                context: raw_context(chunk, t),
                truth: chunk[t],
            });
        }
    }
    Ok(rows)
}

fn initial_prior(rows: &[TrainRow]) -> [f64; BYTE_CLASSES] {
    let mut counts = [0u32; BYTE_CLASSES];
    for row in rows {
        counts[usize::from(row.truth)] += 1;
    }
    let divisor = rows.len() as f64 + 128.0;
    std::array::from_fn(|index| (f64::from(counts[index]) + 0.5) / divisor)
}

fn candidate_masks(rows: &[TrainRow]) -> Vec<u128> {
    rows.iter()
        .map(|row| {
            let mut packed = 0u128;
            for id in 0..CANDIDATES {
                if gate_value(row.context, id) {
                    packed |= 1u128 << id;
                }
            }
            packed
        })
        .collect()
}

fn signature(features: u128, ids: &[usize]) -> usize {
    ids.iter().enumerate().fold(0usize, |out, (index, &id)| {
        out | (((features >> id) as usize & 1) << index)
    })
}

/// Exact leave-one-out log loss, aggregated over identical (signature,target)
/// pairs. Each observation in a group contributes the same leave-one-out term.
fn candidate_loo(
    rows: &[TrainRow],
    masks: &[u128],
    prefix: &[u8],
    candidate: usize,
    k: usize,
    prior: &[f64; BYTE_CLASSES],
) -> f64 {
    let groups = 1usize << k;
    let mut counts = vec![0u32; groups * BYTE_CLASSES];
    let mut totals = vec![0u32; groups];
    let mask = 1u128 << candidate;
    for (index, row) in rows.iter().enumerate() {
        let last_bit = usize::from(masks[index] & mask != 0);
        let key = usize::from(prefix[index]) | (last_bit << (k - 1));
        counts[key * BYTE_CLASSES + usize::from(row.truth)] += 1;
        totals[key] += 1;
    }
    let mut negative_log2 = 0.0f64;
    for (key, &total) in totals.iter().enumerate() {
        if total == 0 {
            continue;
        }
        let log_denom = (f64::from(total - 1) + BETA).log2();
        for (y, &prior_y) in prior.iter().enumerate() {
            let count = counts[key * BYTE_CLASSES + y];
            if count != 0 {
                let log_numerator = (f64::from(count - 1) + BETA * prior_y).log2();
                negative_log2 += f64::from(count) * (log_denom - log_numerator);
            }
        }
    }
    negative_log2 / rows.len() as f64
}

/// Both arms have eight greedy stages and exactly the same LOO criterion.
fn greedy_select(
    arm: Arm,
    rows: &[TrainRow],
    masks: &[u128],
    prior: &[f64; BYTE_CLASSES],
) -> ([usize; MAX_GATES], [f64; MAX_GATES], usize) {
    let mut chosen = [usize::MAX; MAX_GATES];
    let mut losses = [0.0f64; MAX_GATES];
    let mut prefix = vec![0u8; rows.len()];
    let mut evaluations = 0usize;
    for stage in 0..MAX_GATES {
        let mut winner = usize::MAX;
        let mut best_loss = f64::INFINITY;
        for id in 0..arm.cap() {
            if chosen[..stage].contains(&id) {
                continue;
            }
            let loss = candidate_loo(rows, masks, &prefix, id, stage + 1, prior);
            evaluations += 1;
            if loss < best_loss - TIE_EPSILON
                || ((loss - best_loss).abs() <= TIE_EPSILON && id < winner)
            {
                winner = id;
                best_loss = loss;
            }
        }
        assert!(winner < arm.cap(), "the greedy feature search must choose a gate");
        chosen[stage] = winner;
        losses[stage] = best_loss;
        for (entry, &features) in prefix.iter_mut().zip(masks) {
            *entry |= u8::from(features & (1u128 << winner) != 0) << stage;
        }
    }
    (chosen, losses, evaluations)
}

fn fit_table(arm: Arm, ids: &[usize], rows: &[TrainRow], masks: &[u128]) -> Table {
    let groups = 1usize << ids.len();
    let mut counts = vec![0u32; groups * BYTE_CLASSES];
    let mut totals = vec![0u32; groups];
    for (row, &features) in rows.iter().zip(masks) {
        let key = signature(features, ids);
        counts[key * BYTE_CLASSES + usize::from(row.truth)] += 1;
        totals[key] += 1;
    }
    let occupied = totals.iter().filter(|&&v| v != 0).count();
    Table { arm, ids: ids.to_vec(), counts, totals, occupied }
}

fn make_policies() -> Vec<Policy> {
    let mut policies = Vec::with_capacity(60);
    for bank in 0..6 {
        for alpha_index in 0..ALPHAS.len() {
            for lambda_index in 0..LAMBDAS.len() {
                policies.push(Policy { bank, alpha_index, lambda_index });
            }
        }
    }
    policies
}

fn score_split(
    bytes: &[u8],
    ngram2: &NGramModel,
    ngram4: &NGramModel,
    prior: &[f64; BYTE_CLASSES],
    banks: &[Table],
    policies: &[Policy],
) -> Result<Scored, Box<dyn Error>> {
    let layout = ChunkLayout::new(bytes.len())?;
    let mut scores = vec![Metrics::default(); policies.len()];
    let mut baselines = [Metrics::default(); 3];
    let mut unseen = [0usize; 6];
    let mut observations = 0usize;
    let mut ng2 = [0.0f64; BYTE_CLASSES];
    let mut ng4 = [0.0f64; BYTE_CLASSES];
    let unigram_best = (0..BYTE_CLASSES)
        .max_by(|&a, &b| prior[a].total_cmp(&prior[b]).then_with(|| b.cmp(&a)))
        .unwrap();
    for lane in 0..LANES {
        let chunk = &bytes[lane * layout.chunk..(lane + 1) * layout.chunk];
        for t in 4..chunk.len() {
            let target = usize::from(chunk[t]);
            let features = {
                let context = raw_context(chunk, t);
                let mut mask = 0u128;
                for id in 0..CANDIDATES {
                    if gate_value(context, id) {
                        mask |= 1u128 << id;
                    }
                }
                mask
            };
            ngram2.distribution(&chunk[..t], &mut ng2);
            ngram4.distribution(&chunk[..t], &mut ng4);
            let mut best2 = 0usize;
            let mut best4 = 0usize;
            for y in 1..BYTE_CLASSES {
                if ng2[y] > ng2[best2] { best2 = y; }
                if ng4[y] > ng4[best4] { best4 = y; }
            }
            baselines[0].add(prior[target], unigram_best == target);
            baselines[1].add(ng2[target], best2 == target);
            baselines[2].add(ng4[target], best4 == target);

            let mut keys = [0usize; 6];
            for (index, bank) in banks.iter().enumerate() {
                keys[index] = signature(features, &bank.ids);
                unseen[index] += usize::from(bank.totals[keys[index]] == 0);
            }

            for (index, policy) in policies.iter().enumerate() {
                let bank = &banks[policy.bank];
                let key = keys[policy.bank];
                let alpha = ALPHAS[policy.alpha_index];
                let lambda = LAMBDAS[policy.lambda_index];
                let denom = f64::from(bank.totals[key]) + alpha;
                let start = key * BYTE_CLASSES;
                let mut max_prob = f64::NEG_INFINITY;
                let mut argmax = 0usize;
                let mut truth_prob = 0.0f64;
                let mut sum = 0.0f64;
                for (y, &prior_y) in prior.iter().enumerate() {
                    let boolean = (f64::from(bank.counts[start + y]) + alpha * prior_y) / denom;
                    let p = lambda * boolean + (1.0 - lambda) * ng4[y];
                    if p > max_prob {
                        max_prob = p;
                        argmax = y;
                    }
                    if y == target {
                        truth_prob = p;
                    }
                    sum += p;
                }
                if !sum.is_finite() || (sum - 1.0).abs() > 1.0e-8
                    || !(0.0..=1.0).contains(&truth_prob) || truth_prob == 0.0
                {
                    return Err("non-normalized or non-positive 256-way distribution".into());
                }
                scores[index].add(truth_prob, argmax == target);
            }
            observations += 1;
        }
    }
    if scores.iter().any(|m| m.n != observations)
        || baselines.iter().any(|m| m.n != observations)
    {
        return Err("split scoring denominator mismatch".into());
    }
    Ok(Scored { baselines, policies: scores, unseen, observations })
}

fn chosen(
    bank_arm: Arm,
    policies: &[Policy],
    scores: &[Metrics],
    pure_only: bool,
) -> Result<usize, Box<dyn Error>> {
    let mut best = None;
    for (i, &policy) in policies.iter().enumerate() {
        let belongs = (policy.bank < 3) == (bank_arm == Arm::Raw32);
        if !belongs || (pure_only && LAMBDAS[policy.lambda_index] != 1.0) {
            continue;
        }
        let loss = scores[i].bits();
        if best.is_none_or(|(_, current)| loss < current - TIE_EPSILON) {
            best = Some((i, loss));
        }
    }
    Ok(best.ok_or("no predeclared model chosen")?.0)
}

fn print_metric(prefix: &str, name: &str, metrics: Metrics) {
    println!(
        "A6_E3_METRIC,split={},model={},events={},bits_per_byte={:.8},correct={},accuracy={:.8}",
        prefix, name, metrics.n, metrics.bits(), metrics.correct, metrics.accuracy()
    );
}

fn print_selected(
    split: &str,
    which: &str,
    policies: &[Policy],
    scores: &[Metrics],
    banks: &[Table],
    policy_index: usize,
) {
    let policy = policies[policy_index];
    let metrics = scores[policy_index];
    println!(
        "A6_E3_SELECTED,split={},arm={},which={},index={},k={},alpha={:.1},lambda={:.2},events={},bits_per_byte={:.8},correct={},accuracy={:.8}",
        split, banks[policy.bank].arm.name(), which, policy_index,
        banks[policy.bank].ids.len(), ALPHAS[policy.alpha_index],
        LAMBDAS[policy.lambda_index], metrics.n, metrics.bits(),
        metrics.correct, metrics.accuracy()
    );
}

fn run(path: &str) -> Result<(), Box<dyn Error>> {
    let raw = std::fs::read(path)?;
    if raw.len() != EXPECTED_BYTES {
        return Err("incorrect frozen Jules Verne corpus length".into());
    }
    let splits = CharSplits::new(&raw);
    let started = Instant::now();
    let rows = training_rows(&splits.train)?;
    if rows.len() != TRAIN_EVENTS {
        return Err("unexpected TRAIN byte count".into());
    }
    let prior = initial_prior(&rows);
    if (prior.iter().sum::<f64>() - 1.0).abs() > 1.0e-12
        || prior.iter().any(|&p| p <= 0.0 || !p.is_finite())
    {
        return Err("invalid TRAIN unigram prior".into());
    }
    let masks = candidate_masks(&rows);
    let mut banks = Vec::with_capacity(6);
    let mut distinct_native = BTreeSet::new();
    for arm in Arm::ALL {
        let (order, losses, evaluated) = greedy_select(arm, &rows, &masks, &prior);
        println!(
            "A6_E3_GREEDY,arm={},candidate_pool={},stages=8,attempts={},selected={:?},loo_bits={:?}",
            arm.name(), arm.cap(), evaluated, order, losses
        );
        for &id in &order {
            if distinct_native.insert(id) {
                verify_native_gate(id)?;
            }
        }
        for &k in &K_CHOICES {
            let table = fit_table(arm, &order[..k], &rows, &masks);
            println!(
                "A6_E3_BANK,arm={},k={},ids={:?},signature_slots={},occupied={},count_payload_bytes={},total_payload_bytes={},train_events={},public_native_gate_oracle=true",
                arm.name(), k, table.ids, table.totals.len(), table.occupied,
                table.counts.len() * std::mem::size_of::<u32>(),
                table.totals.len() * std::mem::size_of::<u32>(), rows.len()
            );
            banks.push(table);
        }
    }
    if banks.len() != 6 {
        return Err("exactly six predeclared raw/anf gate banks required".into());
    }
    let ngram2 = NGramModel::train(&splits.train, 2);
    let ngram4 = NGramModel::train(&splits.train, 4);
    println!(
        "A6_E3_TRAIN,events={},native_gates_verified={},banks={},ngram2_count_entries={},ngram4_count_entries={},training_seconds={:.3},ngram_early_context_uses_t1_t3=true",
        rows.len(), distinct_native.len(), banks.len(), ngram2.parameters(), ngram4.parameters(),
        started.elapsed().as_secs_f64()
    );
    // All label-driven variable and gate selection completed on TRAIN.
    let policy_grid = make_policies();
    if policy_grid.len() != 60 {
        return Err("incorrect 60-policy frozen grid".into());
    }

    let validation_started = Instant::now();
    let validation = score_split(&splits.val, &ngram2, &ngram4, &prior, &banks, &policy_grid)?;
    if validation.observations != VAL_EVENTS {
        return Err("unexpected validation byte count".into());
    }
    for (index, &policy) in policy_grid.iter().enumerate() {
        let score = validation.policies[index];
        let arm = banks[policy.bank].arm;
        let k = banks[policy.bank].ids.len();
        if policy.lambda_index == LAMBDAS.len() - 1 {
            println!(
                "A6_E3_VAL_PURE,arm={},index={},k={},alpha={:.1},events={},bits_per_byte={:.8},accuracy={:.8}",
                arm.name(), index, k, ALPHAS[policy.alpha_index], score.n, score.bits(), score.accuracy()
            );
        }
        println!(
            "A6_E3_VAL_MIX,arm={},index={},k={},alpha={:.1},lambda={:.2},events={},bits_per_byte={:.8},accuracy={:.8}",
            arm.name(), index, k, ALPHAS[policy.alpha_index],
            LAMBDAS[policy.lambda_index], score.n, score.bits(), score.accuracy()
        );
    }
    for (index, bank) in banks.iter().enumerate() {
        println!(
            "A6_E3_COVERAGE,split=VAL,arm={},k={},unseen_events={},total={},unseen_fraction={:.8}",
            bank.arm.name(), bank.ids.len(), validation.unseen[index], VAL_EVENTS,
            validation.unseen[index] as f64 / VAL_EVENTS as f64
        );
    }
    let mut winners = Vec::<(Arm, usize, usize)>::new();
    for arm in Arm::ALL {
        let pure = chosen(arm, &policy_grid, &validation.policies, true)?;
        let mixed = chosen(arm, &policy_grid, &validation.policies, false)?;
        winners.push((arm, pure, mixed));
        print_selected("VAL", "PURE", &policy_grid, &validation.policies, &banks, pure);
        print_selected("VAL", "MIX", &policy_grid, &validation.policies, &banks, mixed);
    }
    print_metric("VAL", "UNIGRAM", validation.baselines[0]);
    print_metric("VAL", "NGRAM2", validation.baselines[1]);
    print_metric("VAL", "NGRAM4", validation.baselines[2]);
    println!(
        "A6_E3_ALL_FROZEN,train_events={},val_events={},val_pure=12,val_mix=60,raw_pure={},raw_mix={},anf_pure={},anf_mix={},validation_seconds={:.3},first_test_after_frozen=true",
        rows.len(), VAL_EVENTS, winners[0].1, winners[0].2, winners[1].1, winners[1].2,
        validation_started.elapsed().as_secs_f64()
    );

    // The ONLY four TEST policies are the two VAL-frozen winners per arm.
    let selected_policies = winners.iter().flat_map(|&(_, pure, mixed)| {
        [policy_grid[pure], policy_grid[mixed]]
    }).collect::<Vec<_>>();
    let testing_started = Instant::now();
    let test = score_split(&splits.test, &ngram2, &ngram4, &prior, &banks, &selected_policies)?;
    if test.observations != TEST_EVENTS || test.policies.len() != 4 {
        return Err("unexpected heldout byte count or selected model count".into());
    }
    for (index, bank) in banks.iter().enumerate() {
        println!(
            "A6_E3_COVERAGE,split=TEST,arm={},k={},unseen_events={},total={},unseen_fraction={:.8}",
            bank.arm.name(), bank.ids.len(), test.unseen[index], TEST_EVENTS,
            test.unseen[index] as f64 / TEST_EVENTS as f64
        );
    }
    for (offset, &(arm, _, _)) in winners.iter().enumerate() {
        print_selected("TEST", "PURE", &selected_policies, &test.policies, &banks, offset * 2);
        print_selected("TEST", "MIX", &selected_policies, &test.policies, &banks, offset * 2 + 1);
        assert_eq!(banks[selected_policies[offset * 2].bank].arm, arm);
        assert_eq!(banks[selected_policies[offset * 2 + 1].bank].arm, arm);
    }
    print_metric("TEST", "UNIGRAM", test.baselines[0]);
    print_metric("TEST", "NGRAM2", test.baselines[1]);
    print_metric("TEST", "NGRAM4", test.baselines[2]);
    let baseline = test.baselines[2].bits();
    let raw_mix = test.policies[1].bits();
    let anf_pure = test.policies[2].bits();
    let anf_mix = test.policies[3].bits();
    let decision = if anf_pure <= baseline - REQUIRED_GAIN
        && anf_mix <= baseline - REQUIRED_GAIN
        && anf_mix <= raw_mix - REQUIRED_ANF_RAW_ADVANTAGE
    {
        "RESEARCH_PASS"
    } else {
        "STOP"
    };
    println!(
        "A6_E3_COMPLETE,real_corpus=true,task=next_byte_256,train_events=346112,val_events=43008,test_events=43008,val_pure=12,val_mix=60,test_models=4,ngram4_bits_per_byte={:.8},raw_mix_bits_per_byte={:.8},anf_pure_bits_per_byte={:.8},anf_mix_bits_per_byte={:.8},decision={},test_seconds={:.3},integrity=PASS",
        baseline, raw_mix, anf_pure, anf_mix, decision,
        testing_started.elapsed().as_secs_f64()
    );
    Ok(())
}

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 2 {
        eprintln!("usage: sml-real-full-byte-boolean-bank-bench <verified-verne-corpus>");
        std::process::exit(2);
    }
    if let Err(error) = run(&args[1]) {
        eprintln!("A6_E3_FAILURE,{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_128_native_functions_match_public_truthtables() {
        for id in 0..CANDIDATES {
            verify_native_gate(id).unwrap();
        }
        assert_eq!(PAIRS.len() * 8, 32);
        assert_eq!(TRIPLES.len() * 8, 32);
    }

    #[test]
    fn feature_selection_is_train_only_and_alpha_normalizes() {
        let rows = (0..64usize).map(|i| TrainRow {
            context: (i as u32).wrapping_mul(0x9e37_79b9),
            truth: (i % 11) as u8,
        }).collect::<Vec<_>>();
        let prior = initial_prior(&rows);
        assert!((prior.iter().sum::<f64>() - 1.0).abs() < 1.0e-12);
        let masks = candidate_masks(&rows);
        for arm in Arm::ALL {
            let (ids, _, evaluated) = greedy_select(arm, &rows, &masks, &prior);
            assert_eq!(evaluated, 8 * arm.cap() - 28);
            assert_eq!(ids.iter().copied().collect::<BTreeSet<_>>().len(), 8);
            let bank = fit_table(arm, &ids[..4], &rows, &masks);
            assert_eq!(bank.totals.iter().map(|&x| x as usize).sum::<usize>(), rows.len());
            for s in 0..bank.totals.len() {
                let total = f64::from(bank.totals[s]);
                let alpha = ALPHAS[0];
                let sum = (0..256usize).map(|y| {
                    (f64::from(bank.counts[s * 256 + y]) + alpha * prior[y])
                        / (total + alpha)
                }).sum::<f64>();
                assert!((sum - 1.0).abs() < 1.0e-10);
            }
        }
    }

    #[test]
    fn frozen_grid_covers_12_pure_and_60_mix_without_oracle() {
        let grid = make_policies();
        assert_eq!(grid.len(), 60);
        assert_eq!(grid.iter().filter(|p| p.lambda_index == 4).count(), 12);
        assert_eq!(grid.iter().filter(|p| p.lambda_index == 0).count(), 12);
        assert_eq!(ALPHAS.len() * LAMBDAS.len() * K_CHOICES.len() * Arm::ALL.len(), 60);
        assert_eq!(TRAIN_EVENTS, 346_112);
        assert_eq!(VAL_EVENTS, TEST_EVENTS);
        let p = 0.027f64;
        assert_eq!(LAMBDAS[0] * p + (1.0 - LAMBDAS[0]) * 0.039, 0.039);
    }

    #[test]
    fn loo_count_aggregation_agrees_with_bruteforce() {
        let rows = (0..32usize).map(|i| TrainRow {
            context: (i as u32) ^ ((i as u32 / 3) << 8),
            truth: (i % 7) as u8,
        }).collect::<Vec<_>>();
        let masks = candidate_masks(&rows);
        let prior = initial_prior(&rows);
        let prefix = vec![0u8; rows.len()];
        let fast = candidate_loo(&rows, &masks, &prefix, 4, 1, &prior);
        let mut score = 0.0f64;
        for (i, row) in rows.iter().enumerate() {
            let sig = usize::from(masks[i] & (1u128 << 4) != 0);
            let mut count = 0usize;
            let mut total = 0usize;
            for other in &rows {
                if usize::from(gate_value(other.context, 4)) == sig {
                    total += 1;
                    count += usize::from(other.truth == row.truth);
                }
            }
            let p = ((count - 1) as f64 + BETA * prior[usize::from(row.truth)])
                / ((total - 1) as f64 + BETA);
            score -= p.log2();
        }
        assert!((fast - score / rows.len() as f64).abs() < 1.0e-10);
    }
}
