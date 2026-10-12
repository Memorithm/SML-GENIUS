//! A6-I1: preregistered epistemic nonidentifiability and "intuition" trial.
//! The context prior is trained only on PRIOR labeled tasks. Every blind
//! episode has two WideAnfGate hypotheses that are exactly equivalent on
//! the passive TRAIN/VAL distribution but disagree on held-out OOD inputs.
//!
//! Learning a cross-task prior can help when the environment is stationary;
//! it is not a discovery of the true hidden function from impossible data.
//! Negative tests reverse or erase the contextual prior and demand failure.

use std::{error::Error, time::Instant};

use sml_core::{
    anf_epistemic_intuition::{ContextualPrior, EpistemicAnfPair},
    anf_wide_sparse::{WideAnfGate, WideAnfLimits},
    toy_tasks::SplitMix64,
};

const WIDTHS: [usize; 7] = [8, 16, 32, 64, 128, 256, 1024];
const META_EPISODES_PER_CONTEXT: usize = 64;
const BLIND_EPISODES_PER_CONTEXT: usize = 32;
const PASSIVE_TRAIN: usize = 16;
const PASSIVE_VAL: usize = 16;
const OOD_RANDOM: usize = 32;
const OOD_FORCED: usize = 16;
const OOD_TEST: usize = OOD_RANDOM + OOD_FORCED;
const META_SALT: u64 = 0xA6B1_4001_51ED_0001;
const PASSIVE_SALT: u64 = 0xA6B1_4002_51ED_0002;
const TEST_SALT: u64 = 0xA6B1_4003_51ED_0003;
const INTERVENTION_SALT: u64 = 0xA6B1_4004_51ED_0004;
const CANDIDATES_SALT: u64 = 0xA6B1_4005_51ED_0005;
const LABEL_SALT: u64 = 0xA6B1_4006_51ED_0006;

#[derive(Clone, Copy, Debug)]
enum PassiveMode {
    Exclusive,
    FrozenZero,
}
impl PassiveMode {
    const ALL: [Self; 2] = [Self::Exclusive, Self::FrozenZero];
    fn index(self) -> u64 {
        match self { Self::Exclusive => 0, Self::FrozenZero => 1 }
    }
    fn name(self) -> &'static str {
        match self { Self::Exclusive => "exclusive", Self::FrozenZero => "frozen_zero" }
    }
}

#[derive(Clone, Copy, Debug)]
enum Regime {
    Matched,
    Reversed,
    Uninformative,
}
impl Regime {
    const ALL: [Self; 3] = [Self::Matched, Self::Reversed, Self::Uninformative];
    fn index(self) -> u64 {
        match self {
            Self::Matched => 0, Self::Reversed => 1, Self::Uninformative => 2,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Matched => "matched",
            Self::Reversed => "reversed",
            Self::Uninformative => "uninformative",
        }
    }
    fn alternative_eighths(self, context: usize) -> u64 {
        match (self, context) {
            (Self::Matched, 0) | (Self::Reversed, 1) => 1,
            (Self::Matched, 1) | (Self::Reversed, 0) => 7,
            (Self::Uninformative, _) => 4,
            _ => unreachable!(),
        }
    }
}

fn gate_budget() -> WideAnfLimits {
    WideAnfLimits::new(2048, 256, 8192, 8192, 8192)
}
fn block_seed(width: usize, seed: usize, mode: PassiveMode, noisy: bool) -> u64 {
    (seed as u64 + 1).wrapping_mul(0xd134_2543_de82_ef95)
        ^ width as u64
        ^ mode.index().wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ u64::from(noisy).wrapping_mul(0x9e37_79b9_7f4a_7c15)
}
fn stream(base: u64, salt: u64, context: usize, episode: usize) -> u64 {
    base ^ salt
        ^ (context as u64 + 1).wrapping_mul(0x94d0_49bb_1331_11eb)
        ^ (episode as u64 + 1).wrapping_mul(0x369d_ea0f_31a5_3f85)
}

/// Every episode has a fresh pair of *native* sparse GF(2) hypotheses.
/// The hidden status is NOT passed to this builder. Both hypotheses share
/// four base terms (degrees 2,3,4,5) drawn from all but two randomized
/// physical indexes. Only the second adds their AND interaction.
fn hypotheses(width: usize, pair_seed: u64) -> Result<(EpistemicAnfPair, usize, usize), Box<dyn Error>> {
    let mut rng = SplitMix64::new(pair_seed);
    let mut perm = (0..width).collect::<Vec<_>>();
    for i in (1..width).rev() {
        let j = rng.below(i + 1);
        perm.swap(i, j);
    }
    let a = perm[0];
    let b = perm[1];
    let usable = width - 2;
    let mut base_terms = Vec::new();
    for term_idx in 0..4 {
        let degree = term_idx + 2;
        let offset = (term_idx * 3) % usable;
        let mut term = (0..degree)
            .map(|j| perm[2 + (offset + j) % usable])
            .collect::<Vec<_>>();
        term.sort_unstable();
        base_terms.push(term);
    }
    let original = WideAnfGate::from_monomials(
        (0..width).collect(), base_terms.clone(), width, gate_budget()
    )?;
    let mut alternative_terms = base_terms;
    let mut new_term = vec![a, b];
    new_term.sort_unstable();
    alternative_terms.push(new_term);
    let alternative = WideAnfGate::from_monomials(
        (0..width).collect(), alternative_terms, width, gate_budget()
    )?;
    Ok((EpistemicAnfPair::new(original, alternative)?, a, b))
}

/// Train modes always satisfy x_a & x_b = 0, regardless of whether the
/// teacher is the reference or alternative. The pair is *provably*
/// indistinguishable on ALL observations of these distributions.
fn passive_input(width: usize, a: usize, b: usize, rng: &mut SplitMix64, mode: PassiveMode) -> Vec<bool> {
    let mut q = (0..width).map(|_| rng.next_u64() & 1 != 0).collect::<Vec<_>>();
    match mode {
        PassiveMode::Exclusive => match rng.below(3) {
            0 => { q[a] = false; q[b] = false; }
            1 => { q[a] = true; q[b] = false; }
            _ => { q[a] = false; q[b] = true; }
        },
        PassiveMode::FrozenZero => {
            q[a] = false;
            q[b] = rng.next_u64() & 1 != 0;
        }
    }
    q
}

/// The only source of context prior is HISTORIC labeled tasks. Current
/// teacher IDs remain sealed through passive inference, OOD prediction,
/// abstention and optional intervention. Priors are frozen for all 3
/// blinded regimes before any blind task is generated.
fn learn_context_prior(base: u64) -> Result<ContextualPrior, Box<dyn Error>> {
    let mut counts = ContextualPrior::default();
    for context in 0..2 {
        let mut rng = SplitMix64::new(stream(base, META_SALT, context, 0));
        let alt_eighths = if context == 0 { 1 } else { 7 };
        for _ in 0..META_EPISODES_PER_CONTEXT {
            let past_teacher_is_alt = rng.below(8) < alt_eighths;
            counts.observe_historical(context, past_teacher_is_alt)?;
        }
    }
    Ok(counts)
}

#[derive(Default, Clone, Copy)]
struct Metrics {
    episodes: u64,
    passive_observations: u64,
    passive_disagreements: u64,
    noisy_passive_flips: u64,
    ood_queries: u64,
    ood_disagreements: u64,
    forced_ood_differences: u64,
    contextual_correct: u64,
    pooled_correct: u64,
    uniform_correct: u64,
    intervention_correct: u64,
    selective_covered: u64,
    selective_clean_errors: u64,
    selective_noisy_errors: u64,
    contextual_brier_sum: f64,
    pooled_brier_sum: f64,
    uniform_brier_sum: f64,
    confidence_sum: f64,
    active_queries: u64,
    inference_ns: u128,
}
impl Metrics {
    fn add(&mut self, rhs: &Self) {
        self.episodes += rhs.episodes;
        self.passive_observations += rhs.passive_observations;
        self.passive_disagreements += rhs.passive_disagreements;
        self.noisy_passive_flips += rhs.noisy_passive_flips;
        self.ood_queries += rhs.ood_queries;
        self.ood_disagreements += rhs.ood_disagreements;
        self.forced_ood_differences += rhs.forced_ood_differences;
        self.contextual_correct += rhs.contextual_correct;
        self.pooled_correct += rhs.pooled_correct;
        self.uniform_correct += rhs.uniform_correct;
        self.intervention_correct += rhs.intervention_correct;
        self.selective_covered += rhs.selective_covered;
        self.selective_clean_errors += rhs.selective_clean_errors;
        self.selective_noisy_errors += rhs.selective_noisy_errors;
        self.contextual_brier_sum += rhs.contextual_brier_sum;
        self.pooled_brier_sum += rhs.pooled_brier_sum;
        self.uniform_brier_sum += rhs.uniform_brier_sum;
        self.confidence_sum += rhs.confidence_sum;
        self.active_queries += rhs.active_queries;
        self.inference_ns += rhs.inference_ns;
    }
    fn accuracy(self) -> f64 {
        self.contextual_correct as f64 / self.ood_disagreements as f64
    }
    fn pooled_accuracy(self) -> f64 {
        self.pooled_correct as f64 / self.ood_disagreements as f64
    }
    fn uniform_accuracy(self) -> f64 {
        self.uniform_correct as f64 / self.ood_disagreements as f64
    }
    fn brier_context(self) -> f64 {
        self.contextual_brier_sum / self.ood_disagreements as f64
    }
    fn brier_pooled(self) -> f64 {
        self.pooled_brier_sum / self.ood_disagreements as f64
    }
    fn brier_uniform(self) -> f64 {
        self.uniform_brier_sum / self.ood_disagreements as f64
    }
    fn expected_confidence(self) -> f64 {
        self.confidence_sum / self.ood_disagreements as f64
    }
    fn coverage(self) -> f64 {
        self.selective_covered as f64 / self.ood_queries as f64
    }
}

fn run_blind_cell(
    width: usize,
    base: u64,
    mode: PassiveMode,
    noisy: bool,
    regime: Regime,
    prior: &ContextualPrior,
) -> Result<Metrics, Box<dyn Error>> {
    let mut result = Metrics::default();
    let pooled_prior = prior.pooled_probability_alternative()?;
    for context in 0..2 {
        let contextual_prior = prior.probability_alternative(context)?;
        let mut teacher_rng = SplitMix64::new(
            stream(base, TEST_SALT ^ 0x1111_1111_1111_1111, context, regime.index() as usize)
        );
        for episode in 0..BLIND_EPISODES_PER_CONTEXT {
            // The chosen current-teacher ID is used ONLY by the simulator's
            // independent observation and retrospective score oracle.
            let teacher_is_alt =
                teacher_rng.below(8) < regime.alternative_eighths(context);
            let (pair, a, b) = hypotheses(
                width,
                stream(base, CANDIDATES_SALT, context, episode)
            )?;
            let mut passive_rng = SplitMix64::new(stream(base, PASSIVE_SALT, context, episode));
            let mut label_rng = SplitMix64::new(
                stream(base, LABEL_SALT ^ PASSIVE_SALT, context, episode)
            );
            for _ in 0..PASSIVE_TRAIN + PASSIVE_VAL {
                let q = passive_input(width, a, b, &mut passive_rng, mode);
                let pred = pair.predict(&q, contextual_prior)?;
                if pred.disagreement || (q[a] && q[b]) {
                    return Err("passive observational equivalence violated".into());
                }
                let true_label = if teacher_is_alt { pred.alternative } else { pred.reference };
                let flipped = noisy && label_rng.next_u64() & 15 == 0;
                let observed = true_label ^ flipped;
                // Consuming the passive label can never choose the ID,
                // because the hypotheses predict exactly the SAME label.
                let _ = observed;
                result.noisy_passive_flips += u64::from(flipped);
                result.passive_observations += 1;
                result.passive_disagreements += u64::from(pred.disagreement);
            }

            // Separate ACTIVE arm: one targeted disagreement query with
            // x_a=x_b=true and every other input=false. No feedback is
            // supplied to the passive posterior or forced predictions.
            let mut active = vec![false; width];
            active[a] = true;
            active[b] = true;
            let active_truth = pair.predict(&active, 0.5)?;
            if !active_truth.disagreement {
                return Err("intervention did not disambiguate the hypothesis pair".into());
            }
            let true_intervention_label = if teacher_is_alt {
                active_truth.alternative
            } else {
                active_truth.reference
            };
            let mut intervention_rng = SplitMix64::new(
                stream(base, INTERVENTION_SALT, context, episode)
            );
            let votes = if noisy { 21 } else { 1 };
            let mut observed_votes = Vec::new();
            observed_votes
                .try_reserve_exact(votes)
                .map_err(|_| "intervention vote allocation failed")?;
            for _ in 0..votes {
                let flipped = noisy && intervention_rng.next_u64() & 15 == 0;
                observed_votes.push(true_intervention_label ^ flipped);
            }
            let active_guess = pair.classify_intervention(&active, &observed_votes)?;
            result.intervention_correct += u64::from(active_guess == teacher_is_alt);
            result.active_queries += votes as u64;
            result.episodes += 1;

            // Held-out OOD: 32 naturally sampled queries + 16 targeted
            // rare coactivations. Metadata/tag held fixed, no update.
            let mut ood_rng = SplitMix64::new(stream(
                base, TEST_SALT ^ 0x2222_2222_2222_2222, context, episode
            ));
            let mut test_noise_rng = SplitMix64::new(stream(
                base, LABEL_SALT ^ TEST_SALT, context, episode
            ));
            for query_idx in 0..OOD_TEST {
                let mut query = (0..width)
                    .map(|_| ood_rng.next_u64() & 1 != 0)
                    .collect::<Vec<_>>();
                if query_idx >= OOD_RANDOM {
                    query[a] = true;
                    query[b] = true;
                }
                let start = Instant::now();
                let contextual = pair.predict(&query, contextual_prior)?;
                let pooled = pair.predict(&query, pooled_prior)?;
                let uniform = pair.predict(&query, 0.5)?;
                result.inference_ns += start.elapsed().as_nanos();
                if query_idx >= OOD_RANDOM {
                    if !contextual.disagreement {
                        return Err("forced OOD disagreement witness unexpectedly identical".into());
                    }
                    result.forced_ood_differences += 1;
                }
                let truth = if teacher_is_alt {
                    contextual.alternative
                } else {
                    contextual.reference
                };
                let flipped = noisy && test_noise_rng.next_u64() & 15 == 0;
                let noisy_label = truth ^ flipped;
                result.ood_queries += 1;
                if contextual.disagreement {
                    result.ood_disagreements += 1;
                    result.contextual_correct += u64::from(contextual.forced == truth);
                    result.pooled_correct += u64::from(pooled.forced == truth);
                    result.uniform_correct += u64::from(uniform.forced == truth);
                    let target = f64::from(u8::from(truth));
                    result.contextual_brier_sum += (contextual.probability_true - target).powi(2);
                    result.pooled_brier_sum += (pooled.probability_true - target).powi(2);
                    result.uniform_brier_sum += (uniform.probability_true - target).powi(2);
                    result.confidence_sum += contextual_prior.max(1.0 - contextual_prior);
                } else {
                    let accepted = contextual.consensus_only.ok_or("missing consensus prediction")?;
                    result.selective_covered += 1;
                    result.selective_clean_errors += u64::from(accepted != truth);
                    result.selective_noisy_errors += u64::from(accepted != noisy_label);
                }
            }
        }
    }
    if result.episodes != 64
        || result.passive_observations != 64 * (PASSIVE_TRAIN + PASSIVE_VAL) as u64
        || result.passive_disagreements != 0
        || result.ood_queries != 64 * OOD_TEST as u64
        || result.forced_ood_differences != 64 * OOD_FORCED as u64
        || result.selective_clean_errors != 0
        || result.selective_covered + result.ood_disagreements != result.ood_queries
    {
        return Err("epistemic sample counts or observational invariants violated".into());
    }
    Ok(result)
}

fn main() -> Result<(), Box<dyn Error>> {
    println!(
        "A6_I1_CONFIG,widths=8;16;32;64;128;256;1024,seeds=3,passive_modes=exclusive;frozen_zero,panels=clean;noise1_in16,test_regimes=matched;reversed;uninformative,meta_tasks_per_context=64,blind_tasks_per_context=32,passive_train=16,passive_val=16,ood_random=32,ood_forced=16,clean_intervention=1,noisy_intervention=21"
    );
    let mut totals = [Metrics::default(); 3];
    let mut cells = [0usize; 3];
    let mut meta_fits = 0usize;
    for width in WIDTHS {
        for seed_index in 0..3 {
            for mode in PassiveMode::ALL {
                for noisy in [false, true] {
                    let base = block_seed(width, seed_index, mode, noisy);
                    let start = Instant::now();
                    let prior = learn_context_prior(base)?;
                    let meta_us = start.elapsed().as_micros();
                    meta_fits += 1;
                    let p0 = prior.probability_alternative(0)?;
                    let p1 = prior.probability_alternative(1)?;
                    let pg = prior.pooled_probability_alternative()?;
                    if !(p0 < 0.5 && p1 > 0.5) {
                        return Err("learned contextual meta priors unexpectedly inverted".into());
                    }
                    println!(
                        "A6_I1_META,width={width},seed={seed_index},passive_mode={},panel={},historic_tasks={},context0_counts={:?},context1_counts={:?},context0_p1={p0:.6},context1_p1={p1:.6},global_p1={pg:.6},meta_fit_us={meta_us}",
                        mode.name(),
                        if noisy { "label_noise1_in16" } else { "clean" },
                        prior.total_historical(),
                        prior.count_for(0),
                        prior.count_for(1)
                    );
                    for regime in Regime::ALL {
                        let m = run_blind_cell(width, base, mode, noisy, regime, &prior)?;
                        let r = regime.index() as usize;
                        cells[r] += 1;
                        totals[r].add(&m);
                        println!(
                            "A6_I1_CELL,width={width},seed={seed_index},passive_mode={},panel={},regime={},blind_episodes={},passive_observations={},passive_disagreements={},ood_queries={},ood_disagreements={},forced_ood_differences={},contextual_accuracy={:.6},pooled_accuracy={:.6},uniform_accuracy={:.6},contextual_brier={:.6},pooled_brier={:.6},uniform_brier={:.6},selective_coverage={:.6},selective_clean_errors={},selective_noisy_errors={},active_correct={},active_queries={},noisy_passive_flips={},infer_ns={}",
                            mode.name(),
                            if noisy { "label_noise1_in16" } else { "clean" },
                            regime.name(),
                            m.episodes,
                            m.passive_observations,
                            m.passive_disagreements,
                            m.ood_queries,
                            m.ood_disagreements,
                            m.forced_ood_differences,
                            m.accuracy(),
                            m.pooled_accuracy(),
                            m.uniform_accuracy(),
                            m.brier_context(),
                            m.brier_pooled(),
                            m.brier_uniform(),
                            m.coverage(),
                            m.selective_clean_errors,
                            m.selective_noisy_errors,
                            m.intervention_correct,
                            m.active_queries,
                            m.noisy_passive_flips,
                            m.inference_ns
                        );
                    }
                }
            }
        }
    }
    let matched = totals[0];
    let reversed = totals[1];
    let null = totals[2];
    let required_cells = WIDTHS.len() * 3 * PassiveMode::ALL.len() * 2;
    let matched_cal_gap = (matched.accuracy() - matched.expected_confidence()).abs();
    let criterion_matched = matched.accuracy() >= 0.80
        && matched.brier_context() < matched.brier_pooled()
        && matched.brier_context() < matched.brier_uniform()
        && matched_cal_gap < 0.10;
    let criterion_reversed = reversed.accuracy() <= 0.25
        && reversed.brier_context() > reversed.brier_uniform();
    let criterion_null = (0.45..=0.55).contains(&null.accuracy())
        && null.brier_context() >= null.brier_uniform();
    let criterion_consensus = totals.iter().all(|r| {
        r.passive_disagreements == 0
            && r.selective_clean_errors == 0
            && r.ood_disagreements > 0
            && r.selective_covered > 0
    });
    let criterion_intervention = totals.iter().all(|m| {
        (m.intervention_correct as f64 / m.episodes as f64) >= 0.99
    });
    for (index, regime) in Regime::ALL.iter().enumerate() {
        let m = totals[index];
        println!(
            "A6_I1_RESULT,regime={},cells={},blind_tasks={},passive_observations={},passive_disagreements={},ood_queries={},ood_disagreements={},forced_ood_differences={},contextual_accuracy={:.6},pooled_accuracy={:.6},uniform_accuracy={:.6},brier_context={:.6},brier_pooled={:.6},brier_uniform={:.6},mean_confidence={:.6},calibration_gap={:.6},selective_coverage={:.6},selective_clean_errors={},selective_noisy_errors={},active_correct={},active_queries={},inference_ns={}",
            regime.name(),
            cells[index],
            m.episodes,
            m.passive_observations,
            m.passive_disagreements,
            m.ood_queries,
            m.ood_disagreements,
            m.forced_ood_differences,
            m.accuracy(),
            m.pooled_accuracy(),
            m.uniform_accuracy(),
            m.brier_context(),
            m.brier_pooled(),
            m.brier_uniform(),
            m.expected_confidence(),
            (m.accuracy() - m.expected_confidence()).abs(),
            m.coverage(),
            m.selective_clean_errors,
            m.selective_noisy_errors,
            m.intervention_correct,
            m.active_queries,
            m.inference_ns
        );
    }
    let all_pass = cells.iter().all(|&count| count == required_cells)
        && meta_fits == required_cells
        && criterion_matched
        && criterion_reversed
        && criterion_null
        && criterion_consensus
        && criterion_intervention;
    println!(
        "A6_I1_COMPLETE,meta_fits={meta_fits},cells={},matched_pass={criterion_matched},reversed_guard_pass={criterion_reversed},uninformative_guard_pass={criterion_null},consensus_pass={criterion_consensus},intervention_pass={criterion_intervention},pass={all_pass}",
        cells.iter().sum::<usize>()
    );
    if !all_pass {
        return Err("A6-I1 preregistered epistemic stress guards failed".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_widths_and_passive_modes_prove_strict_nonidentifiability() {
        for width in WIDTHS {
            let (pair, a, b) = hypotheses(width, width as u64 ^ 0xabc).unwrap();
            for mode in PassiveMode::ALL {
                let mut rng = SplitMix64::new(width as u64 ^ mode.index());
                for _ in 0..256 {
                    let q = passive_input(width, a, b, &mut rng, mode);
                    let out = pair.predict(&q, 0.92).unwrap();
                    assert!(!out.disagreement);
                    assert!(out.consensus_only.is_some());
                }
            }
            let mut rare = vec![false; width];
            rare[a] = true;
            rare[b] = true;
            assert!(pair.predict(&rare, 0.5).unwrap().disagreement);
        }
    }

    #[test]
    fn real_contextual_meta_probabilities_are_not_task_label_leakage() {
        let prior = learn_context_prior(1977).unwrap();
        assert_eq!(prior.total_historical(), 128);
        assert!(prior.probability_alternative(0).unwrap() < 0.5);
        assert!(prior.probability_alternative(1).unwrap() > 0.5);
        let (_, a, b) = hypotheses(64, 111).unwrap();
        assert_ne!(a, b);
    }
}
