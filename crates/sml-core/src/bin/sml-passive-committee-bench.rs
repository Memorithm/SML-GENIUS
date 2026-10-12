//! A6-I2: passive Markov-context training under four supplied wide ANF
//! hypotheses. No teacher IDs / TEST labels enter the student posterior.
//!
//! Tests exact observational equivalence, prior misspecification, candidate
//! absence, and confidence-selective OOD predictions. This does not fit
//! ANF monomials, train MOR routes, or execute recurrent Delta-KV.

use std::{error::Error, time::Instant};

use sml_core::{
    anf_passive_committee::{PassiveCommittee, PassivePrediction},
    anf_wide_sparse::{WideAnfGate, WideAnfLimits},
    toy_tasks::SplitMix64,
};

const WIDTHS: [usize; 7] = [8, 16, 32, 64, 128, 256, 1024];
const SEEDS: usize = 3;
const TEACHERS: usize = 4;
const TRAIN: usize = 512;
const OOD: usize = 2048;
const ASSUMED_NOISE: f64 = 1.0 / 16.0;
const CONFIDENCE: f64 = 0.90;
const ADEQUACY_MAX_MISMATCH: f64 = 1.0 / 8.0;
const ROLES_SALT: u64 = 0xA6B1_5001_51ED_0001;
const TRAIN_SALT: u64 = 0xA6B1_5002_51ED_0002;
const LABEL_SALT: u64 = 0xA6B1_5003_51ED_0003;
const TEST_SALT: u64 = 0xA6B1_5004_51ED_0004;
const NEGATIVE_SALT: u64 = 0xA6B1_5005_51ED_0005;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Visibility {
    Blind,
    Partial,
    Full,
}
impl Visibility {
    const ALL: [Self; 3] = [Self::Blind, Self::Partial, Self::Full];
    fn id(self) -> u64 {
        match self {
            Self::Blind => 0,
            Self::Partial => 1,
            Self::Full => 2,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Blind => "blind",
            Self::Partial => "partial",
            Self::Full => "full",
        }
    }
    fn expected_classes(self) -> usize {
        match self {
            Self::Blind => 1,
            Self::Partial => 2,
            Self::Full => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Panel {
    Clean,
    Noisy,
}
impl Panel {
    const ALL: [Self; 2] = [Self::Clean, Self::Noisy];
    fn id(self) -> u64 {
        match self {
            Self::Clean => 0,
            Self::Noisy => 1,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::Noisy => "noise_1_in_16",
        }
    }
}

fn mixed_seed(width: usize, seed: usize, salt: u64) -> u64 {
    (seed as u64 + 1).wrapping_mul(0xd134_2543_de82_ef95) ^ width as u64 ^ salt
}
fn train_seed(width: usize, seed: usize, mode: Visibility, panel: Panel) -> u64 {
    mixed_seed(width, seed, TRAIN_SALT)
        ^ mode.id().wrapping_mul(0x94d0_49bb_1331_11eb)
        ^ panel.id().wrapping_mul(0x9e37_79b9_7f4a_7c15)
}
fn noise_seed(width: usize, seed: usize, mode: Visibility, panel: Panel) -> u64 {
    mixed_seed(width, seed, LABEL_SALT)
        ^ mode.id().wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ panel.id().wrapping_mul(0xd134_2543_de82_ef95)
}
fn roles(width: usize, seed: usize) -> [usize; 8] {
    let mut rng = SplitMix64::new(mixed_seed(width, seed, ROLES_SALT));
    let mut perm = (0..width).collect::<Vec<_>>();
    for i in (1..width).rev() {
        let j = rng.below(i + 1);
        perm.swap(i, j);
    }
    perm[..8].try_into().unwrap()
}

fn bank(width: usize, roles: &[usize; 8]) -> Result<Vec<WideAnfGate>, Box<dyn Error>> {
    let base = vec![vec![roles[2]], vec![roles[3]]];
    let first = vec![roles[0], roles[1]];
    let second = vec![roles[4], roles[5]];
    let families = [
        base.clone(),
        [base.clone(), vec![first.clone()]].concat(),
        [base.clone(), vec![second.clone()]].concat(),
        [base, vec![first, second]].concat(),
    ];
    let limits = WideAnfLimits::new(2048, 256, 8192, 8192, 8192);
    families
        .into_iter()
        .map(|terms| {
            WideAnfGate::from_monomials((0..width).collect(), terms, width, limits)
                .map_err(|e| format!("could not compile hypothesis: {e:?}").into())
        })
        .collect()
}
fn committee(width: usize, role_map: &[usize; 8]) -> Result<PassiveCommittee, Box<dyn Error>> {
    Ok(PassiveCommittee::new(
        bank(width, role_map)?,
        vec![4.0, 2.0, 2.0, 1.0],
        ASSUMED_NOISE,
        TRAIN,
    )?)
}

/// The observed input process has persistent Markov trigger state across
/// timesteps; the student is NOT allowed to choose any query.
#[derive(Clone)]
struct Trace {
    width: usize,
    roles: [usize; 8],
    visibility: Option<Visibility>,
    censored_extra: bool,
    rng: SplitMix64,
    trigger_a: bool,
    trigger_b: bool,
}
impl Trace {
    fn training(
        width: usize,
        role_map: [usize; 8],
        mode: Visibility,
        stream: u64,
        censored_extra: bool,
    ) -> Self {
        Self {
            width,
            roles: role_map,
            visibility: Some(mode),
            censored_extra,
            rng: SplitMix64::new(stream),
            trigger_a: false,
            trigger_b: false,
        }
    }
    fn ood(width: usize, role_map: [usize; 8], stream: u64) -> Self {
        Self {
            width,
            roles: role_map,
            visibility: None,
            censored_extra: false,
            rng: SplitMix64::new(stream),
            trigger_a: false,
            trigger_b: false,
        }
    }
    fn transition(&mut self, state: bool, activation_denominator: usize, exit_denominator: usize) -> bool {
        if state {
            self.rng.below(exit_denominator) != 0
        } else {
            self.rng.below(activation_denominator) == 0
        }
    }
    fn next(&mut self) -> Vec<bool> {
        let mut input = (0..self.width)
            .map(|_| self.rng.next_bool())
            .collect::<Vec<_>>();
        let (a, b, forced_partner_on_activation) = match self.visibility {
            Some(Visibility::Blind) => (false, false, true),
            Some(Visibility::Partial) => {
                self.trigger_a = self.transition(self.trigger_a, 14, 2);
                (self.trigger_a, false, true)
            }
            Some(Visibility::Full) => {
                self.trigger_a = self.transition(self.trigger_a, 14, 2);
                self.trigger_b = self.transition(self.trigger_b, 14, 2);
                (self.trigger_a, self.trigger_b, true)
            }
            None => {
                self.trigger_a = self.transition(self.trigger_a, 4, 4);
                self.trigger_b = self.transition(self.trigger_b, 4, 4);
                (self.trigger_a, self.trigger_b, false)
            }
        };
        input[self.roles[0]] = a;
        input[self.roles[4]] = b;
        if forced_partner_on_activation {
            if a {
                input[self.roles[1]] = true;
            }
            if b {
                input[self.roles[5]] = true;
            }
        }
        if self.censored_extra {
            input[self.roles[6]] = false;
        }
        input
    }
}

struct Frozen {
    width: usize,
    seed: usize,
    teacher: usize,
    mode: Visibility,
    panel: Panel,
    committee: PassiveCommittee,
    labels: Vec<bool>,
    teacher_posterior: f64,
    train_ms: f64,
}

#[derive(Clone, Copy)]
enum Negative {
    Visible,
    Censored,
}
impl Negative {
    const ALL: [Self; 2] = [Self::Visible, Self::Censored];
    fn name(self) -> &'static str {
        match self {
            Self::Visible => "visible",
            Self::Censored => "censored",
        }
    }
}

struct FrozenNegative {
    width: usize,
    seed: usize,
    kind: Negative,
    committee: PassiveCommittee,
    trained_adequate: bool,
    mismatch: f64,
}

/// No teacher truth enters the committee: the generator uses it ONLY to
/// provide the observed scalar Boolean label for that step.
fn fit_positive(
    width: usize,
    seed: usize,
    role_map: [usize; 8],
    teacher: usize,
    mode: Visibility,
    panel: Panel,
) -> Result<Frozen, Box<dyn Error>> {
    let candidates = bank(width, &role_map)?;
    let mut learner = committee(width, &role_map)?;
    let mut process = Trace::training(
        width,
        role_map,
        mode,
        train_seed(width, seed, mode, panel),
        false,
    );
    let mut rng = SplitMix64::new(noise_seed(width, seed, mode, panel));
    let mut labels = Vec::with_capacity(TRAIN);
    let start = Instant::now();
    for _ in 0..TRAIN {
        let input = process.next();
        let target = candidates[teacher].evaluate_scalar(&input)?;
        let noise = panel == Panel::Noisy && rng.next_u64() & 15 == 0;
        let observed = target ^ noise;
        learner.observe(&input, observed)?;
        labels.push(observed);
    }
    let train_ms = start.elapsed().as_secs_f64() * 1000.0;
    let posterior = learner.posterior()[teacher];
    Ok(Frozen {
        width,
        seed,
        teacher,
        mode,
        panel,
        committee: learner,
        labels,
        teacher_posterior: posterior,
        train_ms,
    })
}

fn fit_negative(
    width: usize,
    seed: usize,
    role_map: [usize; 8],
    kind: Negative,
) -> Result<FrozenNegative, Box<dyn Error>> {
    let candidates = bank(width, &role_map)?;
    let mut learner = committee(width, &role_map)?;
    let mut process = Trace::training(
        width,
        role_map,
        Visibility::Blind,
        mixed_seed(width, seed, NEGATIVE_SALT),
        matches!(kind, Negative::Censored),
    );
    let mut noise = SplitMix64::new(mixed_seed(width, seed, LABEL_SALT ^ NEGATIVE_SALT));
    for _ in 0..TRAIN {
        let input = process.next();
        let extra = input[role_map[6]] && input[role_map[7]];
        let target = candidates[0].evaluate_scalar(&input)? ^ extra;
        learner.observe(&input, target ^ (noise.next_u64() & 15 == 0))?;
    }
    let mismatch = learner.training_min_mismatch_rate()?;
    let trained_adequate = learner.bank_adequate(ADEQUACY_MAX_MISMATCH)?;
    Ok(FrozenNegative {
        width,
        seed,
        kind,
        committee: learner,
        trained_adequate,
        mismatch,
    })
}

/// All predictions use the post-TRAIN committee; independent OOD test inputs
/// are constructed only AFTER all committees and missing-teacher controls
/// have frozen. Every model at a given width/seed shares the same input stream.
#[derive(Clone, Copy, Default)]
struct Metrics {
    evaluated: usize,
    accepted: usize,
    accepted_errors: usize,
    abstained_on_disagreement_violations: usize,
    map_errors: usize,
    brier_biased_sum: f64,
    brier_uniform_sum: f64,
}
impl Metrics {
    fn combine(&mut self, other: &Self) {
        self.evaluated += other.evaluated;
        self.accepted += other.accepted;
        self.accepted_errors += other.accepted_errors;
        self.abstained_on_disagreement_violations += other.abstained_on_disagreement_violations;
        self.map_errors += other.map_errors;
        self.brier_biased_sum += other.brier_biased_sum;
        self.brier_uniform_sum += other.brier_uniform_sum;
    }
    fn coverage(self) -> f64 {
        self.accepted as f64 / self.evaluated as f64
    }
    fn brier_biased(self) -> f64 {
        self.brier_biased_sum / self.evaluated as f64
    }
    fn brier_uniform(self) -> f64 {
        self.brier_uniform_sum / self.evaluated as f64
    }
}
fn measure_prediction(
    pred: &PassivePrediction,
    truth: bool,
    require_abstention_on_disagreement: bool,
) -> Metrics {
    let binary_target = f64::from(u8::from(truth));
    let uniform_p = pred.candidate_bits.iter().filter(|&&b| b).count() as f64
        / pred.candidate_bits.len() as f64;
    let brier_biased = (pred.probability_true - binary_target).powi(2);
    let brier_uniform = (uniform_p - binary_target).powi(2);
    Metrics {
        evaluated: 1,
        accepted: usize::from(pred.selective_prediction.is_some()),
        accepted_errors: usize::from(pred.selective_prediction.is_some_and(|b| b != truth)),
        abstained_on_disagreement_violations: usize::from(
            require_abstention_on_disagreement
                && !pred.unanimous
                && pred.selective_prediction.is_some(),
        ),
        map_errors: usize::from(pred.map_prediction != truth),
        brier_biased_sum: brier_biased,
        brier_uniform_sum: brier_uniform,
    }
}

fn assert_stratum(
    frozen: &[Frozen],
    width: usize,
    seed: usize,
    mode: Visibility,
    panel: Panel,
) -> Result<(), Box<dyn Error>> {
    let records = frozen.iter().filter(|r| {
        r.width == width && r.seed == seed && r.mode == mode && r.panel == panel
    }).collect::<Vec<_>>();
    if records.len() != TEACHERS {
        return Err("stratum has incorrect number of teachers".into());
    }
    let expected = mode.expected_classes();
    for row in &records {
        if row.committee.observed_equivalence_classes() != expected {
            return Err(format!("expected {expected} exact passive classes in {}", mode.name()).into());
        }
    }
    let identical = |a: usize, b: usize| -> bool {
        records[a].labels == records[b].labels
            && records[a].committee.posterior() == records[b].committee.posterior()
    };
    let passed = match mode {
        Visibility::Blind => (1..4).all(|teacher| identical(0, teacher)),
        Visibility::Partial => identical(0, 2) && identical(1, 3),
        Visibility::Full => (0..4).all(|teacher| records[teacher].teacher_posterior >= 0.99),
    };
    if !passed {
        return Err(format!("passive equivalence or FULL identification failed in {}", mode.name()).into());
    }
    println!(
        "A6_I2_STRATUM,width={width},seed={seed},mode={},panel={},hypotheses=4,signature_classes={expected},observational_pairs_equivalent=true,full_teacher_posterior_at_least_0_99={},oracle=PASS",
        mode.name(), panel.name(), mode != Visibility::Full || passed
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    println!(
        "A6_I2_CONFIG,widths=8;16;32;64;128;256;1024,seeds=3,teachers=4,regimes=blind;partial;full,panels=clean;noise_1_in_16,train=512,ood_test=2048,priors=4;2;2;1,noise_model=1_in_16,confidence=0_90,adequacy=0_125,markov_persistent=true,teacher_bank_incomplete_negatives=2"
    );
    let mut frozen = Vec::<Frozen>::new();
    for width in WIDTHS {
        for seed in 0..SEEDS {
            let role_map = roles(width, seed);
            for mode in Visibility::ALL {
                for panel in Panel::ALL {
                    for teacher in 0..TEACHERS {
                        let candidate = fit_positive(width, seed, role_map, teacher, mode, panel)?;
                        let p = candidate.teacher_posterior;
                        let signatures = candidate.committee.observed_equivalence_classes();
                        let mismatch = candidate.committee.training_min_mismatch_rate()?;
                        println!(
                            "A6_I2_FROZEN,width={width},seed={seed},mode={},panel={},teacher={teacher},samples={},signature_classes={signatures},posterior_teacher={p:.12},min_train_mismatch={mismatch:.6},train_ms={:.6},bank_adequate={}",
                            mode.name(), panel.name(), candidate.committee.samples(), candidate.train_ms,
                            candidate.committee.bank_adequate(ADEQUACY_MAX_MISMATCH)?
                        );
                        frozen.push(candidate);
                    }
                }
            }
        }
    }
    if frozen.len() != 504 {
        return Err("expected 504 frozen committees".into());
    }
    // All 504 training/selection decisions are frozen before constructing
    // ANY held-out OOD input stream.
    for width in WIDTHS {
        for seed in 0..SEEDS {
            for mode in Visibility::ALL {
                for panel in Panel::ALL {
                    assert_stratum(&frozen, width, seed, mode, panel)?;
                }
            }
        }
    }
    // Negative hypotheses, likewise frozen before OOD input construction:
    // g = h0 XOR x_r6*x_r7, absent from all four supplied candidates.
    let mut negatives = Vec::<FrozenNegative>::new();
    for width in WIDTHS {
        for seed in 0..SEEDS {
            for kind in Negative::ALL {
                negatives.push(fit_negative(width, seed, roles(width, seed), kind)?);
            }
        }
    }
    if negatives.len() != 42 {
        return Err("expected 42 missing-teacher negative controls".into());
    }
    println!(
        "A6_I2_ALL_FROZEN,positive=504,negative=42,distinct_ood_input_streams=21,training_teacher_signatures_checked=126"
    );

    let mut by_mode = [Metrics::default(); 3];
    let mut blind_h0 = Metrics::default();
    let mut blind_h3 = Metrics::default();
    let mut negative_visible_rejected = 0usize;
    let mut negative_censored_accepted = 0usize;
    let mut negative_censored_wrong_confident = 0usize;
    let mut full_teacher_posterior_pass = 0usize;
    let mut blind_exact_posterior_groups = 0usize;
    let mut partial_exact_posterior_groups = 0usize;
    let mut strata_count = 0usize;

    for width in WIDTHS {
        for seed in 0..SEEDS {
            let role_map = roles(width, seed);
            let mut ood = Trace::ood(width, role_map, mixed_seed(width, seed, TEST_SALT));
            let inputs = (0..OOD).map(|_| ood.next()).collect::<Vec<_>>();

            for mode in Visibility::ALL {
                for panel in Panel::ALL {
                    strata_count += 1;
                    let subset = frozen.iter().filter(|f| {
                        f.width == width && f.seed == seed && f.mode == mode && f.panel == panel
                    }).collect::<Vec<_>>();
                    if subset.len() != 4 {
                        return Err("unbalanced teacher stratum".into());
                    }
                    if mode == Visibility::Blind {
                        blind_exact_posterior_groups += 1;
                    }
                    if mode == Visibility::Partial {
                        partial_exact_posterior_groups += 1;
                    }
                    for f in subset {
                        let mut result = Metrics::default();
                        for query in &inputs {
                            let prediction = f.committee.predict(query, CONFIDENCE)?;
                            let teacher_bit = prediction.candidate_bits[f.teacher];
                            result.combine(&measure_prediction(
                                &prediction,
                                teacher_bit,
                                mode == Visibility::Blind,
                            ));
                        }
                        if mode == Visibility::Full && f.teacher_posterior >= 0.99 {
                            full_teacher_posterior_pass += 1;
                        }
                        println!(
                            "A6_I2_TEST,width={width},seed={seed},mode={},panel={},teacher={},teacher_posterior={:.12},samples={},coverage={:.6},selective_errors={},map_errors={},brier_biased={:.9},brier_uniform={:.9},blind_disagreement_accepted={},oracle={}",
                            mode.name(), panel.name(), f.teacher, f.teacher_posterior, OOD,
                            result.coverage(), result.accepted_errors, result.map_errors,
                            result.brier_biased(), result.brier_uniform(),
                            result.abstained_on_disagreement_violations,
                            if result.accepted_errors == 0 && result.abstained_on_disagreement_violations == 0 { "PASS" } else { "FAIL" }
                        );
                        if mode == Visibility::Blind && f.teacher == 0 {
                            blind_h0.combine(&result);
                        } else if mode == Visibility::Blind && f.teacher == 3 {
                            blind_h3.combine(&result);
                        }
                        by_mode[mode.id() as usize].combine(&result);
                    }
                }
            }
            for negative in negatives.iter().filter(|f| f.width == width && f.seed == seed) {
                let mut accepted_wrong = 0usize;
                let mut accepted = 0usize;
                let mut all_errors = 0usize;
                for q in &inputs {
                    let pred = negative.committee.predict(q, CONFIDENCE)?;
                    let out_of_bank_truth = pred.candidate_bits[0] ^ (q[role_map[6]] && q[role_map[7]]);
                    all_errors += usize::from(pred.map_prediction != out_of_bank_truth);
                    if let Some(prediction) = pred.selective_prediction {
                        accepted += 1;
                        accepted_wrong += usize::from(prediction != out_of_bank_truth);
                    }
                }
                match negative.kind {
                    Negative::Visible => {
                        if !negative.trained_adequate {
                            negative_visible_rejected += 1;
                        }
                    }
                    Negative::Censored => {
                        if negative.trained_adequate {
                            negative_censored_accepted += 1;
                        }
                        if accepted_wrong > 0 {
                            negative_censored_wrong_confident += 1;
                        }
                    }
                }
                println!(
                    "A6_I2_NEGATIVE,width={width},seed={seed},kind={},actual_teacher_missing=true,train_mismatch={:.6},bank_adequate={},ood_map_errors={all_errors},ood_accepted={accepted},ood_high_confidence_errors={accepted_wrong},oracle={}",
                    negative.kind.name(), negative.mismatch, negative.trained_adequate,
                    if matches!(negative.kind, Negative::Visible) && !negative.trained_adequate
                        || matches!(negative.kind, Negative::Censored) && negative.trained_adequate && accepted_wrong > 0
                    { "PASS" } else { "FAIL" }
                );
            }
        }
    }

    for mode in Visibility::ALL {
        let value = by_mode[mode.id() as usize];
        println!(
            "A6_I2_REGIME,mode={},ood_bits={},coverage={:.9},accepted={},selective_errors={},map_errors={},brier_biased={:.9},brier_uniform={:.9},blind_disagreement_accepted={}",
            mode.name(), value.evaluated, value.coverage(), value.accepted,
            value.accepted_errors, value.map_errors, value.brier_biased(),
            value.brier_uniform(), value.abstained_on_disagreement_violations
        );
    }

    let blind = by_mode[Visibility::Blind.id() as usize];
    let full = by_mode[Visibility::Full.id() as usize];
    let prior_harms_balanced = blind.brier_biased() > blind.brier_uniform();
    let prior_helps_h0 = blind_h0.brier_biased() < blind_h0.brier_uniform();
    let prior_harms_h3 = blind_h3.brier_biased() > blind_h3.brier_uniform();
    let passed = frozen.len() == 504
        && strata_count == 126
        && blind_exact_posterior_groups == 42
        && partial_exact_posterior_groups == 42
        && full_teacher_posterior_pass == 168
        && blind.accepted > 0
        && blind.accepted < blind.evaluated
        && blind.accepted_errors == 0
        && blind.abstained_on_disagreement_violations == 0
        && full.coverage() >= 0.99
        && full.accepted_errors == 0
        && by_mode.iter().all(|m| m.accepted_errors == 0)
        && negative_visible_rejected == 21
        && negative_censored_accepted == 21
        && negative_censored_wrong_confident == 21
        && prior_harms_balanced
        && prior_helps_h0
        && prior_harms_h3;
    println!(
        "A6_I2_COMPLETE,policies=504,strata=126,blind_equivalent_groups={blind_exact_posterior_groups},partial_equivalent_groups={partial_exact_posterior_groups},full_posterior_pass={full_teacher_posterior_pass},negative_visible_rejected={negative_visible_rejected},negative_censored_accepted={negative_censored_accepted},negative_censored_wrong_confident={negative_censored_wrong_confident},blind_selective_errors={},full_selective_errors={},prior_harms_balanced={prior_harms_balanced},prior_helps_h0={prior_helps_h0},prior_harms_h3={prior_harms_h3},oracle={}",
        blind.accepted_errors, full.accepted_errors, if passed { "PASS" } else { "FAIL" }
    );
    if !passed {
        return Err("A6-I2 frozen passive uncertainty, prior and missing-teacher stress failed".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passive_roles_are_distinct_and_spread_across_full_wide_input_domain() {
        for width in WIDTHS {
            for seed in 0..SEEDS {
                let r = roles(width, seed);
                for (i, &bit) in r.iter().enumerate() {
                    assert!(bit < width);
                    assert!(!r[..i].contains(&bit));
                }
                assert_eq!(bank(width, &r).unwrap().len(), 4);
            }
        }
    }

    #[test]
    fn single_teacher_and_all_factors_are_derived_only_in_benchmark() {
        for mode in Visibility::ALL {
            let width = 16;
            let seed = 0;
            let map = roles(width, seed);
            let f = fit_positive(width, seed, map, 0, mode, Panel::Clean).unwrap();
            assert_eq!(f.committee.samples(), TRAIN);
            assert_eq!(f.committee.observed_equivalence_classes(), mode.expected_classes());
        }
    }

    #[test]
    fn missing_teacher_visible_control_is_detectable_but_censored_is_not() {
        let width = 128;
        let seed = 1;
        let map = roles(width, seed);
        let visible = fit_negative(width, seed, map, Negative::Visible).unwrap();
        let censored = fit_negative(width, seed, map, Negative::Censored).unwrap();
        assert!(!visible.trained_adequate);
        assert!(censored.trained_adequate);
    }
}
