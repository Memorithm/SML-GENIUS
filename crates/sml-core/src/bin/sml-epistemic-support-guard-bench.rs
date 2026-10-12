//! A6-I3: preregistered empirical support envelope, confidence and
//! exact pairwise-underdetermination counterexample.
//! Frozen PASSIVE TRAIN before any OOD TEST generation. No learned teacher
//! ID, future labels or target-missing-term representation enters the guards.

use std::error::Error;

use sml_core::{
    anf_marginal_support::AnfMarginalSupportGuard,
    anf_passive_committee::PassiveCommittee,
    anf_wide_sparse::{WideAnfGate, WideAnfLimits},
    toy_tasks::SplitMix64,
};

const WIDTHS: [usize; 7] = [8, 16, 32, 64, 128, 256, 1024];
const SEEDS: usize = 3;
const TRAIN_STEPS: usize = 512;
const TEST_STEPS: usize = 2048;
const HIDDEN_SALT: u64 = 0xA613_1001_51ED_0001;
const TRAIN_SALT: u64 = 0xA613_1002_51ED_0002;
const NOISE_SALT: u64 = 0xA613_1003_51ED_0003;
const TEST_SALT: u64 = 0xA613_1004_51ED_0004;
const SELECT_CONF: f64 = 0.90;
const BANK_ERROR_MAX: f64 = 0.125;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scenario {
    CensoredAbsent,
    CensoredIncluded,
    PairCensoredAbsent,
    FullIncluded,
    FullAbsent,
}
impl Scenario {
    const ALL: [Self; 5] = [
        Self::CensoredAbsent,
        Self::CensoredIncluded,
        Self::PairCensoredAbsent,
        Self::FullIncluded,
        Self::FullAbsent,
    ];
    fn id(self) -> usize {
        match self {
            Self::CensoredAbsent => 0,
            Self::CensoredIncluded => 1,
            Self::PairCensoredAbsent => 2,
            Self::FullIncluded => 3,
            Self::FullAbsent => 4,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::CensoredAbsent => "CENSORED_ABSENT",
            Self::CensoredIncluded => "CENSORED_INCLUDED",
            Self::PairCensoredAbsent => "PAIR_CENSORED_ABSENT",
            Self::FullIncluded => "FULL_INCLUDED",
            Self::FullAbsent => "FULL_ABSENT",
        }
    }
    fn teacher_missing(self) -> bool {
        matches!(
            self,
            Self::CensoredAbsent | Self::PairCensoredAbsent | Self::FullAbsent
        )
    }
    fn single_censored(self) -> bool {
        matches!(self, Self::CensoredAbsent | Self::CensoredIncluded)
    }
    fn pair_censored(self) -> bool {
        self == Self::PairCensoredAbsent
    }
}

fn seed(width: usize, index: usize, scenario: Scenario, salt: u64) -> u64 {
    (index as u64 + 1).wrapping_mul(0xd134_2543_de82_ef95)
        ^ (scenario.id() as u64 + 1).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ width as u64
        ^ salt
}

/// The eight role positions are fixed per width/seed and hidden from the
/// guard. They are available in the SUPPLIED BANK hypotheses.
fn permute_roles(width: usize, index: usize) -> [usize; 8] {
    let mut positions = (0..width).collect::<Vec<_>>();
    let mut rng = SplitMix64::new(seed(width, index, Scenario::CensoredAbsent, HIDDEN_SALT));
    for i in (1..width).rev() {
        let j = rng.below(i + 1);
        positions.swap(i, j);
    }
    positions[..8].try_into().expect("k>=8")
}

fn gate(k: usize, mut supports: Vec<Vec<usize>>) -> WideAnfGate {
    for term in &mut supports {
        term.sort_unstable();
    }
    WideAnfGate::from_monomials(
        (0..k).collect(),
        supports,
        k,
        WideAnfLimits::new(2048, 256, 8192, 8192, 8192),
    )
    .expect("preregistered ANF masks are unique, in range and bounded")
}

fn candidates(width: usize, r: &[usize; 8]) -> Vec<WideAnfGate> {
    [
        vec![vec![r[2]], vec![r[3]]],
        vec![vec![r[2]], vec![r[3]], vec![r[0], r[1]]],
        vec![vec![r[2]], vec![r[3]], vec![r[4], r[5]]],
        vec![vec![r[2]], vec![r[3]], vec![r[0], r[1]], vec![r[4], r[5]]],
    ]
    .into_iter()
    .map(|terms| gate(width, terms))
    .collect()
}

fn absent_teacher(width: usize, r: &[usize; 8]) -> WideAnfGate {
    gate(width, vec![vec![r[2]], vec![r[3]], vec![r[6], r[7]]])
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
    if scenario.single_censored() {
        input[roles[6]] = false;
    } else if scenario.pair_censored() {
        // Both marginal values appear, yet the pair 11 NEVER does.
        match step % 3 {
            0 => {
                input[roles[6]] = false;
                input[roles[7]] = false;
            }
            1 => {
                input[roles[6]] = true;
                input[roles[7]] = false;
            }
            _ => {
                input[roles[6]] = false;
                input[roles[7]] = true;
            }
        }
    }
    input
}

/// TEST is deliberately paired between supported and activated contexts:
/// 1024 baseline r6=0 contexts versus 1024 r6=r7=1 contexts.
fn test_input(rng: &mut SplitMix64, width: usize, roles: &[usize; 8], step: usize) -> Vec<bool> {
    let mut input = (0..width)
        .map(|_| rng.next_u64() & 1 != 0)
        .collect::<Vec<_>>();
    if step % 2 == 0 {
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
    true_gate: WideAnfGate,
    guard: AnfMarginalSupportGuard,
    committee: PassiveCommittee,
    train_flip_count: usize,
    bank_adequate: bool,
    observed_classes: usize,
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
        absent_teacher(width, &roles)
    } else {
        hypotheses[0].clone()
    };
    let mut model = PassiveCommittee::new(
        hypotheses,
        vec![4.0, 2.0, 2.0, 1.0],
        1.0 / 16.0,
        TRAIN_STEPS,
    )?;
    let mut guard = AnfMarginalSupportGuard::new(width, TRAIN_STEPS)?;
    let mut stream = SplitMix64::new(seed(width, index, scenario, TRAIN_SALT));
    let mut noise_stream = SplitMix64::new(seed(width, index, scenario, NOISE_SALT));
    let mut flips = 0usize;
    let mut saw_pair = false;
    let mut saw_r6_one = false;
    let mut saw_r7_one = false;
    for step in 0..TRAIN_STEPS {
        let input = train_input(&mut stream, width, &roles, scenario, step);
        saw_pair |= input[roles[6]] && input[roles[7]];
        saw_r6_one |= input[roles[6]];
        saw_r7_one |= input[roles[7]];
        let flipped = noisy && noise_stream.next_u64() & 15 == 0;
        flips += usize::from(flipped);
        let true_label = teacher.evaluate_scalar(&input)?;
        model.observe(&input, true_label ^ flipped)?;
        guard.observe(&input)?;
    }
    if scenario.single_censored() && saw_r6_one {
        return Err("prereg CENSORED TRAIN unexpectedly has r6=1".into());
    }
    if scenario.pair_censored() && (saw_pair || !saw_r6_one || !saw_r7_one) {
        return Err(
            "prereg PAIR TRAIN did not cover both marginals without pair coactivation".into(),
        );
    }
    if !scenario.single_censored() && !scenario.pair_censored() && !saw_pair {
        return Err("prereg FULL TRAIN did not observe a true interaction".into());
    }
    let bank_adequate = model.bank_adequate(BANK_ERROR_MAX)?;
    let classes = model.observed_equivalence_classes();
    Ok(Frozen {
        width,
        index,
        scenario,
        noisy,
        roles,
        true_gate: teacher,
        guard,
        committee: model,
        train_flip_count: flips,
        bank_adequate,
        observed_classes: classes,
    })
}

#[derive(Default, Clone)]
struct Metrics {
    tests: usize,
    committee_accepted: usize,
    committee_errors: usize,
    guarded_accepted: usize,
    guarded_errors: usize,
    novel_cases: usize,
    bank_inadequate_episodes: usize,
}

fn evaluate_one(frozen: &Frozen) -> Result<Metrics, Box<dyn Error>> {
    let mut rng = SplitMix64::new(seed(frozen.width, frozen.index, frozen.scenario, TEST_SALT));
    let mut out = Metrics::default();
    for step in 0..TEST_STEPS {
        let input = test_input(&mut rng, frozen.width, &frozen.roles, step);
        let truth = frozen.true_gate.evaluate_scalar(&input)?;
        let evaluated =
            frozen
                .guard
                .predict_guarded(&frozen.committee, &input, SELECT_CONF, BANK_ERROR_MAX)?;
        out.tests += 1;
        out.novel_cases += usize::from(!evaluated.marginal.marginally_supported);
        if let Some(pred) = evaluated.committee.selective_prediction {
            out.committee_accepted += 1;
            out.committee_errors += usize::from(pred != truth);
        }
        if let Some(pred) = evaluated.guarded_prediction {
            out.guarded_accepted += 1;
            out.guarded_errors += usize::from(pred != truth);
        }
    }
    out.bank_inadequate_episodes = usize::from(!frozen.bank_adequate);
    Ok(out)
}

fn main() -> Result<(), Box<dyn Error>> {
    println!(
        "A6_I3_CONFIG,widths=8;16;32;64;128;256;1024,seeds=3,panels=clean;noise_1_in_16,regimes=CENSORED_ABSENT;CENSORED_INCLUDED;PAIR_CENSORED_ABSENT;FULL_INCLUDED;FULL_ABSENT,train=512,test=2048,confidence=0.90,bank_mismatch_threshold=0.125,guard=univariate"
    );
    let mut frozen = Vec::<Frozen>::new();
    for width in WIDTHS {
        for index in 0..SEEDS {
            for scenario in Scenario::ALL {
                for noisy in [false, true] {
                    let f = fit_one(width, index, scenario, noisy)?;
                    println!(
                        "A6_I3_FROZEN,width={},seed={},regime={},panel={},train_samples={},train_noise_flips={},observed_signature_classes={},bank_adequate={},guard_payload_bytes={},teacher_in_bank={}",
                        width,
                        index,
                        scenario.name(),
                        if noisy { "noise_1_in_16" } else { "clean" },
                        f.guard.samples(),
                        f.train_flip_count,
                        f.observed_classes,
                        f.bank_adequate,
                        f.guard.logical_payload_bytes(),
                        !scenario.teacher_missing(),
                    );
                    frozen.push(f);
                }
            }
        }
    }
    if frozen.len() != 210 {
        return Err("A6-I3 frozen count mismatch".into());
    }
    println!("A6_I3_ALL_FROZEN,policies=210,paired_structures=105");

    // All 210 histories and their adequacy gates are frozen BEFORE the first
    // TEST label or test input is materialized.
    let mut all = vec![Metrics::default(); Scenario::ALL.len()];
    let mut structural_failures = 0usize;
    for f in &frozen {
        let measured = evaluate_one(f)?;
        let s = &mut all[f.scenario.id()];
        s.tests += measured.tests;
        s.committee_accepted += measured.committee_accepted;
        s.committee_errors += measured.committee_errors;
        s.guarded_accepted += measured.guarded_accepted;
        s.guarded_errors += measured.guarded_errors;
        s.novel_cases += measured.novel_cases;
        s.bank_inadequate_episodes += measured.bank_inadequate_episodes;
        let required = match f.scenario {
            Scenario::CensoredAbsent => {
                measured.novel_cases == TEST_STEPS / 2
                    && measured.guarded_accepted == TEST_STEPS / 2
                    && measured.guarded_errors == 0
                    && measured.committee_errors > 0
                    && f.bank_adequate
            }
            Scenario::CensoredIncluded => {
                measured.novel_cases == TEST_STEPS / 2
                    && measured.guarded_accepted == TEST_STEPS / 2
                    && measured.committee_errors == 0
                    && measured.guarded_errors == 0
                    && f.bank_adequate
            }
            Scenario::PairCensoredAbsent => {
                measured.novel_cases == 0
                    && measured.guarded_accepted == TEST_STEPS
                    && measured.guarded_errors > 0
                    && f.bank_adequate
            }
            Scenario::FullIncluded => {
                measured.novel_cases == 0
                    && measured.guarded_accepted == TEST_STEPS
                    && measured.guarded_errors == 0
                    && f.bank_adequate
            }
            Scenario::FullAbsent => {
                measured.novel_cases == 0 && measured.guarded_accepted == 0 && !f.bank_adequate
            }
        };
        structural_failures += usize::from(!required);
        println!(
            "A6_I3_TEST,width={},seed={},regime={},panel={},samples={},committee_accepted={},committee_errors={},guarded_accepted={},guarded_errors={},novel_cases={},bank_adequate={},oracle={}",
            f.width, f.index, f.scenario.name(),
            if f.noisy { "noise_1_in_16" } else { "clean" },
            measured.tests,
            measured.committee_accepted,
            measured.committee_errors,
            measured.guarded_accepted,
            measured.guarded_errors,
            measured.novel_cases,
            f.bank_adequate,
            if required { "PASS" } else { "FAIL" }
        );
    }

    for scenario in Scenario::ALL {
        let m = &all[scenario.id()];
        println!(
            "A6_I3_SUMMARY,regime={},policies=42,samples={},committee_accepted={},committee_errors={},guarded_accepted={},guarded_errors={},novel_cases={},bank_inadequate_episodes={}",
            scenario.name(),
            m.tests,
            m.committee_accepted,
            m.committee_errors,
            m.guarded_accepted,
            m.guarded_errors,
            m.novel_cases,
            m.bank_inadequate_episodes
        );
    }
    println!(
        "A6_I3_COMPLETE,policies=210,tests=430080,regimes=5,preregistered_failures={structural_failures}"
    );
    if structural_failures != 0 {
        return Err(
            "A6-I3 preregistered abstention and interaction-failure criteria not met".into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exactly_censored_interactions_are_indistinguishable_on_all_possible_train_inputs() {
        for k in WIDTHS {
            let role = permute_roles(k, 0);
            let bank = candidates(k, &role);
            let absent = absent_teacher(k, &role);
            for z in 0..32usize {
                let mut input = (0..k).map(|j| z & (1 << (j % 5)) != 0).collect::<Vec<_>>();
                input[role[6]] = false;
                assert_eq!(
                    bank[0].evaluate_scalar(&input),
                    absent.evaluate_scalar(&input),
                );
                input[role[6]] = true;
                input[role[7]] = false;
                assert_eq!(
                    bank[0].evaluate_scalar(&input),
                    absent.evaluate_scalar(&input),
                );
                input[role[6]] = true;
                input[role[7]] = true;
                assert_ne!(
                    bank[0].evaluate_scalar(&input),
                    absent.evaluate_scalar(&input),
                );
            }
        }
    }

    #[test]
    fn actual_pair_censored_training_shows_both_marginals_but_never_interaction() {
        for width in WIDTHS {
            let roles = permute_roles(width, 1);
            let mut rng = SplitMix64::new(17);
            let mut seen6 = [false; 2];
            let mut seen7 = [false; 2];
            for t in 0..TRAIN_STEPS {
                let sample = train_input(&mut rng, width, &roles, Scenario::PairCensoredAbsent, t);
                seen6[usize::from(sample[roles[6]])] = true;
                seen7[usize::from(sample[roles[7]])] = true;
                assert!(!(sample[roles[6]] && sample[roles[7]]));
            }
            assert_eq!(seen6, [true, true]);
            assert_eq!(seen7, [true, true]);
        }
    }
}
