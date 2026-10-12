//! A6-I5 preregistered passive strength-three support / strength-four blind-spot bench.
//! TEST data is generated only after all 90 TRAIN committees/guards are frozen.
//! The benchmark alone knows hidden role positions and true teacher functions.

use sml_core::{
    anf_pair_support::AnfPairSupportGuard,
    anf_passive_committee::PassiveCommittee,
    anf_triple_support::AnfTripleSupportGuard,
    anf_wide_sparse::{WideAnfGate, WideAnfLimits},
    toy_tasks::SplitMix64,
};
use std::error::Error;

const WIDTHS: [usize; 5] = [8, 16, 32, 64, 128];
const TRAIN: usize = 512;
const TEST: usize = 512;
const MAX_TRIPLES: usize = 350_000;
const MAX_MISSING: usize = 100_000;
const CONFIDENCE: f64 = 0.90;
const MAX_MISMATCH: f64 = 0.125;
const ROLE_SALT: u64 = 0xa615_0001_51ed_0001;
const TRAIN_SALT: u64 = 0xa615_0002_51ed_0002;
const NOISE_SALT: u64 = 0xa615_0003_51ed_0003;
const TEST_SALT: u64 = 0xa615_0004_51ed_0004;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scenario {
    TripleAbsent,
    TripleIncluded,
    QuadAbsent,
}

impl Scenario {
    fn all() -> [Self; 3] {
        [Self::TripleAbsent, Self::TripleIncluded, Self::QuadAbsent]
    }

    fn family(self) -> usize {
        if self == Self::QuadAbsent {
            4
        } else {
            3
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::TripleAbsent => "TRIPLE_ABSENT",
            Self::TripleIncluded => "TRIPLE_INCLUDED",
            Self::QuadAbsent => "QUAD_ABSENT",
        }
    }
}

struct Frozen {
    width: usize,
    seed_id: usize,
    noisy: bool,
    scenario: Scenario,
    roles: [usize; 8],
    teacher: WideAnfGate,
    committee: PassiveCommittee,
    pair: AnfPairSupportGuard,
    triple: AnfTripleSupportGuard,
    train_flips: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Metrics {
    tests: usize,
    committee_accepted: usize,
    committee_errors: usize,
    pair_accepted: usize,
    pair_errors: usize,
    triple_accepted: usize,
    triple_errors: usize,
    novel_pairs: usize,
    novel_triples: usize,
    false_abstentions: usize,
    triggers: usize,
}

impl Metrics {
    fn add(&mut self, x: Self) {
        self.tests += x.tests;
        self.committee_accepted += x.committee_accepted;
        self.committee_errors += x.committee_errors;
        self.pair_accepted += x.pair_accepted;
        self.pair_errors += x.pair_errors;
        self.triple_accepted += x.triple_accepted;
        self.triple_errors += x.triple_errors;
        self.novel_pairs += x.novel_pairs;
        self.novel_triples += x.novel_triples;
        self.false_abstentions += x.false_abstentions;
        self.triggers += x.triggers;
    }
}

fn seed(width: usize, index: usize, family: usize, salt: u64) -> u64 {
    (width as u64).wrapping_mul(0xd134_2543_de82_ef95)
        ^ ((index + 1) as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ ((family + 1) as u64).wrapping_mul(0x94d0_49bb_1331_11eb)
        ^ salt
}

fn roles(width: usize, index: usize) -> [usize; 8] {
    let mut rng = SplitMix64::new(seed(width, index, 0, ROLE_SALT));
    let mut positions = (0..width).collect::<Vec<_>>();
    for i in (1..width).rev() {
        let j = rng.below(i + 1);
        positions.swap(i, j);
    }
    positions[..8].try_into().expect("predeclared width >=8")
}

fn gate(width: usize, mut terms: Vec<Vec<usize>>) -> WideAnfGate {
    for term in &mut terms {
        term.sort_unstable();
    }
    WideAnfGate::from_monomials(
        (0..width).collect(),
        terms,
        width,
        WideAnfLimits::new(2048, 256, 8192, 8192, 8192),
    )
    .expect("unique preregistered roles and positive monomials")
}

fn bank(width: usize, r: &[usize; 8]) -> Vec<WideAnfGate> {
    [
        vec![vec![r[2]], vec![r[3]]],
        vec![vec![r[2]], vec![r[3]], vec![r[0], r[1]]],
        vec![vec![r[2]], vec![r[3]], vec![r[0]]],
        vec![vec![r[2]], vec![r[3]], vec![r[1]]],
    ]
    .into_iter()
    .map(|terms| gate(width, terms))
    .collect()
}

fn teacher(width: usize, r: &[usize; 8], scenario: Scenario) -> WideAnfGate {
    let mut terms = vec![vec![r[2]], vec![r[3]]];
    match scenario {
        Scenario::TripleIncluded => {}
        Scenario::TripleAbsent => terms.push(vec![r[4], r[5], r[6]]),
        Scenario::QuadAbsent => terms.push(vec![r[4], r[5], r[6], r[7]]),
    }
    gate(width, terms)
}

fn input_for(
    rng: &mut SplitMix64,
    width: usize,
    r: &[usize; 8],
    scenario: Scenario,
    step: usize,
    train: bool,
) -> Vec<bool> {
    let mut input = (0..width)
        .map(|_| rng.next_u64() & 1 != 0)
        .collect::<Vec<_>>();
    let family = scenario.family();
    if train {
        // Enumerate all except the final all-ones tuple, in a fixed order.
        let pattern = step % ((1 << family) - 1);
        for j in 0..family {
            input[r[4 + j]] = pattern & (1 << j) != 0;
        }
    } else if step % 2 == 0 {
        input[r[4]] = false;
    } else {
        for j in 0..family {
            input[r[4 + j]] = true;
        }
    }
    input
}

fn fit_one(
    width: usize,
    seed_id: usize,
    scenario: Scenario,
    noisy: bool,
) -> Result<Frozen, Box<dyn Error>> {
    let r = roles(width, seed_id);
    let truth = teacher(width, &r, scenario);
    let mut committee =
        PassiveCommittee::new(bank(width, &r), vec![4.0, 2.0, 2.0, 1.0], 1.0 / 16.0, TRAIN)?;
    let mut pair = AnfPairSupportGuard::new(width, TRAIN, 600_000, 8192)?;
    let mut triple = AnfTripleSupportGuard::new(width, TRAIN, MAX_TRIPLES, MAX_MISSING)?;
    let mut rng = SplitMix64::new(seed(width, seed_id, scenario.family(), TRAIN_SALT));
    let mut noise_rng = SplitMix64::new(seed(width, seed_id, scenario.family(), NOISE_SALT));
    let mut flips = 0usize;
    let mut seen_hidden_ones = false;
    let mut patterns = vec![false; (1 << scenario.family()) - 1];
    for step in 0..TRAIN {
        let input = input_for(&mut rng, width, &r, scenario, step, true);
        let mut pattern = 0usize;
        for j in 0..scenario.family() {
            pattern |= usize::from(input[r[4 + j]]) << j;
        }
        if pattern == (1 << scenario.family()) - 1 {
            seen_hidden_ones = true;
        } else {
            patterns[pattern] = true;
        }
        let flipped = noisy && noise_rng.next_u64() & 15 == 0;
        flips += usize::from(flipped);
        let label = truth.evaluate_scalar(&input)? ^ flipped;
        committee.observe(&input, label)?;
        pair.observe(&input)?;
        triple.observe(&input)?;
    }
    if seen_hidden_ones || patterns.contains(&false) {
        return Err("TRAIN censure did not cover every lower pattern and exclude all-ones".into());
    }
    pair.seal()?;
    triple.seal()?;
    if !committee.bank_adequate(MAX_MISMATCH)? {
        return Err(
            "TRAIN committee rejects supplied h0 although censored teacher equals h0".into(),
        );
    }
    println!(
        "A6_I5_FROZEN,width={},seed={},scenario={},noisy={},train={},flips={},missing_pairs={},missing_triples={},triple_payload_bytes={}",
        width,
        seed_id,
        scenario.label(),
        noisy,
        committee.samples(),
        flips,
        pair.missing_pair_count().unwrap_or_default(),
        triple.missing_triple_count().unwrap_or_default(),
        triple.logical_payload_bytes()
    );
    Ok(Frozen {
        width,
        seed_id,
        noisy,
        scenario,
        roles: r,
        teacher: truth,
        committee,
        pair,
        triple,
        train_flips: flips,
    })
}

fn test_one(frozen: &Frozen) -> Result<Metrics, Box<dyn Error>> {
    let mut rng = SplitMix64::new(seed(
        frozen.width,
        frozen.seed_id,
        frozen.scenario.family(),
        TEST_SALT,
    ));
    let mut metrics = Metrics::default();
    for step in 0..TEST {
        let input = input_for(
            &mut rng,
            frozen.width,
            &frozen.roles,
            frozen.scenario,
            step,
            false,
        );
        let label = frozen.teacher.evaluate_scalar(&input)?;
        let is_trigger = step % 2 == 1;
        let pair =
            frozen
                .pair
                .predict_guarded(&frozen.committee, &input, CONFIDENCE, MAX_MISMATCH)?;
        let triple =
            frozen
                .triple
                .predict_guarded(&frozen.committee, &input, CONFIDENCE, MAX_MISMATCH)?;
        if triple.committee.selective_prediction != pair.committee.selective_prediction {
            return Err("pair/triple baseline committees diverged".into());
        }
        let truth_in_bank = frozen.scenario == Scenario::TripleIncluded;
        metrics.tests += 1;
        metrics.triggers += usize::from(is_trigger);
        metrics.novel_pairs += usize::from(!pair.pair.pairwise_supported);
        metrics.novel_triples += usize::from(!triple.triple.triplewise_supported);
        if let Some(pred) = pair.committee.selective_prediction {
            metrics.committee_accepted += 1;
            metrics.committee_errors += usize::from(pred != label);
        }
        if let Some(pred) = pair.guarded_prediction {
            metrics.pair_accepted += 1;
            metrics.pair_errors += usize::from(pred != label);
        }
        if let Some(pred) = triple.guarded_prediction {
            metrics.triple_accepted += 1;
            metrics.triple_errors += usize::from(pred != label);
        } else if truth_in_bank && pair.committee.selective_prediction == Some(label) {
            metrics.false_abstentions += 1;
        }
    }
    println!(
        "A6_I5_TEST,width={},seed={},scenario={},noisy={},train_flips={},tests={},triple_accepted={},triple_errors={},novel_triples={},pair_accepted={},pair_errors={}",
        frozen.width,
        frozen.seed_id,
        frozen.scenario.label(),
        frozen.noisy,
        frozen.train_flips,
        metrics.tests,
        metrics.triple_accepted,
        metrics.triple_errors,
        metrics.novel_triples,
        metrics.pair_accepted,
        metrics.pair_errors
    );
    Ok(metrics)
}

fn verify(scenario: Scenario, m: Metrics) -> Result<(), Box<dyn Error>> {
    let expected = match scenario {
        Scenario::TripleAbsent => Metrics {
            tests: 15360,
            committee_accepted: 15360,
            committee_errors: 7680,
            pair_accepted: 15360,
            pair_errors: 7680,
            triple_accepted: 7680,
            triple_errors: 0,
            novel_pairs: 0,
            novel_triples: 7680,
            false_abstentions: 0,
            triggers: 7680,
        },
        Scenario::TripleIncluded => Metrics {
            tests: 15360,
            committee_accepted: 15360,
            committee_errors: 0,
            pair_accepted: 15360,
            pair_errors: 0,
            triple_accepted: 7680,
            triple_errors: 0,
            novel_pairs: 0,
            novel_triples: 7680,
            false_abstentions: 7680,
            triggers: 7680,
        },
        Scenario::QuadAbsent => Metrics {
            tests: 15360,
            committee_accepted: 15360,
            committee_errors: 7680,
            pair_accepted: 15360,
            pair_errors: 7680,
            triple_accepted: 15360,
            triple_errors: 7680,
            novel_pairs: 0,
            novel_triples: 0,
            false_abstentions: 0,
            triggers: 7680,
        },
    };
    if m != expected {
        return Err(format!(
            "predeclared {} metrics violated: actual={m:?}, expected={expected:?}",
            scenario.label()
        )
        .into());
    }
    Ok(())
}

fn run() -> Result<(), Box<dyn Error>> {
    // Each fit, including all TRAIN-only seals, happens before any TEST input.
    let mut frozen = Vec::new();
    for scenario in Scenario::all() {
        for width in WIDTHS {
            for seed_id in 0..3 {
                for noisy in [false, true] {
                    frozen.push(fit_one(width, seed_id, scenario, noisy)?);
                }
            }
        }
    }
    if frozen.len() != 90 {
        return Err("exactly 90 policies must be frozen before TEST".into());
    }
    println!("A6_I5_ALL_FROZEN,policies=90,widths=5,regimes=3,train=512");
    let mut totals = [Metrics::default(); 3];
    for fit in &frozen {
        let result = test_one(fit)?;
        let scenario_index = match fit.scenario {
            Scenario::TripleAbsent => 0,
            Scenario::TripleIncluded => 1,
            Scenario::QuadAbsent => 2,
        };
        totals[scenario_index].add(result);
    }
    for (scenario, m) in Scenario::all().into_iter().zip(totals) {
        println!(
            "A6_I5_SUMMARY,scenario={},tests={},committee_accepted={},committee_errors={},pair_accepted={},pair_errors={},triple_accepted={},triple_errors={},novel_pairs={},novel_triples={},false_abstentions={},triggers={}",
            scenario.label(),
            m.tests,
            m.committee_accepted,
            m.committee_errors,
            m.pair_accepted,
            m.pair_errors,
            m.triple_accepted,
            m.triple_errors,
            m.novel_pairs,
            m.novel_triples,
            m.false_abstentions,
            m.triggers
        );
        verify(scenario, m)?;
    }
    if AnfTripleSupportGuard::new(256, TRAIN, MAX_TRIPLES, MAX_MISSING).is_ok()
        || AnfTripleSupportGuard::new(1024, TRAIN, MAX_TRIPLES, MAX_MISSING).is_ok()
    {
        return Err("combinatorial budget must reject full-width k256 and k1024".into());
    }
    println!(
        "A6_I5_COMPLETE,policies=90,tests=46080,triple_absent_protected=7680,triple_included_false_abstentions=7680,quad_confident_errors=7680,oversize_rejected=true,oracle=PASS"
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("A6_I5_FAILURE,{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_width_triple_slot_budget_must_reject_256_and_1024() {
        assert!(AnfTripleSupportGuard::new(128, TRAIN, MAX_TRIPLES, MAX_MISSING).is_ok());
        assert!(AnfTripleSupportGuard::new(256, TRAIN, MAX_TRIPLES, MAX_MISSING).is_err());
        assert!(AnfTripleSupportGuard::new(1024, TRAIN, MAX_TRIPLES, MAX_MISSING).is_err());
    }

    #[test]
    fn frozen_training_never_contains_hidden_all_ones() {
        for scenario in Scenario::all() {
            let roles = roles(8, 0);
            let mut rng = SplitMix64::new(seed(8, 0, scenario.family(), TRAIN_SALT));
            for step in 0..TRAIN {
                let input = input_for(&mut rng, 8, &roles, scenario, step, true);
                assert!(
                    !(0..scenario.family()).all(|j| input[roles[4 + j]]),
                    "TRAIN must not expose true hidden interaction"
                );
            }
        }
    }
}
