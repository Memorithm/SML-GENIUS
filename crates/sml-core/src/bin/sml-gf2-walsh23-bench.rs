//! A6-C12b: preregistered exact GF(2), arbitrary-pair Walsh and exhaustive
//! order-three Walsh source-screening costs on a bounded 3-gate N128 ring.
//! Noise is generated independently, but teacher-forced previous terminal
//! outputs also contain label noise; Gaussian elimination is clean-only.
//! ALL fitted policies freeze before the separate native TEST streams exist.
//! Not a solver for general LPN, a free recurrent graph, or SML-25B.

use std::{collections::BTreeSet, error::Error, time::Instant};

use sml_core::{
    anf_dual_hidden_credit::{make_multi_gate_native, native_errors},
    anf_intermediate_credit::IntermediateRule,
    anf_local_incremental::{LaneBlock, LaneData},
    anf_three_gate_diagnostic::{fit_three_gate_candidates, BudgetFit, Source},
    toy_tasks::SplitMix64,
};

const WIDTH: usize = 128;
const INPUT_BITS: usize = 5;
const LANES: usize = 64;
const TRAIN_STEPS: usize = 512;
const VAL_STEPS: usize = 512;
const TEST_STEPS: usize = 4096;
const GAUSS_STEPS: usize = 16;
const TOP_PAIRS: usize = 8;
const TOP_TRIPLES: usize = 16;
const NS: [usize; 3] = [32, 64, 128];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Task {
    XorXorXor,
    AndXorXor,
    XorAndXor,
    AndAndAnd,
}
impl Task {
    const ALL: [Self; 4] = [
        Self::XorXorXor,
        Self::AndXorXor,
        Self::XorAndXor,
        Self::AndAndAnd,
    ];
    fn name(self) -> &'static str {
        match self {
            Self::XorXorXor => "xor_xor_xor",
            Self::AndXorXor => "and_xor_xor",
            Self::XorAndXor => "xor_and_xor",
            Self::AndAndAnd => "and_and_and",
        }
    }
    fn index(self) -> usize {
        Self::ALL.iter().position(|&t| t == self).unwrap()
    }
    fn truths(self) -> [u8; 3] {
        match self {
            Self::XorXorXor => [6, 6, 6],
            Self::AndXorXor => [8, 6, 6],
            Self::XorAndXor => [6, 8, 6],
            Self::AndAndAnd => [8, 8, 8],
        }
    }
    fn sources(self) -> [Source; 3] {
        [
            Source { gate: 20, raw: 2 },
            Source { gate: 68, raw: 3 },
            Source { gate: 116, raw: 4 },
        ]
    }
    fn rules(self) -> Vec<IntermediateRule> {
        let sources = self.sources();
        let truths = self.truths();
        (0..3)
            .map(|i| IntermediateRule {
                gate: sources[i].gate,
                raw: sources[i].raw,
                truth: truths[i],
            })
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Panel {
    Clean,
    Noise,
}
impl Panel {
    const ALL: [Self; 2] = [Self::Clean, Self::Noise];
    fn name(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::Noise => "noise_1_in_16",
        }
    }
    fn index(self) -> usize {
        match self {
            Self::Clean => 0,
            Self::Noise => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Method {
    GaussExact,
    Walsh2Pair,
    Walsh3Exhaustive,
}
impl Method {
    const ALL: [Self; 3] = [Self::GaussExact, Self::Walsh2Pair, Self::Walsh3Exhaustive];
    fn name(self) -> &'static str {
        match self {
            Self::GaussExact => "gauss_exact",
            Self::Walsh2Pair => "walsh2_pair",
            Self::Walsh3Exhaustive => "walsh3_exhaustive",
        }
    }
}

fn seed(index: usize, task: Task, panel: Panel, stage: u64) -> u64 {
    (index as u64 + 1).wrapping_mul(0xd134_2543_de82_ef95)
        ^ (task.index() as u64 + 1).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ (panel.index() as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ stage
}

fn data(stream: u64, task: Task, panel: Panel, steps: usize) -> LaneData {
    let teacher = make_multi_gate_native(WIDTH, &task.rules()).unwrap();
    let mut rng = SplitMix64::new(stream);
    let mut noise_rng = SplitMix64::new(stream ^ 0xa6c1_2b66_dada_1177);
    let mut inputs = vec![0u64; steps * INPUT_BITS];
    for word in &mut inputs {
        *word = rng.next_u64();
    }
    let mut state = vec![0u64; teacher.source_width()];
    let mut next = vec![0u64; WIDTH];
    let mut scratch = vec![0u64; teacher.scratch_len()];
    let mut targets = vec![0u64; steps];
    for (t, target) in targets.iter_mut().enumerate() {
        let from = t * INPUT_BITS;
        state[WIDTH..].copy_from_slice(&inputs[from..from + INPUT_BITS]);
        teacher.step_lanes(&mut state, &mut next, &mut scratch);
        let clean = teacher.readout().evaluate_lanes(&state, &mut scratch);
        let noise = if panel == Panel::Noise {
            noise_rng.next_u64()
                & noise_rng.next_u64()
                & noise_rng.next_u64()
                & noise_rng.next_u64()
        } else {
            0
        };
        *target = clean ^ noise;
    }
    LaneData {
        input_bits: INPUT_BITS,
        target_bits: 1,
        blocks: vec![LaneBlock {
            steps,
            inputs,
            targets,
            valid: u64::MAX,
        }],
    }
}

/// The only search-space prior: a fixed, increasingly dense grid of valid
/// gate positions. The true mutated positions are not provided to the ranker.
fn grid(n: usize) -> Vec<Source> {
    let gates = match n {
        32 => (0..8).map(|i| 4 + 16 * i).collect::<Vec<_>>(),
        64 => (0..16).map(|i| 4 + 8 * i).collect::<Vec<_>>(),
        128 => (0..31)
            .map(|i| 4 + 4 * i)
            .chain(std::iter::once(127))
            .collect::<Vec<_>>(),
        _ => panic!("only preregistered grid sizes"),
    };
    let source = gates
        .into_iter()
        .flat_map(|gate| (1..=4).map(move |raw| Source { gate, raw }))
        .collect::<Vec<_>>();
    assert_eq!(source.len(), n);
    source
}

/// All source columns are derived only from the observed final TRAIN labels
/// and raw inputs: p = z XOR Rpast. The use of a past noisy R introduces a
/// second independent label-noise bit on the proxy, not on autonomous TEST.
struct Observed {
    p: Vec<u64>,
    columns: Vec<Vec<u64>>,
    sources: Vec<Source>,
}
impl Observed {
    fn new(train: &LaneData, sources: Vec<Source>) -> Self {
        let b = &train.blocks[0];
        assert!(b.steps > WIDTH && b.valid == u64::MAX);
        let get = |t: usize, raw: usize| b.inputs[t * INPUT_BITS + raw];
        let p = (WIDTH..b.steps)
            .map(|t| {
                let r_now = b.targets[t] ^ get(t, 0);
                let r_past = b.targets[t - WIDTH] ^ get(t - WIDTH, 0);
                r_now ^ get(t, 1) ^ r_past
            })
            .collect::<Vec<_>>();
        let columns = sources
            .iter()
            .map(|s| {
                (WIDTH..b.steps)
                    .map(|t| get(t - (WIDTH - s.gate), s.raw))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        Self { p, columns, sources }
    }
    fn same_gate(&self, i: usize, j: usize) -> bool {
        self.sources[i].gate == self.sources[j].gate
    }
    fn magnitude(&self, flips: u64) -> u64 {
        let samples = (self.p.len() * LANES) as u64;
        samples.abs_diff(2 * flips)
    }
    fn pair_magnitude(&self, i: usize, j: usize) -> u64 {
        let errors = self
            .p
            .iter()
            .zip(&self.columns[i])
            .zip(&self.columns[j])
            .map(|((&p, &a), &b)| u64::from((p ^ a ^ b).count_ones()))
            .sum();
        self.magnitude(errors)
    }
}

#[derive(Clone, Copy)]
struct PairScore {
    ids: [usize; 2],
    correlation: u64,
}

#[derive(Clone, Copy)]
struct TripleScore {
    ids: [usize; 3],
    correlation: u64,
}

fn rank_pairs(o: &Observed) -> Vec<PairScore> {
    let mut scores = Vec::new();
    for i in 0..o.sources.len() {
        for j in i + 1..o.sources.len() {
            if o.same_gate(i, j) {
                continue;
            }
            scores.push(PairScore {
                ids: [i, j],
                correlation: o.pair_magnitude(i, j),
            });
        }
    }
    scores.sort_unstable_by(|a, b| {
        b.correlation
            .cmp(&a.correlation)
            .then_with(|| a.ids.cmp(&b.ids))
    });
    scores
}

fn rank_triples(o: &Observed) -> Vec<TripleScore> {
    let mut scores = Vec::new();
    for i in 0..o.sources.len() {
        for j in i + 1..o.sources.len() {
            if o.same_gate(i, j) {
                continue;
            }
            // Share the first two XORs across all candidate third sources.
            let paired = o
                .p
                .iter()
                .zip(&o.columns[i])
                .zip(&o.columns[j])
                .map(|((&p, &a), &b)| p ^ a ^ b)
                .collect::<Vec<_>>();
            for k in j + 1..o.sources.len() {
                if o.same_gate(i, k) || o.same_gate(j, k) {
                    continue;
                }
                let errors = paired
                    .iter()
                    .zip(&o.columns[k])
                    .map(|(&p, &c)| u64::from((p ^ c).count_ones()))
                    .sum();
                scores.push(TripleScore {
                    ids: [i, j, k],
                    correlation: o.magnitude(errors),
                });
            }
        }
    }
    scores.sort_unstable_by(|a, b| {
        b.correlation
            .cmp(&a.correlation)
            .then_with(|| a.ids.cmp(&b.ids))
    });
    scores
}

fn lowest_bit(values: &[u64]) -> Option<usize> {
    values
        .iter()
        .enumerate()
        .find(|(_, word)| **word != 0)
        .map(|(index, &word)| index * 64 + word.trailing_zeros() as usize)
}
fn xor_assign(to: &mut [u64], other: &[u64]) {
    for (left, right) in to.iter_mut().zip(other) {
        *left ^= *right;
    }
}

/// Exact column-elimination over GF(2): basis columns carry provenance
/// bitsets, hence the target's zero reduction yields learned source indices.
/// Verify the result against the *entire* independent TRAIN prefix afterwards.
fn solve_gauss(o: &Observed) -> Option<Vec<usize>> {
    if o.p.len() < GAUSS_STEPS || o.sources.len() > 128 {
        return None;
    }
    let repwords = o.sources.len().div_ceil(64);
    let mut pivot = vec![None::<(Vec<u64>, Vec<u64>)>; GAUSS_STEPS * LANES];
    for (index, column) in o.columns.iter().enumerate() {
        let mut bits = column[..GAUSS_STEPS].to_vec();
        let mut rep = vec![0u64; repwords];
        rep[index / 64] |= 1u64 << (index % 64);
        while let Some(row) = lowest_bit(&bits) {
            if let Some((basis, coordinates)) = &pivot[row] {
                xor_assign(&mut bits, basis);
                xor_assign(&mut rep, coordinates);
            } else {
                pivot[row] = Some((bits, rep));
                break;
            }
        }
    }
    let mut target = o.p[..GAUSS_STEPS].to_vec();
    let mut rep = vec![0u64; repwords];
    while let Some(row) = lowest_bit(&target) {
        if let Some((basis, coordinates)) = &pivot[row] {
            xor_assign(&mut target, basis);
            xor_assign(&mut rep, coordinates);
        } else {
            return None;
        }
    }
    let selected = (0..o.sources.len())
        .filter(|&i| rep[i / 64] & (1u64 << (i % 64)) != 0)
        .collect::<Vec<_>>();
    // Postsolve verification using *all* proxy observations, not just 1024
    // equations. This rejects coincidental affine solutions or nonlinear tasks.
    if o.p.iter().enumerate().any(|(t, &p)| {
        selected.iter().fold(p, |value, &i| value ^ o.columns[i][t]) != 0
    }) {
        return None;
    }
    Some(selected)
}
fn triplet(o: &Observed, ids: [usize; 3]) -> [Source; 3] {
    ids.map(|index| o.sources[index])
}
fn pair_extension(o: &Observed, scores: &[PairScore]) -> Vec<[Source; 3]> {
    let mut out = BTreeSet::new();
    for pair in scores.iter().take(TOP_PAIRS) {
        for k in 0..o.sources.len() {
            if o.same_gate(pair.ids[0], k) || o.same_gate(pair.ids[1], k) {
                continue;
            }
            let mut ids = [pair.ids[0], pair.ids[1], k];
            ids.sort_unstable();
            out.insert(triplet(o, ids));
        }
    }
    out.into_iter().collect()
}
fn triplet_candidates(o: &Observed, scores: &[TripleScore]) -> Vec<[Source; 3]> {
    scores
        .iter()
        .take(TOP_TRIPLES)
        .map(|score| triplet(o, score.ids))
        .collect()
}
fn count_combinations(o: &Observed) -> (usize, usize) {
    let p = o.sources.len() / 4;
    (p * (p - 1) / 2 * 16, p * (p - 1) * (p - 2) / 6 * 64)
}

struct Frozen {
    task: Task,
    panel: Panel,
    seed: usize,
    n: usize,
    method: Method,
    status: &'static str,
    proposals: usize,
    rank_true: usize,
    score_seconds: f64,
    factor_seconds: f64,
    pair_rank: usize,
    full_pairs: usize,
    full_triples: usize,
    fit: Option<BudgetFit>,
}
fn freeze(
    task: Task,
    panel: Panel,
    seed: usize,
    n: usize,
    method: Method,
    train: &LaneData,
    validation: &LaneData,
    o: &Observed,
) -> Result<Frozen, Box<dyn Error>> {
    let expected = task.sources();
    let (full_pairs, full_triples) = count_combinations(o);
    let start = Instant::now();
    let mut rank_true = 0usize;
    let mut pair_rank = 0usize;
    let (status, proposed): (&'static str, Vec<[Source; 3]>) = match method {
        Method::GaussExact => {
            if panel == Panel::Noise {
                ("SKIPPED_NOISY", Vec::new())
            } else if let Some(found) = solve_gauss(o) {
                if found.len() == 3 {
                    let sources = triplet(o, [found[0], found[1], found[2]]);
                    if sources[0].gate < sources[1].gate
                        && sources[1].gate < sources[2].gate
                    {
                        ("SOLVED", vec![sources])
                    } else {
                        ("NO_THREE_DISTINCT_GATES", Vec::new())
                    }
                } else {
                    ("NO_EXACT_THREE_SOURCE_SOLUTION", Vec::new())
                }
            } else {
                ("NO_EXACT_GF2_SOLUTION", Vec::new())
            }
        }
        Method::Walsh2Pair => {
            let ranked = rank_pairs(o);
            if task == Task::AndXorXor {
                pair_rank = ranked
                    .iter()
                    .position(|pair| {
                        o.sources[pair.ids[0]] == expected[1]
                            && o.sources[pair.ids[1]] == expected[2]
                    })
                    .map_or(0, |index| index + 1);
            }
            ("SCREENED", pair_extension(o, &ranked))
        }
        Method::Walsh3Exhaustive => {
            let ranked = rank_triples(o);
            rank_true = ranked
                .iter()
                .position(|candidate| triplet(o, candidate.ids) == expected)
                .map_or(0, |index| index + 1);
            ("SCREENED", triplet_candidates(o, &ranked))
        }
    };
    let score_seconds = start.elapsed().as_secs_f64();
    let factor_start = Instant::now();
    let fit = if proposed.is_empty() {
        None
    } else {
        Some(fit_three_gate_candidates(
            train,
            validation,
            WIDTH,
            &proposed,
        )?)
    };
    let factor_seconds = factor_start.elapsed().as_secs_f64();
    println!(
        "A6C12B_FROZEN,task={},panel={},seed={},n={},method={},status={},pair_rank={},triplet_rank={},true_triplet_proposed={},pair_screens={},triple_screens={},factorized_structures={},score_seconds={:.6},factor_seconds={:.6},proposal_sources={:?},proposal_truths={:?},accepted={}",
        task.name(),panel.name(),seed,n,method.name(),status,pair_rank,rank_true,
        proposed.iter().any(|&sources| sources == expected),full_pairs,full_triples,
        proposed.len(),score_seconds,factor_seconds,
        fit.as_ref().map(|x|x.proposal_sources),
        fit.as_ref().map(|x|x.proposal_rules.rules.map(|r| r.truth)),
        fit.as_ref().map(|x|x.accepted).unwrap_or(false)
    );
    Ok(Frozen {
        task,
        panel,
        seed,
        n,
        method,
        status,
        proposals: proposed.len(),
        rank_true,
        score_seconds,
        factor_seconds,
        pair_rank,
        full_pairs,
        full_triples,
        fit,
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() > 2 {
        return Err("usage: sml-gf2-walsh23-bench [seeds:1..=3]".into());
    }
    let nseeds: usize = args.get(1).map_or(Ok(3), |arg| arg.parse())?;
    if !(1..=3).contains(&nseeds) {
        return Err("seed count must be 1..=3".into());
    }
    println!(
        "A6C12B_CONFIG,seeds={nseeds},tasks=xor_xor_xor;and_xor_xor;xor_and_xor;and_and_and,panels=clean;noise_1_in_16,n=32;64;128,train=512,val=512,test=4096,lanes=64,gauss_samples=1024,top_pairs=8,top_triples=16"
    );
    println!(
        "A6C12B_PROTOCOL,native_oracle=true,teacher_rules_not_passed_to_screening=true,full_screen_counts=true,noise_gf2_skipped=true,all_frozen_before_test=true"
    );
    let mut frozen = Vec::with_capacity(4 * 2 * nseeds * 3 * 3);
    for task in Task::ALL {
        for panel in Panel::ALL {
            for index in 0..nseeds {
                let train = data(seed(index, task, panel, 0xa6c1_2b11), task, panel, TRAIN_STEPS);
                let validation = data(seed(index, task, panel, 0xa6c1_2b22), task, panel, VAL_STEPS);
                for n in NS {
                    let o = Observed::new(&train, grid(n));
                    assert_eq!(o.p.len(), TRAIN_STEPS - WIDTH);
                    assert!(task.sources().iter().all(|source| o.sources.contains(source)));
                    let (pairs, triples) = count_combinations(&o);
                    println!(
                        "A6C12B_GRID,n={},pairs={},triples={},samples={},raw_candidates={}",
                        n,pairs,triples,o.p.len()*LANES,o.sources.len()
                    );
                    for method in Method::ALL {
                        frozen.push(freeze(task, panel, index, n, method, &train, &validation, &o)?);
                    }
                }
            }
        }
    }
    let expected_attempts = 4 * 2 * nseeds * NS.len() * Method::ALL.len();
    if frozen.len() != expected_attempts {
        return Err("frozen attempt census mismatch".into());
    }
    println!("A6C12B_ALL_FROZEN,attempts={expected_attempts},streams={}", 4*2*nseeds);
    // Create independent TEST streams only after *every* policy is frozen.
    for task in Task::ALL {
        for panel in Panel::ALL {
            for index in 0..nseeds {
                let test = data(seed(index, task, panel, 0xa6c1_2b33), task, panel, TEST_STEPS);
                let teacher = make_multi_gate_native(WIDTH, &task.rules())?;
                let oracle_err = native_errors(&teacher, &test)?;
                for result in frozen
                    .iter()
                    .filter(|r| r.task == task && r.panel == panel && r.seed == index)
                {
                    let (sources_exact, truth_exact, proposed_err, selected_err) =
                        if let Some(fit) = &result.fit {
                            (
                                fit.proposal_sources == task.sources(),
                                fit.proposal_rules.rules.map(|r| r.truth) == task.truths(),
                                Some(native_errors(&fit.proposal, &test)?),
                                Some(native_errors(&fit.committed, &test)?),
                            )
                        } else {
                            (false, false, None, None)
                        };
                    println!(
                        "A6C12B_TEST,task={},panel={},seed={},n={},method={},status={},samples={},oracle_errors={},proposal_errors={:?},selected_errors={:?},sources_exact={},truths_exact={},accepted={},screened_pairs={},screened_triples={},triplet_rank={},pair_rank={},factor_structures={},screen_seconds={:.6},factor_seconds={:.6}",
                        task.name(),panel.name(),index,result.n,result.method.name(),result.status,
                        TEST_STEPS*LANES,oracle_err,proposed_err,selected_err,sources_exact,truth_exact,
                        result.fit.as_ref().is_some_and(|x|x.accepted),
                        result.full_pairs,result.full_triples,result.rank_true,result.pair_rank,
                        result.proposals,result.score_seconds,result.factor_seconds
                    );
                }
            }
        }
    }
    println!(
        "A6C12B_COMPLETE,attempts={},independent_test_streams={},methods=3,grid_sizes=3",
        frozen.len(),4*2*nseeds
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_grid_admissible_hypotheses_and_teacher_coverage() {
        let expected = [(32, 448, 3_584), (64, 1_920, 35_840), (128, 7_936, 317_440)];
        for (n, pairs, triples) in expected {
            let observed = Observed::new(
                &data(21, Task::XorXorXor, Panel::Clean, TRAIN_STEPS),
                grid(n),
            );
            assert_eq!(count_combinations(&observed), (pairs, triples));
            assert!(Task::XorXorXor
                .sources()
                .iter()
                .all(|source| observed.sources.contains(source)));
        }
    }

    #[test]
    fn clean_parity_is_exactly_solved_and_mixed_is_not_exactly_affine() {
        for n in NS {
            let o = Observed::new(
                &data(24, Task::XorXorXor, Panel::Clean, TRAIN_STEPS),
                grid(n),
            );
            let result = solve_gauss(&o).expect("exact clean XOR must be solvable");
            assert_eq!(result.len(), 3);
            let mut found = result.iter().map(|&i| o.sources[i]).collect::<Vec<_>>();
            found.sort_unstable();
            assert_eq!(found, Task::XorXorXor.sources());

            let mixed = Observed::new(
                &data(26, Task::AndXorXor, Panel::Clean, TRAIN_STEPS),
                grid(n),
            );
            assert!(solve_gauss(&mixed).is_none());
        }
    }

    #[test]
    fn both_orders_rank_true_adversarial_features_when_exhaustive() {
        let o = Observed::new(
            &data(27, Task::AndXorXor, Panel::Clean, TRAIN_STEPS),
            grid(32),
        );
        let pairs = rank_pairs(&o);
        assert_eq!(pairs.len(), 448);
        assert!(pairs.iter().any(|pair| {
            o.sources[pair.ids[0]] == Task::AndXorXor.sources()[1]
                && o.sources[pair.ids[1]] == Task::AndXorXor.sources()[2]
        }));
        let triples = rank_triples(&o);
        assert_eq!(triples.len(), 3584);
        assert!(triples.iter().any(|x| triplet(&o, x.ids) == Task::AndXorXor.sources()));
    }
}
