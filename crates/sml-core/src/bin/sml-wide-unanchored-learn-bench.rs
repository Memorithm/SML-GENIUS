//! A6-WL2 preregistered selector-free positive sparse ANF identification.
//! All true supports exist ONLY in this test generator/validator, not in the
//! train-only active learner. TEST queries generated after every frozen fit.

use std::{collections::BTreeSet, error::Error, time::Instant};

use sml_core::{
    anf_wide_sparse::{WideAnfBackend, WideAnfGate, WideAnfLimits},
    anf_wide_unanchored_learn::{fit_unanchored_disjoint_sparse, UnanchoredFit, UnanchoredLimits},
    mor::MorDecision,
    pvp6_sbg::{SbgCoordinate, SbgField},
    semantic::SemanticState,
    toy_tasks::SplitMix64,
    wide_anf_selected_pages::{WideAnfPage, WideAnfPageAtlas},
    PageId,
};

const WIDTHS: [usize; 7] = [8, 16, 32, 64, 128, 256, 1024];
const SEEDS: usize = 3;
const HIDDEN_SALT: u64 = 0xA6B1_2001_51ED_0001;
const NOISE_SALT: u64 = 0xA6B1_2002_51ED_0002;
const VAL_SALT: u64 = 0xA6B1_2003_51ED_0003;
const TEST_SALT: u64 = 0xA6B1_2004_51ED_0004;
const WITNESS_SALT: u64 = 0xA6B1_2005_51ED_0005;
const SBG_SALT: u64 = 0xA6B1_2006_51ED_0006;
const VAL_RANDOM: usize = 512;
const VAL_STRUCTURED: usize = 32;
const TEST_RANDOM: usize = 4096;

fn seed(width: usize, index: usize, salt: u64) -> u64 {
    (index as u64 + 1).wrapping_mul(0xd134_2543_de82_ef95) ^ width as u64 ^ salt
}
fn learn_limits() -> UnanchoredLimits {
    UnanchoredLimits::new(2048, 4, 5, 25_000, 512)
}
fn gate_limits() -> WideAnfLimits {
    WideAnfLimits::new(2048, 256, 8192, 8192, 8192)
}

/// Hidden disjoint-support teacher generated independently of learner PRNG.
fn truth(width: usize, index: usize) -> Vec<Vec<usize>> {
    let mut rng = SplitMix64::new(seed(width, index, HIDDEN_SALT));
    let mut positions = (0..width).collect::<Vec<_>>();
    for i in (1..width).rev() {
        let j = rng.below(i + 1);
        positions.swap(i, j);
    }
    let degrees = if width == 8 {
        [2, 2, 2, 2]
    } else {
        [2, 3, 4, 5]
    };
    let mut cursor = 0usize;
    let mut terms = Vec::new();
    for degree in degrees {
        let mut monomial = positions[cursor..cursor + degree].to_vec();
        monomial.sort_unstable();
        terms.push(monomial);
        cursor += degree;
    }
    terms.sort_unstable();
    terms
}
fn native_teacher(width: usize, terms: Vec<Vec<usize>>) -> Result<WideAnfGate, Box<dyn Error>> {
    Ok(WideAnfGate::from_monomials(
        (0..width).collect(),
        terms,
        width,
        gate_limits(),
    )?)
}
fn disagreement(
    teacher: &WideAnfGate,
    candidate: &WideAnfGate,
    query: &[bool],
) -> Result<usize, Box<dyn Error>> {
    Ok(usize::from(
        teacher.evaluate_scalar(query)? != candidate.evaluate_scalar(query)?,
    ))
}

/// Validator truth is used only AFTER the fitted support is immutable:
/// 512 random masks plus 2 x 16 exact combinations of true supports,
/// first without and then with irrelevant (non-support) variables enabled.
fn validation(
    teacher: &WideAnfGate,
    model: &WideAnfGate,
    width: usize,
    teacher_terms: &[Vec<usize>],
    stream: u64,
) -> Result<(usize, usize), Box<dyn Error>> {
    let mut sample = vec![false; width];
    let mut errors = 0usize;
    let mut checked = 0usize;
    let mut rng = SplitMix64::new(stream);
    for _ in 0..VAL_RANDOM {
        for bit in &mut sample {
            *bit = rng.next_u64() & 1 != 0;
        }
        errors += disagreement(teacher, model, &sample)?;
        checked += 1;
    }
    let used = teacher_terms
        .iter()
        .flat_map(|term| term.iter().copied())
        .collect::<BTreeSet<_>>();
    for background in [false, true] {
        for mask in 0..16usize {
            sample.fill(background);
            for &variable in &used {
                sample[variable] = false;
            }
            for (term_index, term) in teacher_terms.iter().enumerate() {
                if mask & (1 << term_index) != 0 {
                    for &position in term {
                        sample[position] = true;
                    }
                }
            }
            errors += disagreement(teacher, model, &sample)?;
            checked += 1;
        }
    }
    assert_eq!(checked, VAL_RANDOM + VAL_STRUCTURED);
    Ok((errors, checked))
}

fn heldout(
    teacher: &WideAnfGate,
    model: &WideAnfGate,
    width: usize,
    stream: u64,
) -> Result<usize, Box<dyn Error>> {
    let mut rng = SplitMix64::new(stream);
    let mut sample = vec![false; width];
    let mut errors = 0usize;
    for _ in 0..TEST_RANDOM {
        for bit in &mut sample {
            *bit = rng.next_u64() & 1 != 0;
        }
        errors += disagreement(teacher, model, &sample)?;
    }
    Ok(errors)
}
fn exhaustive_k8(teacher: &WideAnfGate, model: &WideAnfGate) -> Result<usize, Box<dyn Error>> {
    let mut errors = 0usize;
    for mask in 0..256usize {
        let query = (0..8)
            .map(|index| mask & (1 << index) != 0)
            .collect::<Vec<_>>();
        errors += disagreement(teacher, model, &query)?;
    }
    Ok(errors)
}

struct Frozen {
    width: usize,
    index: usize,
    noisy: bool,
    teacher: WideAnfGate,
    terms: Vec<Vec<usize>>,
    fit: UnanchoredFit,
    val_errors: usize,
    val_samples: usize,
}

fn negative_control(width: usize) -> Result<(), Box<dyn Error>> {
    let base = truth(width, 0);
    let mut invalid_terms = base.clone();
    invalid_terms.push((0..width).collect());
    let unsupported = native_teacher(width, invalid_terms)?;
    let fit = fit_unanchored_disjoint_sparse(
        width,
        1,
        seed(width, 0, WITNESS_SALT),
        learn_limits(),
        gate_limits(),
        |q| unsupported.evaluate_scalar(q).map_err(Into::into),
    );
    match fit {
        Err(err) => {
            println!(
                "WIDE_UNANCHORED_NEGATIVE,width={width},extra_term=full_degree_AND,fit_rejected=true,reason={err:?},validation_errors=None,accepted=false,oracle=PASS"
            );
        }
        Ok(candidate) => {
            let (errors, total) = validation(
                &unsupported,
                &candidate.gate,
                width,
                &base,
                seed(width, 0, VAL_SALT),
            )?;
            if errors == 0 {
                return Err(format!("unsupported five-term teacher accepted for k{width}").into());
            }
            println!(
                "WIDE_UNANCHORED_NEGATIVE,width={width},extra_term=full_degree_AND,fit_rejected=false,validation_errors=Some({errors}),validation_samples={total},accepted=false,oracle=PASS"
            );
        }
    }
    Ok(())
}

fn selected_learned_sbg(frozen: &[Frozen]) -> Result<(), Box<dyn Error>> {
    let item = frozen
        .iter()
        .find(|item| item.width == 1024 && item.index == 0 && !item.noisy)
        .ok_or("k1024 clean learned model not frozen")?;
    let atlas = WideAnfPageAtlas::<1024>::new(
        vec![WideAnfPage {
            id: PageId(19),
            gates: vec![item.fit.gate.clone()],
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
    )
    .map_err(|e| format!("failed to build learned page: {e:?}"))?;
    let selected = MorDecision {
        active_routers: vec![],
        ranked_experts: vec![],
        selected_experts: vec![PageId(19)],
        noise_ppm: 0,
        structural_disagreement_ppm: 0,
        expert_mass_threshold_ppm: 0,
        request_precision_escalation: false,
        hysteresis_retained_previous_primary: false,
    };
    let mut rng = SplitMix64::new(seed(1024, 0, SBG_SALT));
    let mut disagreements = 0usize;
    for case in 0..16 {
        let mut bits = (0..1024)
            .map(|_| rng.next_u64() & 1 != 0)
            .collect::<Vec<_>>();
        if case == 0 {
            bits.fill(false);
        } else if case == 1 {
            bits.fill(true);
        } else if case <= 5 {
            bits.fill(false);
            for &variable in &item.terms[case - 2] {
                bits[variable] = true;
            }
        }
        let mut sbg = SemanticState::<1024>::zeroed()?;
        for (index, &on) in bits.iter().enumerate() {
            if on {
                sbg.value.set(index, true)?;
            }
        }
        let executed = atlas
            .execute(&sbg, &selected, 1, 1)
            .map_err(|e| format!("selected learned SBG page failed: {e:?}"))?;
        let output = executed.state.meaning_mask.get(0)?;
        let expected = item.teacher.evaluate_scalar(&bits)?;
        disagreements += usize::from(output != expected);
        if executed.traces.len() != 1 || executed.traces[0].id != PageId(19) {
            return Err("selected page evidence mismatched".into());
        }
    }
    println!(
        "WIDE_UNANCHORED_SBG,width=1024,seed=0,panel=clean,page=19,learned_gate=true,comparisons=16,bit_disagreements={disagreements},oracle={}",
        if disagreements == 0 { "PASS" } else { "FAIL" }
    );
    if disagreements != 0 {
        return Err("learned selector-free sparse SBG page diverged from oracle".into());
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    println!(
        "WIDE_UNANCHORED_CONFIG,widths=8;16;32;64;128;256;1024,seeds=3,panels=clean;noise_1_in_16,unknown_selectors=true,disjoint_terms=4,degree_max=5,clean_repeats=1,noisy_repeats=21,max_witness_attempts=512,max_physical_queries=25000,val=544,test=4096"
    );
    let mut frozen = Vec::new();
    for width in WIDTHS {
        for index in 0..SEEDS {
            let true_terms = truth(width, index);
            let teacher = native_teacher(width, true_terms.clone())?;
            for noisy in [false, true] {
                let panel = if noisy { "noise_1_in_16" } else { "clean" };
                let repeats = if noisy { 21 } else { 1 };
                let mut noise_rng = SplitMix64::new(seed(width, index, NOISE_SALT));
                let mut noise_flips = 0usize;
                let start = Instant::now();
                let fit = fit_unanchored_disjoint_sparse(
                    width,
                    repeats,
                    seed(width, index, WITNESS_SALT),
                    learn_limits(),
                    gate_limits(),
                    |q| {
                        let output = teacher.evaluate_scalar(q)?;
                        let flip = noisy && noise_rng.next_u64() & 15 == 0;
                        if flip {
                            noise_flips += 1;
                        }
                        Ok(output ^ flip)
                    },
                )?;
                let fit_ms = start.elapsed().as_secs_f64() * 1000.0;
                let (val_errors, val_samples) = validation(
                    &teacher,
                    &fit.gate,
                    width,
                    &true_terms,
                    seed(width, index, VAL_SALT),
                )?;
                let structure_exact = fit.learned_monomials == true_terms;
                println!(
                    "WIDE_UNANCHORED_FROZEN,width={width},seed={index},panel={panel},method=unanchored_residual_witness,terms=4,structure_exact={structure_exact},validation_errors={val_errors},validation_samples={val_samples},query_logical={},query_physical={},witness_attempts={},rejected_witnesses={},train_noise_flips={noise_flips},backend={:?},payload_bytes={},scratch_words={},fit_ms={fit_ms:.6},accepted={}",
                    fit.logical_queries,
                    fit.physical_queries,
                    fit.witness_attempts,
                    fit.rejected_witnesses,
                    fit.gate.backend(),
                    fit.gate.payload_bytes(),
                    fit.gate.scratch_words(),
                    val_errors == 0
                );
                if fit.gate.backend() != WideAnfBackend::Sparse {
                    return Err("learned four-term support unexpectedly got dense backend".into());
                }
                frozen.push(Frozen {
                    width,
                    index,
                    noisy,
                    teacher: teacher.clone(),
                    terms: true_terms.clone(),
                    fit,
                    val_errors,
                    val_samples,
                });
            }
        }
    }
    if frozen.len() != 42 {
        return Err("frozen policy count must be 42".into());
    }
    println!("WIDE_UNANCHORED_ALL_FROZEN,policies=42,distinct_teachers=21");
    for width in WIDTHS {
        negative_control(width)?;
    }
    // No held-out queries may be materialized before all 42 fit/VAL
    // policies and all 7 fixed negative controls have been evaluated.
    let mut exact = 0usize;
    let mut valid = 0usize;
    let mut zero_test = 0usize;
    let mut exhaustive_pass = 0usize;
    for item in &frozen {
        let disagreements = heldout(
            &item.teacher,
            &item.fit.gate,
            item.width,
            seed(item.width, item.index, TEST_SALT),
        )?;
        let strict_structural = item.fit.learned_monomials == item.terms;
        let accepted = item.val_errors == 0;
        let brute = if item.width == 8 {
            let mismatches = exhaustive_k8(&item.teacher, &item.fit.gate)?;
            if mismatches == 0 {
                exhaustive_pass += 1;
            }
            Some(mismatches)
        } else {
            None
        };
        exact += usize::from(strict_structural);
        valid += usize::from(accepted);
        zero_test += usize::from(disagreements == 0);
        println!(
            "WIDE_UNANCHORED_TEST,width={},seed={},panel={},structure_exact={strict_structural},validation_errors={},validation_samples={},accepted={accepted},bit_disagreements={disagreements},test_samples={TEST_RANDOM},k8_exhaustive_errors={brute:?},query_logical={},query_physical={},witness_attempts={},rejected_witnesses={},oracle={}",
            item.width, item.index,
            if item.noisy { "noise_1_in_16" } else { "clean" },
            item.val_errors, item.val_samples,
            item.fit.logical_queries, item.fit.physical_queries,
            item.fit.witness_attempts, item.fit.rejected_witnesses,
            if strict_structural && accepted && disagreements == 0 && brute.is_none_or(|x| x == 0) {
                "PASS"
            } else {
                "FAIL"
            }
        );
    }
    selected_learned_sbg(&frozen)?;
    println!(
        "WIDE_UNANCHORED_COMPLETE,policies=42,structure_exact={exact},validation_pass={valid},zero_test={zero_test},negative_rejected=7,k8_exhaustive_pass={exhaustive_pass},sbg_learned_gate=true"
    );
    if exact != 42 || valid != 42 || zero_test != 42 || exhaustive_pass != 6 {
        return Err("predeclared four-term unanchored identification threshold failed".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_has_no_special_selector_and_every_support_is_disjoint() {
        for width in WIDTHS {
            for seed_index in 0..SEEDS {
                let terms = truth(width, seed_index);
                assert_eq!(terms.len(), 4);
                let mut seen = BTreeSet::new();
                for term in terms {
                    assert!((2..=5).contains(&term.len()));
                    for pos in term {
                        assert!(pos < width);
                        assert!(seen.insert(pos));
                    }
                }
            }
        }
    }

    #[test]
    fn frozen_validation_rejects_high_degree_extra_term() {
        negative_control(16).unwrap();
    }
}
