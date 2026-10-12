//! Preregistered bounded active-learning evidence for wide sparse ANF.
//! One fixed public selector per hidden positive AND term, with unknown data
//! variable supports. No passive LPN or general Boolean circuit claim.

use std::{collections::BTreeSet, error::Error, time::Instant};

use sml_core::{
    anf_wide_active_learn::{
        fit_selector_anchored_sparse, WideAnchorFit, WideAnchorLearnError, WideAnchorLearnLimits,
    },
    anf_wide_sparse::{WideAnfBackend, WideAnfGate, WideAnfLimits},
    mor::MorDecision,
    pvp6_sbg::{SbgCoordinate, SbgField},
    semantic::SemanticState,
    toy_tasks::SplitMix64,
    wide_anf_selected_pages::{WideAnfPage, WideAnfPageAtlas},
    PageId,
};

const WIDTHS: [usize; 7] = [8, 16, 32, 64, 128, 256, 1024];
const SELECTORS: [usize; 4] = [0, 1, 2, 3];
const HIDDEN_SALT: u64 = 0xA6B1_1001_51ED_0001;
const NOISE_SALT: u64 = 0xA6B1_1002_51ED_0002;
const VAL_SALT: u64 = 0xA6B1_1003_51ED_0003;
const TEST_SALT: u64 = 0xA6B1_1004_51ED_0004;
const SEEDS: usize = 3;
const VAL_RANDOM: usize = 512;
const TEST_RANDOM: usize = 4096;

fn seed(width: usize, index: usize, salt: u64) -> u64 {
    (index as u64 + 1).wrapping_mul(0xd134_2543_de82_ef95) ^ width as u64 ^ salt
}

fn learner_budget() -> WideAnchorLearnLimits {
    WideAnchorLearnLimits::new(2048, 4, 20_000, 8)
}

fn gate_budget() -> WideAnfLimits {
    WideAnfLimits::new(2048, 256, 8192, 8192, 8192)
}

/// Teacher supports are generated here, in the benchmark ONLY. The fitter
/// receives just its Boolean membership callback and the public selectors.
/// Each selector gates one nonempty positive-literal monomial.
fn teacher_terms(width: usize, index: usize) -> Vec<Vec<usize>> {
    let mut rng = SplitMix64::new(seed(width, index, HIDDEN_SALT));
    let available = width - SELECTORS.len();
    (0..SELECTORS.len())
        .map(|selector| {
            let target = [2, 3, 4, 5][selector].min(available);
            let mut support = BTreeSet::new();
            while support.len() < target {
                support.insert(SELECTORS.len() + rng.below(available));
            }
            let mut term = vec![selector];
            term.extend(support);
            term
        })
        .collect()
}

fn teacher_gate(width: usize, terms: Vec<Vec<usize>>) -> Result<WideAnfGate, Box<dyn Error>> {
    Ok(WideAnfGate::from_monomials(
        (0..width).collect(),
        terms,
        width,
        gate_budget(),
    )?)
}

fn probe_once(
    teacher: &WideAnfGate,
    model: &WideAnfGate,
    query: &[bool],
) -> Result<usize, WideAnchorLearnError> {
    let a = teacher.evaluate_scalar(query)?;
    let b = model.evaluate_scalar(query)?;
    Ok(usize::from(a != b))
}

/// All 16 selector configurations with data bits all-one and all-zero,
/// then 512 independent random bits from the frozen VAL RNG. No tuning.
fn validate(
    teacher: &WideAnfGate,
    learned: &WideAnfGate,
    width: usize,
    stream: u64,
) -> Result<(usize, usize), Box<dyn Error>> {
    let mut sample = vec![true; width];
    let mut errors = 0usize;
    let mut comparisons = 0usize;
    for selector_mask in 0..(1 << SELECTORS.len()) {
        for enabled in [true, false] {
            sample.fill(enabled);
            for (i, &selector) in SELECTORS.iter().enumerate() {
                sample[selector] = selector_mask & (1 << i) != 0;
            }
            errors += probe_once(teacher, learned, &sample)?;
            comparisons += 1;
        }
    }
    let mut rng = SplitMix64::new(stream);
    for _ in 0..VAL_RANDOM {
        for bit in &mut sample {
            *bit = rng.next_u64() & 1 != 0;
        }
        errors += probe_once(teacher, learned, &sample)?;
        comparisons += 1;
    }
    Ok((errors, comparisons))
}

/// Generate held-out TEST input/labels strictly after ALL policies freeze.
fn test_heldout(
    teacher: &WideAnfGate,
    learned: &WideAnfGate,
    width: usize,
    stream: u64,
) -> Result<(usize, usize), Box<dyn Error>> {
    let mut rng = SplitMix64::new(stream);
    let mut sample = vec![false; width];
    let mut disagreements = 0usize;
    for _ in 0..TEST_RANDOM {
        for bit in &mut sample {
            *bit = rng.next_u64() & 1 != 0;
        }
        disagreements += probe_once(teacher, learned, &sample)?;
    }
    Ok((disagreements, TEST_RANDOM))
}

fn k8_exhaustive(teacher: &WideAnfGate, learned: &WideAnfGate) -> Result<usize, Box<dyn Error>> {
    let mut differences = 0usize;
    for mask in 0..256usize {
        let q = (0..8).map(|bit| mask & (1 << bit) != 0).collect::<Vec<_>>();
        differences += probe_once(teacher, learned, &q)?;
    }
    Ok(differences)
}

struct Frozen {
    width: usize,
    index: usize,
    noisy: bool,
    teacher: WideAnfGate,
    truth: Vec<Vec<usize>>,
    model: WideAnchorFit,
    validation_errors: usize,
    validation_samples: usize,
}

/// A deliberately unsupported two-selector interaction leaves every
/// one-hot TRAIN answer identical to the supported task, but is observable
/// on VALIDATION when two selectors are simultaneously active.
fn negative_control(width: usize) -> Result<(), Box<dyn Error>> {
    let mut truth = teacher_terms(width, 0);
    truth.push(vec![0, 1]);
    let unsupported = teacher_gate(width, truth)?;
    let fitted = fit_selector_anchored_sparse(
        width,
        &SELECTORS,
        1,
        learner_budget(),
        gate_budget(),
        |query| unsupported.evaluate_scalar(query).map_err(Into::into),
    )?;
    let (errors, val_samples) =
        validate(&unsupported, &fitted.gate, width, seed(width, 0, VAL_SALT))?;
    println!(
        "WIDE_LEARN_NEGATIVE,width={width},extra_monomial=selector0_AND_selector1,fit_was_possible=true,validation_errors={errors},validation_samples={val_samples},accepted={},oracle=PASS",
        errors == 0
    );
    if errors == 0 {
        return Err(format!("out-of-grammar interaction accepted at width {width}").into());
    }
    Ok(())
}

fn sbg_selected_learned_gate(frozen: &[Frozen]) -> Result<(), Box<dyn Error>> {
    let row = frozen
        .iter()
        .find(|f| f.width == 1024 && f.index == 0 && !f.noisy)
        .ok_or("missing k1024 clean learned model")?;
    let learned_gate = row.model.gate.clone();
    let atlas = WideAnfPageAtlas::<1024>::new(
        vec![WideAnfPage {
            id: PageId(17),
            gates: vec![learned_gate],
            inputs: (0..1024)
                .map(|bit| SbgCoordinate {
                    field: SbgField::Value,
                    bit,
                })
                .collect(),
            outputs: vec![SbgCoordinate {
                field: SbgField::Meaning,
                bit: 0,
            }],
        }],
        1,
    )?;
    // Real MorDecision carrier; selection is fixed here, not learned.
    let decision = MorDecision {
        active_routers: vec![],
        ranked_experts: vec![],
        selected_experts: vec![PageId(17)],
        noise_ppm: 0,
        structural_disagreement_ppm: 0,
        expert_mass_threshold_ppm: 0,
        request_precision_escalation: false,
        hysteresis_retained_previous_primary: false,
    };
    let mut comparisons = 0usize;
    for mask in 0..16usize {
        let mut inputs = vec![true; 1024];
        for &selector in &SELECTORS {
            inputs[selector] = mask & (1 << selector) != 0;
        }
        // Perturb a non-selector on half of the checks; no training or
        // validation data is used for these SBG replay checks.
        if mask % 2 != 0 {
            inputs[1023] = false;
        }
        let mut state = SemanticState::<1024>::zeroed()?;
        for (index, &bit) in inputs.iter().enumerate() {
            if bit {
                state.value.set(index, true)?;
            }
        }
        let result = atlas.execute(&state, &decision, 1, 1)?;
        if result.state.meaning_mask.get(0)? != row.teacher.evaluate_scalar(&inputs)? {
            return Err("learned wide gate disagreed with teacher inside selected SBG page".into());
        }
        if result.traces.len() != 1 || result.traces[0].id != PageId(17) {
            return Err("selected wide ANF page trace malformed".into());
        }
        comparisons += 1;
    }
    println!(
        "WIDE_LEARN_SBG,width=1024,seed=0,panel=clean,selected_page=17,real_wide_gate=true,oracle_comparisons={comparisons},bit_disagreements=0,oracle=PASS"
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    println!(
        "WIDE_LEARN_CONFIG,widths=8;16;32;64;128;256;1024,seeds=3,panels=clean;noise_1_in_16,selectors=4,monomials=4,clean_repeats=1,noisy_repeats=15,training=adaptive_membership_only,val=544,test=4096,max_physical_queries=20000"
    );
    let mut frozen = Vec::new();
    for width in WIDTHS {
        for index in 0..SEEDS {
            // The two TRAIN panels share the same underlying teacher,
            // but their callback noise streams and fit selection differ.
            let truth = teacher_terms(width, index);
            let oracle = teacher_gate(width, truth.clone())?;
            for noisy in [false, true] {
                let panel = if noisy { "noise_1_in_16" } else { "clean" };
                let repetitions = if noisy { 15 } else { 1 };
                let mut noise_rng = SplitMix64::new(seed(width, index, NOISE_SALT));
                let mut flipped_training_labels = 0usize;
                let start = Instant::now();
                // This closure is the only observation surface. In
                // particular, no truth list reaches the learner.
                let learned = fit_selector_anchored_sparse(
                    width,
                    &SELECTORS,
                    repetitions,
                    learner_budget(),
                    gate_budget(),
                    |q| {
                        let native = oracle.evaluate_scalar(q)?;
                        let flip = noisy && noise_rng.next_u64() & 15 == 0;
                        if flip {
                            flipped_training_labels += 1;
                        }
                        Ok(native ^ flip)
                    },
                )?;
                let fit_ms = start.elapsed().as_secs_f64() * 1000.0;
                let (validation_errors, validation_samples) =
                    validate(&oracle, &learned.gate, width, seed(width, index, VAL_SALT))?;
                let structural_exact = learned.learned_monomials == truth;
                println!(
                    "WIDE_LEARN_FROZEN,width={width},seed={index},panel={panel},method=anchored_adaptive_group_testing,terms=4,recovered_terms={},hidden_literals={},query_logical={},query_physical={},train_noise_flips={},validation_errors={validation_errors},validation_samples={validation_samples},structure_exact={structural_exact},backend={:?},payload_bytes={},scratch_words={},fit_ms={fit_ms:.6},accepted={}",
                    learned.learned_monomials.len(),
                    learned.recovered_hidden_literals,
                    learned.logical_queries,
                    learned.physical_queries,
                    flipped_training_labels,
                    learned.gate.backend(),
                    learned.gate.payload_bytes(),
                    learned.gate.scratch_words(),
                    validation_errors == 0,
                );
                if learned.gate.backend() != WideAnfBackend::Sparse {
                    return Err("sparse active learner unexpectedly selected dense backend".into());
                }
                frozen.push(Frozen {
                    width,
                    index,
                    noisy,
                    teacher: oracle.clone(),
                    truth: truth.clone(),
                    model: learned,
                    validation_errors,
                    validation_samples,
                });
            }
        }
    }
    if frozen.len() != 42 {
        return Err("expected exactly 42 frozen fitted policies".into());
    }
    println!("WIDE_LEARN_ALL_FROZEN,policies=42,distinct_teachers=21");
    // These negative fixtures are a separate misspecification audit,
    // performed only after positive fitting and before held-out TEST.
    for width in WIDTHS {
        negative_control(width)?;
    }

    // TEST: never generate a single TEST query before all proposals and VAL
    // decisions are frozen. Report failures rather than refitting.
    let mut all_exact = 0usize;
    let mut all_accepted = 0usize;
    let mut all_zero_test = 0usize;
    let mut k8_exhaustive_pass = 0usize;
    for row in &frozen {
        let panel = if row.noisy { "noise_1_in_16" } else { "clean" };
        let (test_disagreements, test_samples) = test_heldout(
            &row.teacher,
            &row.model.gate,
            row.width,
            seed(row.width, row.index, TEST_SALT),
        )?;
        let true_monomials = row.model.learned_monomials == row.truth;
        let accepted = row.validation_errors == 0;
        let exhaustive = if row.width == 8 {
            let errors = k8_exhaustive(&row.teacher, &row.model.gate)?;
            if errors == 0 {
                k8_exhaustive_pass += 1;
            }
            Some(errors)
        } else {
            None
        };
        all_exact += usize::from(true_monomials);
        all_accepted += usize::from(accepted);
        all_zero_test += usize::from(test_disagreements == 0);
        println!(
            "WIDE_LEARN_TEST,width={},seed={},panel={panel},method=anchored_adaptive_group_testing,structure_exact={true_monomials},validation_errors={},validation_samples={},accepted={accepted},test_disagreements={test_disagreements},test_samples={test_samples},k8_exhaustive_errors={:?},query_logical={},query_physical={},hidden_literals={},oracle={}",
            row.width,
            row.index,
            row.validation_errors,
            row.validation_samples,
            exhaustive,
            row.model.logical_queries,
            row.model.physical_queries,
            row.model.recovered_hidden_literals,
            if accepted && true_monomials && test_disagreements == 0 && exhaustive.is_none_or(|e| e == 0) { "PASS" } else { "FAIL" }
        );
    }
    sbg_selected_learned_gate(&frozen)?;
    println!(
        "WIDE_LEARN_COMPLETE,policies=42,structure_exact={all_exact},validation_accepted={all_accepted},zero_test_disagreements={all_zero_test},negative_rejected=7,k8_exhaustive_pass={k8_exhaustive_pass},sbg_learned_gate=true"
    );
    if all_exact != 42 || all_accepted != 42 || all_zero_test != 42 || k8_exhaustive_pass != 6 {
        return Err("at least one preregistered wide sparse recovery or TEST failed".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_truth_generation_is_reproducible_and_well_formed() {
        for width in WIDTHS {
            let a = teacher_terms(width, 0);
            assert_eq!(a, teacher_terms(width, 0));
            assert_eq!(a.len(), 4);
            for (selector, term) in a.iter().enumerate() {
                assert_eq!(term[0], selector);
                assert!(term.len() >= 2);
                assert!(term.len() <= 6);
                assert!(term.iter().skip(1).all(|&i| (4..width).contains(&i)));
                let unique = term.iter().copied().collect::<BTreeSet<_>>();
                assert_eq!(unique.len(), term.len());
            }
        }
    }

    #[test]
    fn negative_interaction_is_train_invisible_but_validation_visible() {
        for width in WIDTHS {
            negative_control(width).unwrap();
        }
    }
}
