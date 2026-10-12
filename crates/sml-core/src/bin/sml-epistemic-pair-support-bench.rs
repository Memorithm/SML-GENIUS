//! A6-I4: preregistered pairwise support novelty vs unavoidable
//! third-order OOD failure. No future labels/hidden ANF parameters enter
//! the supplied passive Bayesian committee or its TRAIN-only guards.

use std::{error::Error, time::Instant};

use sml_core::{
    anf_marginal_support::AnfMarginalSupportGuard,
    anf_pair_support::AnfPairSupportGuard,
    anf_passive_committee::PassiveCommittee,
    anf_wide_sparse::{WideAnfGate, WideAnfLimits},
    toy_tasks::SplitMix64,
};

const WIDTHS: [usize; 7] = [8, 16, 32, 64, 128, 256, 1024];
const SEEDS: usize = 3;
const TRAIN_STEPS: usize = 512;
const TEST_STEPS: usize = 2048;
const HIDDEN_SALT: u64 = 0xa614_1001_51ed_0001;
const TRAIN_SALT: u64 = 0xa614_1002_51ed_0002;
const NOISE_SALT: u64 = 0xa614_1003_51ed_0003;
const TEST_SALT: u64 = 0xa614_1004_51ed_0004;
const SELECT_CONF: f64 = 0.90;
const BANK_ERROR_MAX: f64 = 0.125;
const MAX_PAIR_SLOTS: usize = 600_000;
const MAX_MISSING_PAIRS: usize = 8_192;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scenario {
    CensoredAbsent,
    PairCensoredAbsent,
    PairCensoredIncluded,
    TripleCensoredAbsent,
    FullIncluded,
    FullAbsent,
}
impl Scenario {
    const ALL: [Self; 6] = [
        Self::CensoredAbsent,
        Self::PairCensoredAbsent,
        Self::PairCensoredIncluded,
        Self::TripleCensoredAbsent,
        Self::FullIncluded,
        Self::FullAbsent,
    ];
    fn id(self) -> usize {
        match self {
            Self::CensoredAbsent => 0,
            Self::PairCensoredAbsent => 1,
            Self::PairCensoredIncluded => 2,
            Self::TripleCensoredAbsent => 3,
            Self::FullIncluded => 4,
            Self::FullAbsent => 5,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::CensoredAbsent => "CENSORED_ABSENT",
            Self::PairCensoredAbsent => "PAIR_CENSORED_ABSENT",
            Self::PairCensoredIncluded => "PAIR_CENSORED_INCLUDED",
            Self::TripleCensoredAbsent => "TRIPLE_CENSORED_ABSENT",
            Self::FullIncluded => "FULL_INCLUDED",
            Self::FullAbsent => "FULL_ABSENT",
        }
    }
    fn teacher_missing(self) -> bool {
        !matches!(self, Self::PairCensoredIncluded | Self::FullIncluded)
    }
    fn pair_censored(self) -> bool {
        matches!(self, Self::PairCensoredAbsent | Self::PairCensoredIncluded)
    }
    fn triple_censored(self) -> bool {
        self == Self::TripleCensoredAbsent
    }
}

fn seed(width: usize, index: usize, scenario: Scenario, salt: u64) -> u64 {
    (index as u64 + 1).wrapping_mul(0xd134_2543_de82_ef95)
        ^ (scenario.id() as u64 + 1).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ width as u64
        ^ salt
}
fn permute_roles(width: usize, index: usize) -> [usize; 8] {
    let mut positions = (0..width).collect::<Vec<_>>();
    let mut rng = SplitMix64::new(seed(width, index, Scenario::CensoredAbsent, HIDDEN_SALT));
    for i in (1..width).rev() {
        let j = rng.below(i + 1);
        positions.swap(i, j);
    }
    positions[..8].try_into().expect("preregistered width >=8")
}

fn gate(k: usize, mut terms: Vec<Vec<usize>>) -> WideAnfGate {
    for monomial in &mut terms {
        monomial.sort_unstable();
    }
    WideAnfGate::from_monomials(
        (0..k).collect(),
        terms,
        k,
        WideAnfLimits::new(2048, 256, 8192, 8192, 8192),
    )
    .expect("preregistered unique ANF terms")
}
fn candidates(width: usize, roles: &[usize; 8]) -> Vec<WideAnfGate> {
    [
        vec![vec![roles[2]], vec![roles[3]]],
        vec![vec![roles[2]], vec![roles[3]], vec![roles[0], roles[1]]],
        vec![vec![roles[2]], vec![roles[3]], vec![roles[4], roles[5]]],
        vec![
            vec![roles[2]],
            vec![roles[3]],
            vec![roles[0], roles[1]],
            vec![roles[4], roles[5]],
        ],
    ]
    .into_iter()
    .map(|terms| gate(width, terms))
    .collect()
}
fn absent_teacher(width: usize, roles: &[usize; 8], triple: bool) -> WideAnfGate {
    let missing = if triple {
        vec![roles[5], roles[6], roles[7]]
    } else {
        vec![roles[6], roles[7]]
    };
    gate(width, vec![vec![roles[2]], vec![roles[3]], missing])
}

fn train_input(
    rng: &mut SplitMix64,
    width: usize,
    roles: &[usize; 8],
    scenario: Scenario,
    step: usize,
) -> Vec<bool> {
    let mut input = (0..width)
        .map(|_| rng.next_u64() & 1 != 0)
        .collect::<Vec<_>>();
    if scenario == Scenario::CensoredAbsent {
        input[roles[6]] = false;
    } else if scenario.pair_censored() {
        let (a, b) = match step % 3 {
            0 => (false, false),
            1 => (true, false),
            _ => (false, true),
        };
        input[roles[6]] = a;
        input[roles[7]] = b;
    } else if scenario.triple_censored() {
        // All 7 assignments except 111 appear every 7 samples. Therefore
        // each of the 3 PAIRS sees all 4 Boolean configurations.
        let bits = step % 7;
        input[roles[5]] = bits & 4 != 0;
        input[roles[6]] = bits & 2 != 0;
        input[roles[7]] = bits & 1 != 0;
    }
    input
}

fn test_input(
    rng: &mut SplitMix64,
    width: usize,
    roles: &[usize; 8],
    scenario: Scenario,
    step: usize,
) -> Vec<bool> {
    let mut input = (0..width)
        .map(|_| rng.next_u64() & 1 != 0)
        .collect::<Vec<_>>();
    if scenario.triple_censored() {
        if step % 2 == 0 {
            input[roles[5]] = false;
        } else {
            input[roles[5]] = true;
            input[roles[6]] = true;
            input[roles[7]] = true;
        }
    } else if step % 2 == 0 {
        input[roles[6]] = false;
    } else {
        input[roles[6]] = true;
        input[roles[7]] = true;
    }
    input
}

#[derive(Clone)]
struct Frozen {
    width: usize,
    index: usize,
    scenario: Scenario,
    noisy: bool,
    roles: [usize; 8],
    teacher: WideAnfGate,
    marginal: AnfMarginalSupportGuard,
    pair: AnfPairSupportGuard,
    committee: PassiveCommittee,
    flips: usize,
    bank_adequate: bool,
    classes: usize,
    seal_ms: f64,
}

fn fit_one(
    width: usize,
    index: usize,
    scenario: Scenario,
    noisy: bool,
) -> Result<Frozen, Box<dyn Error>> {
    let roles = permute_roles(width, index);
    let hypotheses = candidates(width, &roles);
    let teacher = if scenario.teacher_missing() {
        absent_teacher(width, &roles, scenario.triple_censored())
    } else {
        hypotheses[0].clone()
    };
    let mut committee = PassiveCommittee::new(
        hypotheses,
        vec![4.0, 2.0, 2.0, 1.0],
        1.0 / 16.0,
        TRAIN_STEPS,
    )?;
    let mut marginal = AnfMarginalSupportGuard::new(width, TRAIN_STEPS)?;
    let mut pair =
        AnfPairSupportGuard::new(width, TRAIN_STEPS, MAX_PAIR_SLOTS, MAX_MISSING_PAIRS)?;
    let mut rng = SplitMix64::new(seed(width, index, scenario, TRAIN_SALT));
    let mut noise_rng = SplitMix64::new(seed(width, index, scenario, NOISE_SALT));
    let mut flips = 0usize;
    let mut seen_pair11 = false;
    let mut seen_triple111 = false;
    let mut seen_pair6 = [false; 2];
    let mut seen_pair7 = [false; 2];
    // Pair-triple pattern coverage measured independently of pair guard.
    let mut seen_triple_pair = [[false; 4]; 3];
    for step in 0..TRAIN_STEPS {
        let input = train_input(&mut rng, width, &roles, scenario, step);
        seen_pair11 |= input[roles[6]] && input[roles[7]];
        seen_triple111 |= input[roles[5]] && input[roles[6]] && input[roles[7]];
        seen_pair6[usize::from(input[roles[6]])] = true;
        seen_pair7[usize::from(input[roles[7]])] = true;
        for (slot, (a, b)) in [(5usize, 6usize), (5, 7), (6, 7)].into_iter().enumerate() {
            let pattern = usize::from(input[roles[a]]) * 2 + usize::from(input[roles[b]]);
            seen_triple_pair[slot][pattern] = true;
        }
        let flip = noisy && noise_rng.next_u64() & 15 == 0;
        flips += usize::from(flip);
        let truth = teacher.evaluate_scalar(&input)?;
        committee.observe(&input, truth ^ flip)?;
        marginal.observe(&input)?;
        pair.observe(&input)?;
    }
    if scenario == Scenario::CensoredAbsent && seen_pair6[1] {
        return Err("single censor leaked r6=1 during TRAIN".into());
    }
    if scenario.pair_censored()
        && (seen_pair11 || seen_pair6 != [true, true] || seen_pair7 != [true, true])
    {
        return Err("pair censor failed to show both marginals without joint 11".into());
    }
    if scenario.triple_censored()
        && (seen_triple111 || seen_triple_pair.iter().any(|patterns| patterns.contains(&false)))
    {
        return Err("triple negative did not hide 111 while covering EVERY pair".into());
    }
    if matches!(scenario, Scenario::FullIncluded | Scenario::FullAbsent) && !seen_pair11 {
        return Err("FULL TRAIN did not activate missing interaction".into());
    }
    let now = Instant::now();
    pair.seal()?;
    let seal_ms = now.elapsed().as_secs_f64() * 1000.0;
    let bank_adequate = committee.bank_adequate(BANK_ERROR_MAX)?;
    let classes = committee.observed_equivalence_classes();
    Ok(Frozen {
        width,
        index,
        scenario,
        noisy,
        roles,
        teacher,
        marginal,
        pair,
        committee,
        flips,
        bank_adequate,
        classes,
        seal_ms,
    })
}

#[derive(Clone, Default)]
struct Metrics {
    tests: usize,
    committee_accepted: usize,
    committee_errors: usize,
    marginal_accepted: usize,
    marginal_errors: usize,
    pair_accepted: usize,
    pair_errors: usize,
    marginal_novel_cases: usize,
    pair_novel_cases: usize,
    bank_inadequate: usize,
    eval_ms: f64,
}
impl Metrics {
    fn add(&mut self, other: &Self) {
        self.tests += other.tests;
        self.committee_accepted += other.committee_accepted;
        self.committee_errors += other.committee_errors;
        self.marginal_accepted += other.marginal_accepted;
        self.marginal_errors += other.marginal_errors;
        self.pair_accepted += other.pair_accepted;
        self.pair_errors += other.pair_errors;
        self.marginal_novel_cases += other.marginal_novel_cases;
        self.pair_novel_cases += other.pair_novel_cases;
        self.bank_inadequate += other.bank_inadequate;
        self.eval_ms += other.eval_ms;
    }
}

fn evaluate_one(frozen: &Frozen) -> Result<Metrics, Box<dyn Error>> {
    let mut rng = SplitMix64::new(seed(
        frozen.width,
        frozen.index,
        frozen.scenario,
        TEST_SALT,
    ));
    let mut m = Metrics::default();
    let started = Instant::now();
    for step in 0..TEST_STEPS {
        let input = test_input(
            &mut rng,
            frozen.width,
            &frozen.roles,
            frozen.scenario,
            step,
        );
        let truth = frozen.teacher.evaluate_scalar(&input)?;
        let pair = frozen.pair.predict_guarded(
            &frozen.committee,
            &input,
            SELECT_CONF,
            BANK_ERROR_MAX,
        )?;
        let marginal = frozen.marginal.assess(&input)?;
        if pair.bank_adequate != frozen.bank_adequate {
            return Err("bank adequacy changed between TRAIN and TEST".into());
        }
        let marginal_selected = if pair.bank_adequate && marginal.marginally_supported {
            pair.committee.selective_prediction
        } else {
            None
        };
        m.tests += 1;
        m.marginal_novel_cases += usize::from(!marginal.marginally_supported);
        m.pair_novel_cases += usize::from(!pair.pair.pairwise_supported);
        if let Some(prediction) = pair.committee.selective_prediction {
            m.committee_accepted += 1;
            m.committee_errors += usize::from(prediction != truth);
        }
        if let Some(prediction) = marginal_selected {
            m.marginal_accepted += 1;
            m.marginal_errors += usize::from(prediction != truth);
        }
        if let Some(prediction) = pair.guarded_prediction {
            m.pair_accepted += 1;
            m.pair_errors += usize::from(prediction != truth);
        }
    }
    m.eval_ms = started.elapsed().as_secs_f64() * 1000.0;
    m.bank_inadequate = usize::from(!frozen.bank_adequate);
    Ok(m)
}

fn correct_case(f: &Frozen, m: &Metrics) -> bool {
    let half = TEST_STEPS / 2;
    match f.scenario {
        Scenario::CensoredAbsent => {
            m.pair_novel_cases == half
                && m.pair_accepted == half
                && m.pair_errors == 0
                && m.committee_errors == half
                && m.marginal_novel_cases == half
                && f.bank_adequate
        }
        Scenario::PairCensoredAbsent => {
            m.pair_novel_cases == half
                && m.pair_accepted == half
                && m.pair_errors == 0
                && m.marginal_novel_cases == 0
                && m.marginal_accepted == TEST_STEPS
                && m.marginal_errors == half
                && m.committee_errors == half
                && f.bank_adequate
        }
        Scenario::PairCensoredIncluded => {
            m.pair_novel_cases == half
                && m.pair_accepted == half
                && m.pair_errors == 0
                && m.marginal_accepted == TEST_STEPS
                && m.marginal_errors == 0
                && m.committee_errors == 0
                && f.bank_adequate
        }
        Scenario::TripleCensoredAbsent => {
            m.pair_novel_cases == 0
                && m.pair_accepted == TEST_STEPS
                && m.pair_errors == half
                && m.marginal_novel_cases == 0
                && m.marginal_errors == half
                && m.committee_errors == half
                && f.bank_adequate
        }
        Scenario::FullIncluded => {
            m.pair_novel_cases == 0
                && m.pair_accepted == TEST_STEPS
                && m.pair_errors == 0
                && m.committee_errors == 0
                && f.bank_adequate
        }
        Scenario::FullAbsent => {
            m.pair_novel_cases == 0
                && m.pair_accepted == 0
                && m.pair_errors == 0
                && !f.bank_adequate
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    println!(
        "A6_I4_CONFIG,widths=8;16;32;64;128;256;1024,seeds=3,panels=clean;noise_1_in_16,regimes=CENSORED_ABSENT;PAIR_CENSORED_ABSENT;PAIR_CENSORED_INCLUDED;TRIPLE_CENSORED_ABSENT;FULL_INCLUDED;FULL_ABSENT,train=512,test=2048,confidence=0.90,bank_mismatch_threshold=0.125,max_pair_slots={MAX_PAIR_SLOTS},max_missing_pairs={MAX_MISSING_PAIRS}"
    );
    let mut frozen = Vec::new();
    for width in WIDTHS {
        for index in 0..SEEDS {
            for scenario in Scenario::ALL {
                for noisy in [false, true] {
                    let f = fit_one(width, index, scenario, noisy)?;
                    println!(
                        "A6_I4_FROZEN,width={},seed={},regime={},panel={},train_samples={},train_noise_flips={},observed_signature_classes={},bank_adequate={},missing_pairs={},pair_payload_bytes={},marginal_payload_bytes={},seal_ms={:.6},teacher_in_bank={}",
                        width,
                        index,
                        scenario.name(),
                        if noisy { "noise_1_in_16" } else { "clean" },
                        f.pair.samples(),
                        f.flips,
                        f.classes,
                        f.bank_adequate,
                        f.pair.missing_pair_count().ok_or("pair guard not sealed")?,
                        f.pair.logical_payload_bytes(),
                        f.marginal.logical_payload_bytes(),
                        f.seal_ms,
                        !scenario.teacher_missing(),
                    );
                    frozen.push(f);
                }
            }
        }
    }
    if frozen.len() != 252 {
        return Err("A6-I4 preregistered frozen policy count differs from 252".into());
    }
    println!("A6_I4_ALL_FROZEN,policies=252,paired_structures=126");

    // No future OOD TEST input is generated before the 252 policies and all
    // 252 pairwise support summaries/adequacy checks have been sealed.
    let mut summary = vec![Metrics::default(); Scenario::ALL.len()];
    let mut failed_cells = 0usize;
    for frozen in &frozen {
        let m = evaluate_one(frozen)?;
        summary[frozen.scenario.id()].add(&m);
        let pass = correct_case(frozen, &m);
        failed_cells += usize::from(!pass);
        println!(
            "A6_I4_TEST,width={},seed={},regime={},panel={},tests={},committee_accepted={},committee_errors={},marginal_accepted={},marginal_errors={},pair_accepted={},pair_errors={},marginal_novel_cases={},pair_novel_cases={},missing_pairs={},bank_adequate={},eval_ms={:.6},oracle={}",
            frozen.width,
            frozen.index,
            frozen.scenario.name(),
            if frozen.noisy { "noise_1_in_16" } else { "clean" },
            m.tests,
            m.committee_accepted,
            m.committee_errors,
            m.marginal_accepted,
            m.marginal_errors,
            m.pair_accepted,
            m.pair_errors,
            m.marginal_novel_cases,
            m.pair_novel_cases,
            frozen.pair.missing_pair_count().ok_or("unsealed during TEST")?,
            frozen.bank_adequate,
            m.eval_ms,
            if pass { "PASS" } else { "FAIL" },
        );
    }
    for scenario in Scenario::ALL {
        let m = &summary[scenario.id()];
        println!(
            "A6_I4_SUMMARY,regime={},policies=42,tests={},committee_accepted={},committee_errors={},marginal_accepted={},marginal_errors={},pair_accepted={},pair_errors={},marginal_novel_cases={},pair_novel_cases={},bank_inadequate_episodes={},eval_total_ms={:.6}",
            scenario.name(), m.tests, m.committee_accepted, m.committee_errors,
            m.marginal_accepted, m.marginal_errors, m.pair_accepted, m.pair_errors,
            m.marginal_novel_cases, m.pair_novel_cases, m.bank_inadequate, m.eval_ms,
        );
    }
    println!(
        "A6_I4_COMPLETE,policies=252,tests=516096,regimes=6,preregistered_failures={failed_cells}"
    );
    if failed_cells != 0 {
        return Err("A6-I4 pairwise positive AND triple negative criteria were violated".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn censure_pair_is_real_and_triple111_hidden_with_pairs_present() {
        for width in WIDTHS {
            let roles = permute_roles(width, 0);
            let mut rng = SplitMix64::new(456);
            let mut seen_pair = [false; 4];
            let mut seen_triple_pair = [[false; 4]; 3];
            for step in 0..TRAIN_STEPS {
                let p = train_input(&mut rng, width, &roles, Scenario::PairCensoredAbsent, step);
                seen_pair[usize::from(p[roles[6]]) * 2 + usize::from(p[roles[7]])] = true;
                let x = train_input(&mut rng, width, &roles, Scenario::TripleCensoredAbsent, step);
                assert!(!(x[roles[5]] && x[roles[6]] && x[roles[7]]));
                for (slot, (a, b)) in [(5usize, 6usize), (5, 7), (6, 7)].into_iter().enumerate()
                {
                    seen_triple_pair[slot]
                        [usize::from(x[roles[a]]) * 2 + usize::from(x[roles[b]])] = true;
                }
            }
            assert_eq!(seen_pair, [true, true, true, false]);
            assert!(seen_triple_pair.iter().all(|seen| *seen == [true; 4]));
        }
    }

    #[test]
    fn real_pair_vs_triple_ood_teacher_contrast_is_bit_exact() {
        for width in WIDTHS {
            let roles = permute_roles(width, 1);
            let base = candidates(width, &roles).remove(0);
            let pair = absent_teacher(width, &roles, false);
            let triple = absent_teacher(width, &roles, true);
            let mut all = vec![false; width];
            all[roles[5]] = true;
            all[roles[6]] = true;
            all[roles[7]] = true;
            assert_ne!(base.evaluate_scalar(&all), pair.evaluate_scalar(&all));
            assert_ne!(base.evaluate_scalar(&all), triple.evaluate_scalar(&all));
            all[roles[5]] = false;
            assert_eq!(base.evaluate_scalar(&all), triple.evaluate_scalar(&all));
        }
    }
}
