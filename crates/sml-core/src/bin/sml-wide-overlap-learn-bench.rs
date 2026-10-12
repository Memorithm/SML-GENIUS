//! A6-WL3: preregistered 126-fit overlap and unknown-term-count ANF panel.
//! True supports are hidden inside this benchmark's generator; the learner
//! receives only active TRAIN membership callback outputs. 126 TEST datasets
//! are generated only after all fit+VAL decisions have frozen.

use std::{collections::BTreeSet, error::Error, time::Instant};

use sml_core::{
    anf_wide_overlap_learn::{
        fit_overlap_sparse, OverlapFit, OverlapLearnError, OverlapLearnLimits,
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
const FAMILIES: [Family; 3] = [Family::Star4, Family::Chain3, Family::Nested2];
const SEEDS: usize = 3;
const HIDDEN_SALT: u64 = 0xA6B1_3001_51ED_0001;
const NOISE_SALT: u64 = 0xA6B1_3002_51ED_0002;
const VAL_SALT: u64 = 0xA6B1_3003_51ED_0003;
const TEST_SALT: u64 = 0xA6B1_3004_51ED_0004;
const PROPOSE_SALT: u64 = 0xA6B1_3005_51ED_0005;
const SBG_SALT: u64 = 0xA6B1_3006_51ED_0006;
const VAL_RANDOM: usize = 512;
const VAL_STRUCTURED: usize = 32;
const TEST_RANDOM: usize = 4096;
const MAX_PHYSICAL: usize = 160_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    Star4,
    Chain3,
    Nested2,
}
impl Family {
    fn index(self) -> usize {
        match self {
            Self::Star4 => 0,
            Self::Chain3 => 1,
            Self::Nested2 => 2,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Star4 => "star4",
            Self::Chain3 => "chain3",
            Self::Nested2 => "nested2",
        }
    }
    fn term_count(self) -> usize {
        match self {
            Self::Star4 => 4,
            Self::Chain3 => 3,
            Self::Nested2 => 2,
        }
    }
}

fn seed(width: usize, family: Family, index: usize, salt: u64) -> u64 {
    (index as u64 + 1).wrapping_mul(0xd134_2543_de82_ef95)
        ^ (family.index() as u64 + 1).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ width as u64
        ^ salt
}

fn learner_limits() -> OverlapLearnLimits {
    OverlapLearnLimits::new(2048, 4, 2, 5, MAX_PHYSICAL, 512)
}
fn gate_limits() -> WideAnfLimits {
    WideAnfLimits::new(2048, 256, 8192, 8192, 8192)
}

fn role_supports(width: usize, family: Family) -> Vec<Vec<usize>> {
    match (width, family) {
        (8, Family::Star4) => vec![
            vec![0, 1],
            vec![0, 2],
            vec![0, 3],
            vec![0, 4],
        ],
        (_, Family::Star4) => vec![
            vec![0, 1],
            vec![0, 2, 3],
            vec![0, 4, 5, 6],
            vec![0, 7, 8, 9, 10],
        ],
        (8, Family::Chain3) => vec![
            vec![0, 1],
            vec![1, 2],
            vec![2, 3],
        ],
        (_, Family::Chain3) => vec![
            vec![0, 1],
            vec![1, 2, 3],
            vec![3, 4, 5, 6],
        ],
        (_, Family::Nested2) => vec![vec![0, 1], vec![0, 1, 2, 3, 4]],
    }
}

/// All role→physical mappings are randomized independently and HIDDEN from
/// the training code, which gets only oracle query input/output pairs.
fn permuted_roles(width: usize, family: Family, index: usize) -> Vec<usize> {
    let mut permutation = (0..width).collect::<Vec<_>>();
    let mut rng = SplitMix64::new(seed(width, family, index, HIDDEN_SALT));
    for i in (1..width).rev() {
        let j = rng.below(i + 1);
        permutation.swap(i, j);
    }
    permutation
}

fn truth(width: usize, family: Family, index: usize) -> Vec<Vec<usize>> {
    let permutation = permuted_roles(width, family, index);
    let mut monomials = role_supports(width, family)
        .into_iter()
        .map(|role| {
            let mut term = role.into_iter().map(|position| permutation[position]).collect::<Vec<_>>();
            term.sort_unstable();
            term
        })
        .collect::<Vec<_>>();
    monomials.sort_unstable();
    monomials
}

fn teacher(width: usize, terms: Vec<Vec<usize>>) -> Result<WideAnfGate, Box<dyn Error>> {
    Ok(WideAnfGate::from_monomials(
        (0..width).collect(),
        terms,
        width,
        gate_limits(),
    )?)
}
fn difference(
    original: &WideAnfGate,
    model: &WideAnfGate,
    assignment: &[bool],
) -> Result<usize, Box<dyn Error>> {
    Ok(usize::from(
        original.evaluate_scalar(assignment)? != model.evaluate_scalar(assignment)?,
    ))
}

fn validate(
    original: &WideAnfGate,
    candidate: &WideAnfGate,
    width: usize,
    terms: &[Vec<usize>],
    stream: u64,
) -> Result<(usize, usize), Box<dyn Error>> {
    let mut rng = SplitMix64::new(stream);
    let mut assignment = vec![false; width];
    let mut errors = 0usize;
    let mut samples = 0usize;
    for _ in 0..VAL_RANDOM {
        for bit in &mut assignment {
            *bit = rng.next_u64() & 1 != 0;
        }
        errors += difference(original, candidate, &assignment)?;
        samples += 1;
    }
    let used = terms.iter().flat_map(|t| t.iter().copied()).collect::<BTreeSet<_>>();
    for background in [false, true] {
        for mask in 0..16usize {
            assignment.fill(background);
            for &pos in &used {
                assignment[pos] = false;
            }
            for (term_index, term) in terms.iter().enumerate() {
                if mask & (1 << term_index) != 0 {
                    for &pos in term {
                        assignment[pos] = true;
                    }
                }
            }
            errors += difference(original, candidate, &assignment)?;
            samples += 1;
        }
    }
    assert_eq!(samples, VAL_RANDOM + VAL_STRUCTURED);
    Ok((errors, samples))
}

fn heldout(
    original: &WideAnfGate,
    candidate: &WideAnfGate,
    width: usize,
    stream: u64,
) -> Result<usize, Box<dyn Error>> {
    let mut rng = SplitMix64::new(stream);
    let mut assignment = vec![false; width];
    let mut errors = 0usize;
    for _ in 0..TEST_RANDOM {
        for bit in &mut assignment {
            *bit = rng.next_u64() & 1 != 0;
        }
        errors += difference(original, candidate, &assignment)?;
    }
    Ok(errors)
}
fn exhaustive_k8(original: &WideAnfGate, candidate: &WideAnfGate) -> Result<usize, Box<dyn Error>> {
    let mut errors = 0usize;
    for mask in 0..256usize {
        let assignment = (0..8).map(|i| mask & (1 << i) != 0).collect::<Vec<_>>();
        errors += difference(original, candidate, &assignment)?;
    }
    Ok(errors)
}

struct Frozen {
    width: usize,
    family: Family,
    index: usize,
    noisy: bool,
    original: WideAnfGate,
    true_terms: Vec<Vec<usize>>,
    model: Option<OverlapFit>,
    fit_error: Option<OverlapLearnError>,
    val_errors: Option<usize>,
    val_samples: usize,
    fit_ms: f64,
    train_noise_flips: usize,
}

fn negative_control(width: usize) -> Result<bool, Box<dyn Error>> {
    let family = Family::Star4;
    let mut terms = truth(width, family, 0);
    let permutation = permuted_roles(width, family, 0);
    let mut extra = permutation[..6].to_vec();
    extra.sort_unstable();
    terms.push(extra);
    let original = teacher(width, terms.clone())?;
    let mut actual_flips = 0usize;
    let fit = fit_overlap_sparse(
        width,
        1,
        seed(width, family, 0, PROPOSE_SALT),
        learner_limits(),
        gate_limits(),
        |input| {
            let val = original.evaluate_scalar(input)?;
            let _ = &mut actual_flips;
            Ok(val)
        },
    );
    match fit {
        Err(error) => {
            println!(
                "A6_WL3_NEGATIVE,width={width},family=star4_plus_degree6,fit_rejected=true,reason={error:?},validation_errors=None,accepted=false"
            );
            Ok(true)
        }
        Ok(candidate) => {
            let (val_errors, val_samples) = validate(
                &original,
                &candidate.gate,
                width,
                &terms,
                seed(width, family, 0, VAL_SALT),
            )?;
            println!(
                "A6_WL3_NEGATIVE,width={width},family=star4_plus_degree6,fit_rejected=false,validation_errors=Some({val_errors}),validation_samples={val_samples},accepted={},found_terms={}",
                val_errors == 0,
                candidate.learned_monomials.len(),
            );
            Ok(val_errors > 0)
        }
    }
}

fn replay_learned_sbg(frozen: &[Frozen]) -> Result<usize, Box<dyn Error>> {
    let item = frozen
        .iter()
        .find(|r| r.width == 1024 && r.family == Family::Star4 && r.index == 0 && !r.noisy)
        .ok_or("missing frozen k1024 STAR4 teacher")?;
    let fitted = item.model.as_ref().ok_or("k1024 STAR4 clean fit failed")?;
    let atlas = WideAnfPageAtlas::<1024>::new(
        vec![WideAnfPage {
            id: PageId(23),
            gates: vec![fitted.gate.clone()],
            inputs: (0..1024)
                .map(|bit| SbgCoordinate { field: SbgField::Value, bit })
                .collect(),
            outputs: vec![SbgCoordinate { field: SbgField::Meaning, bit: 0 }],
        }],
        1,
    )
    .map_err(|e| format!("failed to create SBG page: {e:?}"))?;
    let decision = MorDecision {
        active_routers: vec![],
        ranked_experts: vec![],
        selected_experts: vec![PageId(23)],
        noise_ppm: 0,
        structural_disagreement_ppm: 0,
        expert_mass_threshold_ppm: 0,
        request_precision_escalation: false,
        hysteresis_retained_previous_primary: false,
    };
    let mut rng = SplitMix64::new(seed(1024, Family::Star4, 0, SBG_SALT));
    let mut disagreed = 0usize;
    for case in 0..16 {
        let mut input = (0..1024)
            .map(|_| rng.next_u64() & 1 != 0)
            .collect::<Vec<_>>();
        if case == 0 {
            input.fill(false);
        } else if case == 1 {
            input.fill(true);
        } else if case <= 5 {
            input.fill(false);
            for &variable in &item.true_terms[case - 2] {
                input[variable] = true;
            }
        }
        let mut state = SemanticState::<1024>::zeroed()?;
        for (index, &bit) in input.iter().enumerate() {
            if bit {
                state.value.set(index, true)?;
            }
        }
        let got = atlas
            .execute(&state, &decision, 1, 1)
            .map_err(|e| format!("learned SBG page execution failed: {e:?}"))?;
        let observed = got.state.meaning_mask.get(0)?;
        let expected = item.original.evaluate_scalar(&input)?;
        disagreed += usize::from(observed != expected);
        if got.traces.len() != 1 || got.traces[0].id != PageId(23) {
            return Err("incorrect selected MOR PageId trace".into());
        }
    }
    println!(
        "A6_WL3_SBG,width=1024,family=star4,seed=0,panel=clean,page=23,learned_sparse_gate=true,comparisons=16,bit_disagreements={disagreed},oracle={}",
        if disagreed == 0 { "PASS" } else { "FAIL" },
    );
    Ok(disagreed)
}

fn main() -> Result<(), Box<dyn Error>> {
    println!(
        "A6_WL3_CONFIG,widths=8;16;32;64;128;256;1024,families=star4;chain3;nested2,seeds=3,panels=clean;noise_1_in_16,selectors=unknown,term_count=unknown_2_to_4,degree_max=5,max_physical_queries={MAX_PHYSICAL},stop_trials=512,noise_votes=21,VAL=544,TEST=4096"
    );
    let mut frozen = Vec::<Frozen>::new();
    for width in WIDTHS {
        for family in FAMILIES {
            for index in 0..SEEDS {
                let true_terms = truth(width, family, index);
                let original = teacher(width, true_terms.clone())?;
                for noisy in [false, true] {
                    let panel = if noisy { "noise_1_in_16" } else { "clean" };
                    let repetitions = if noisy { 21 } else { 1 };
                    let mut noise_rng = SplitMix64::new(seed(width, family, index, NOISE_SALT));
                    let mut flipped = 0usize;
                    let start = Instant::now();
                    let attempt = fit_overlap_sparse(
                        width,
                        repetitions,
                        seed(width, family, index, PROPOSE_SALT),
                        learner_limits(),
                        gate_limits(),
                        |q| {
                            let native = original.evaluate_scalar(q)?;
                            let flip = noisy && noise_rng.next_u64() & 15 == 0;
                            flipped += usize::from(flip);
                            Ok(native ^ flip)
                        },
                    );
                    let fit_ms = start.elapsed().as_secs_f64() * 1000.0;
                    let (model, fit_error) = match attempt {
                        Ok(fit) => (Some(fit), None),
                        Err(error) => (None, Some(error)),
                    };
                    let (val_errors, val_samples) = if let Some(fit) = &model {
                        let (errors, count) = validate(
                            &original,
                            &fit.gate,
                            width,
                            &true_terms,
                            seed(width, family, index, VAL_SALT),
                        )?;
                        (Some(errors), count)
                    } else {
                        (None, 0)
                    };
                    let exact = model.as_ref().is_some_and(|x| x.learned_monomials == true_terms);
                    println!(
                        "A6_WL3_FROZEN,width={width},family={},seed={index},panel={panel},true_term_count={},recovered_terms={},structure_exact={exact},fit_error={fit_error:?},validation_errors={val_errors:?},validation_samples={val_samples},query_logical={},query_physical={},witness_attempts={},candidate_rejections={},mobius_certificates={},zero_residual_probes={},train_noise_flips={flipped},payload_bytes={},scratch_words={},fit_ms={fit_ms:.6},accepted={}",
                        family.name(),
                        family.term_count(),
                        model.as_ref().map_or(0, |x| x.learned_monomials.len()),
                        model.as_ref().map_or(0, |x| x.logical_queries),
                        model.as_ref().map_or(0, |x| x.physical_queries),
                        model.as_ref().map_or(0, |x| x.attempted_witnesses),
                        model.as_ref().map_or(0, |x| x.candidate_rejections),
                        model.as_ref().map_or(0, |x| x.mobius_certificates),
                        model.as_ref().map_or(0, |x| x.zero_residual_probes),
                        model.as_ref().map_or(0, |x| x.gate.payload_bytes()),
                        model.as_ref().map_or(0, |x| x.gate.scratch_words()),
                        val_errors == Some(0),
                    );
                    frozen.push(Frozen {
                        width, family, index, noisy, original: original.clone(),
                        true_terms: true_terms.clone(), model, fit_error,
                        val_errors, val_samples, fit_ms, train_noise_flips: flipped,
                    });
                }
            }
        }
    }
    if frozen.len() != 126 {
        return Err("expected 126 frozen policies".into());
    }
    println!("A6_WL3_ALL_FROZEN,policies=126,distinct_teachers=63");
    let mut negatives_pass = 0usize;
    for width in WIDTHS {
        if negative_control(width)? {
            negatives_pass += 1;
        }
    }
    // TEST is created only after EVERY TRAIN/VAL decision and the fixed
    // negative-control audit. No previously generated TEST data is reused
    // for fitting or model-order stopping.
    let mut structural = 0usize;
    let mut validation_pass = 0usize;
    let mut test_pass = 0usize;
    let mut exhaustive_pass = 0usize;
    for row in &frozen {
        let panel = if row.noisy { "noise_1_in_16" } else { "clean" };
        let (test_errors, exhaustive) = if let Some(fit) = &row.model {
            let test = heldout(
                &row.original,
                &fit.gate,
                row.width,
                seed(row.width, row.family, row.index, TEST_SALT),
            )?;
            let exhaustive = if row.width == 8 {
                Some(exhaustive_k8(&row.original, &fit.gate)?)
            } else {
                None
            };
            (Some(test), exhaustive)
        } else {
            (None, None)
        };
        let exact = row.model.as_ref().is_some_and(|m| m.learned_monomials == row.true_terms);
        structural += usize::from(exact);
        validation_pass += usize::from(row.val_errors == Some(0));
        test_pass += usize::from(test_errors == Some(0));
        if exhaustive == Some(0) {
            exhaustive_pass += 1;
        }
        let passed = exact && row.val_errors == Some(0) && test_errors == Some(0)
            && (row.width != 8 || exhaustive == Some(0));
        println!(
            "A6_WL3_TEST,width={},family={},seed={},panel={panel},true_term_count={},recovered_terms={},structure_exact={exact},fit_error={:?},validation_errors={:?},validation_samples={},bit_disagreements={test_errors:?},test_samples={TEST_RANDOM},k8_exhaustive_errors={exhaustive:?},query_physical={},train_noise_flips={},fit_ms={:.6},oracle={}",
            row.width, row.family.name(), row.index,
            row.family.term_count(),
            row.model.as_ref().map_or(0, |x| x.learned_monomials.len()),
            row.fit_error, row.val_errors, row.val_samples,
            row.model.as_ref().map_or(0, |x| x.physical_queries),
            row.train_noise_flips, row.fit_ms,
            if passed { "PASS" } else { "FAIL" },
        );
    }
    let sbg_disagree = if structural == 126 {
        replay_learned_sbg(&frozen)?
    } else {
        println!(
            "A6_WL3_SBG,width=1024,family=star4,seed=0,panel=clean,page=23,learned_sparse_gate=false,comparisons=0,bit_disagreements=NA,oracle=FAIL"
        );
        usize::MAX
    };
    println!(
        "A6_WL3_COMPLETE,policies=126,structure_exact={structural},validation_pass={validation_pass},zero_test={test_pass},negative_rejected={negatives_pass},k8_exhaustive_pass={exhaustive_pass},sbg_bit_disagreements={sbg_disagree}"
    );
    if structural != 126 || validation_pass != 126 || test_pass != 126
        || negatives_pass != 7 || exhaustive_pass != 18 || sbg_disagree != 0
    {
        return Err("A6-WL3 preregistered identification/rejection criteria failed".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hidden_teacher_has_expected_overlap_geometry_and_orders() {
        for width in WIDTHS {
            for family in FAMILIES {
                for index in 0..SEEDS {
                    let terms = truth(width, family, index);
                    assert_eq!(terms.len(), family.term_count());
                    let mut seen = BTreeSet::new();
                    for term in &terms {
                        assert!((2..=5).contains(&term.len()));
                        assert!(term.iter().all(|&j| j < width));
                        let unique = term.iter().copied().collect::<BTreeSet<_>>();
                        assert_eq!(unique.len(), term.len());
                        seen.extend(unique);
                    }
                    assert!(seen.len() < terms.iter().map(Vec::len).sum::<usize>());
                    assert!(terms.windows(2).all(|w| w[0] < w[1]));
                }
            }
        }
    }
    #[test]
    fn all_negative_fixtures_are_detectable_or_rejected() {
        for width in WIDTHS {
            assert!(negative_control(width).unwrap(), "k={width}");
        }
    }
}
