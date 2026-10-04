//! DKV-4c semantic-impact comparison on the paired K=64/K=256 panel.
//!
//! Protocol: docs/SML-DKV-4C-SEMANTIC-IMPACT-PREREGISTRATION.md.
//! This is a synthetic development diagnostic, not a model-quality benchmark.

use std::{collections::BTreeMap, error::Error, fmt::Write as _};

use sml_core::{
    delta_kv::{DeltaKvCounters, DeltaKvMemory, DELTA_KV_ONE, DELTA_KV_SCALE},
    delta_kv_adaptive::{
        run_dkv4_recurrent, run_dkv4_semantic_recurrent, Dkv4Controller, Dkv4Decision,
        Dkv4Policy, Dkv4Provenance, Dkv4SemanticController, Dkv4SemanticPolicy,
    },
    delta_kv_recurrent::{
        compare_delta_kv_recurrent, DeltaKvRecurrentKernel, DeltaKvRecurrentPolicy,
    },
    delta_kv_semantic::{DeltaKvSemanticProjectionPolicy, DeltaKvSemanticProjector},
    mor::MorDecision,
    semantic::{SemanticState, SemanticStateDelta},
    HaltingPolicy, PageId,
};

type R<T> = Result<T, Box<dyn Error>>;

const BITS: usize = 64;
const FACTS: usize = 16;
const SEEDS: u64 = 8;
const HAMMING: [usize; 6] = [1, 2, 4, 8, 16, 32];
const POSITIONS: usize = 4 * BITS;
const FULL_RATE: u16 = DELTA_KV_SCALE as u16;

#[derive(Clone, Copy)]
struct Stream(u64);

impl Stream {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

fn base(seed: u64, index: u64) -> u64 {
    seed.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ index.wrapping_add(1).wrapping_mul(0xD6E8_FEB8_6659_FD93)
        ^ 0xA5A5_A5A5_A5A5_A5A5
}

fn state(seed: u64, index: u64) -> SemanticState<BITS> {
    let mut stream = Stream::new(base(seed, index));
    let mut out = SemanticState::<BITS>::zeroed().unwrap();
    for field in [
        &mut out.value,
        &mut out.meaning_mask,
        &mut out.ambiguity,
        &mut out.routing,
    ] {
        for bit in 0..BITS {
            field.set(bit, stream.next() & 1 != 0).unwrap();
        }
    }
    out
}

fn near(seed: u64, fact: usize, hamming: usize) -> SemanticState<BITS> {
    let mut out = state(seed, fact as u64);
    let mut stream = Stream::new(
        base(seed, fact as u64)
            ^ (hamming as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
            ^ 0x1656_67B1_9E37_79F9,
    );
    let mut positions: Vec<usize> = (0..POSITIONS).collect();
    for k in 0..hamming {
        let j = k + (stream.next() % (POSITIONS - k) as u64) as usize;
        positions.swap(k, j);
    }
    for position in &positions[..hamming] {
        let bit = position % BITS;
        let field = match position / BITS {
            0 => &mut out.value,
            1 => &mut out.meaning_mask,
            2 => &mut out.ambiguity,
            _ => &mut out.routing,
        };
        let current = field.get(bit).unwrap();
        field.set(bit, !current).unwrap();
    }
    out
}

fn mor() -> MorDecision {
    MorDecision {
        active_routers: Vec::new(),
        ranked_experts: Vec::new(),
        selected_experts: vec![PageId(5)],
        noise_ppm: 0,
        structural_disagreement_ppm: 0,
        expert_mass_threshold_ppm: 0,
        request_precision_escalation: false,
        hysteresis_retained_previous_primary: false,
    }
}

fn target(fact: usize) -> [bool; 2] {
    [fact & 1 != 0, fact & 2 != 0]
}

fn target_q12(fact: usize) -> [i16; 2] {
    target(fact).map(|bit| if bit { DELTA_KV_ONE } else { -DELTA_KV_ONE })
}

fn answer<const N: usize>(state: &SemanticState<N>) -> [bool; 2] {
    [state.value.get(0).unwrap(), state.value.get(1).unwrap()]
}

#[derive(Default, Clone)]
struct Agg {
    rows: u64,
    written_correct_dkv4: u64,
    written_stop_dkv4: u64,
    written_abstain_dkv4: u64,
    written_cycles_dkv4: u64,
    written_macs_dkv4: u64,
    written_correct_semantic: u64,
    written_stop_semantic: u64,
    written_abstain_semantic: u64,
    written_cycles_semantic: u64,
    written_macs_semantic: u64,
    written_correct_fixed1: u64,
    written_correct_fixed4: u64,
    written_correct_stability: u64,
    written_cycles_stability: u64,
    unknown_safe_reads_semantic: u64,
    unknown_safe_stop_semantic: u64,
    unknown_unverified_stop_semantic: u64,
    unknown_unverified_cycles_semantic: u64,
    projection_coordinates: u64,
    transient_key_bits: u128,
}

fn kernel<const K: usize>(
    memory: DeltaKvMemory,
    evidence: sml_core::delta_kv_semantic::DeltaKvSemanticKeyEvidence,
) -> R<DeltaKvRecurrentKernel<BITS>> {
    Ok(DeltaKvRecurrentKernel::new(
        memory,
        evidence,
        vec![0, 1],
        DeltaKvRecurrentPolicy::default(),
    )?)
}

fn run_k<const K: usize>(
    text: &mut String,
    summaries: &mut BTreeMap<(usize, String), Agg>,
) -> R<()> {
    for seed in 0..SEEDS {
        let states = (0..FACTS)
            .map(|fact| state(seed, fact as u64))
            .collect::<Vec<_>>();
        let projector =
            DeltaKvSemanticProjector::<K>::new(DeltaKvSemanticProjectionPolicy::default(), seed)?;
        let fixed_mor = mor();
        let evidences = states
            .iter()
            .map(|s| projector.project(s, &fixed_mor, None))
            .collect::<Result<Vec<_>, _>>()?;
        let mut memory = DeltaKvMemory::new(K, 2)?;
        let mut ingest = DeltaKvCounters::default();
        for (fact, evidence) in evidences.iter().enumerate() {
            memory.update(&evidence.key_q12, &target_q12(fact), FULL_RATE, &mut ingest)?;
        }

        for fact in 0..FACTS {
            let mut queries = Vec::with_capacity(2 + HAMMING.len());
            queries.push(("written".to_string(), states[fact].clone(), true));
            for h in HAMMING {
                let candidate = near(seed, fact, h);
                let exact_delta = SemanticStateDelta::between(&states[fact], &candidate);
                if exact_delta.total() != h as u128 {
                    return Err("near-query Hamming contract drift".into());
                }
                queries.push((format!("near_{h}"), candidate, false));
            }
            queries.push((
                "independent".to_string(),
                state(seed, 1000 + fact as u64),
                false,
            ));

            for (kind, query, written) in queries {
                if !written && states.iter().any(|s| s == &query) {
                    return Err("unknown fixture equals an admitted written state".into());
                }

                let evidence = projector.project(&query, &fixed_mor, None)?;
                let projected_coords = evidence.counters.quantized_key_coordinates;
                let key_bits = evidence.transient_key_bits;
                let agg = summaries.entry((K, kind.clone())).or_default();
                agg.rows += 1;
                agg.projection_coordinates += projected_coords;
                agg.transient_key_bits += key_bits;

                if written {
                    let expected = target(fact);
                    let adaptive_policy = HaltingPolicy {
                        min_depth: 1,
                        max_depth: 4,
                        stable_hamming_threshold: 0,
                    };
                    let base_kernel = kernel::<K>(memory.clone(), evidence.clone())?;
                    let fixed1 = compare_delta_kv_recurrent(
                        query.clone(),
                        base_kernel.clone(),
                        adaptive_policy,
                        1,
                    )?;
                    let fixed4 = compare_delta_kv_recurrent(
                        query.clone(),
                        base_kernel.clone(),
                        adaptive_policy,
                        4,
                    )?;
                    let mut old_controlled = base_kernel.clone();
                    let old_out = run_dkv4_recurrent(
                        query.clone(),
                        &mut old_controlled,
                        &fixed_mor,
                        Dkv4Provenance::VerifiedPresent,
                        Dkv4Controller::new(Dkv4Policy::default())?,
                    )?;
                    let mut semantic_controlled = base_kernel;
                    let semantic_out = run_dkv4_semantic_recurrent(
                        query.clone(),
                        &mut semantic_controlled,
                        &fixed_mor,
                        Dkv4Provenance::VerifiedPresent,
                        Dkv4SemanticController::new(Dkv4SemanticPolicy::default())?,
                    )?;

                    let dkv4_correct = answer(&old_out.state) == expected;
                    let semantic_correct = answer(&semantic_out.state) == expected;
                    let fixed1_correct = answer(&fixed1.fixed_depth.state) == expected;
                    let fixed4_correct = answer(&fixed4.fixed_depth.state) == expected;
                    let stability_correct = answer(&fixed1.adaptive.state) == expected;
                    agg.written_correct_dkv4 += u64::from(dkv4_correct);
                    agg.written_stop_dkv4 +=
                        u64::from(matches!(old_out.decision, Dkv4Decision::StopCandidate(_)));
                    agg.written_abstain_dkv4 +=
                        u64::from(matches!(old_out.decision, Dkv4Decision::Abstain(_)));
                    agg.written_cycles_dkv4 += u64::from(old_out.depth);
                    let dkv4_macs: u64 = old_out.cycles.iter().map(|c| c.read_macs_delta).sum();
                    agg.written_macs_dkv4 += dkv4_macs;

                    agg.written_correct_semantic += u64::from(semantic_correct);
                    agg.written_stop_semantic +=
                        u64::from(matches!(semantic_out.decision, Dkv4Decision::StopCandidate(_)));
                    agg.written_abstain_semantic +=
                        u64::from(matches!(semantic_out.decision, Dkv4Decision::Abstain(_)));
                    agg.written_cycles_semantic += u64::from(semantic_out.depth);
                    let semantic_macs: u64 =
                        semantic_out.cycles.iter().map(|c| c.read_macs_delta).sum();
                    agg.written_macs_semantic += semantic_macs;

                    agg.written_correct_fixed1 += u64::from(fixed1_correct);
                    agg.written_correct_fixed4 += u64::from(fixed4_correct);
                    agg.written_correct_stability += u64::from(stability_correct);
                    agg.written_cycles_stability += u64::from(fixed1.adaptive.depth);

                    let first_correction = old_out
                        .cycles
                        .first()
                        .map(|c| c.correction_abs_max_q12)
                        .unwrap_or(0);
                    let final_correction = old_out
                        .cycles
                        .last()
                        .map(|c| c.correction_abs_max_q12)
                        .unwrap_or(0);
                    let semantic_final_correction = semantic_out
                        .cycles
                        .last()
                        .map(|c| c.correction_abs_max_q12)
                        .unwrap_or(0);
                    let stability_macs: u64 = fixed1
                        .adaptive_cycles
                        .iter()
                        .map(|c| c.read_macs_delta)
                        .sum();
                    writeln!(
                        text,
                        "ROW,{K},{seed},{fact},{kind},written,{},dkv4_correct={dkv4_correct},dkv4_stop={},dkv4_depth={},dkv4_macs={dkv4_macs},semantic_correct={semantic_correct},semantic_stop={},semantic_depth={},semantic_macs={semantic_macs},first_correction={first_correction},dkv4_final_correction={final_correction},semantic_final_correction={semantic_final_correction},fixed1_correct={fixed1_correct},fixed4_correct={fixed4_correct},stability_correct={stability_correct},stability_depth={},stability_macs={stability_macs},projection_coordinates={projected_coords},key_bits={key_bits}",
                        if expected[0] { 2 } else { 0 } + if expected[1] { 1 } else { 0 },
                        matches!(old_out.decision, Dkv4Decision::StopCandidate(_)),
                        old_out.depth,
                        matches!(semantic_out.decision, Dkv4Decision::StopCandidate(_)),
                        semantic_out.depth,
                        fixed1.adaptive.depth,
                    )?;
                } else {
                    let mut safe_kernel = kernel::<K>(memory.clone(), evidence.clone())?;
                    let safe_before = safe_kernel.counters();
                    let safe = run_dkv4_semantic_recurrent(
                        query.clone(),
                        &mut safe_kernel,
                        &fixed_mor,
                        Dkv4Provenance::Missing,
                        Dkv4SemanticController::new(Dkv4SemanticPolicy::default())?,
                    )?;
                    let safe_after = safe_kernel.counters();
                    let safe_reads = safe_after.reads.saturating_sub(safe_before.reads);
                    if safe.depth != 0 || !safe.cycles.is_empty() {
                        return Err("semantic missing-provenance path executed recurrent memory".into());
                    }
                    let safe_stop =
                        matches!(safe.decision, Dkv4Decision::StopCandidate(_));
                    agg.unknown_safe_reads_semantic += safe_reads;
                    agg.unknown_safe_stop_semantic += u64::from(safe_stop);

                    let mut unverified_kernel = kernel::<K>(memory.clone(), evidence)?;
                    let unverified = run_dkv4_semantic_recurrent(
                        query,
                        &mut unverified_kernel,
                        &fixed_mor,
                        Dkv4Provenance::Unverified,
                        Dkv4SemanticController::new(Dkv4SemanticPolicy::default())?,
                    )?;
                    let unverified_stop =
                        matches!(unverified.decision, Dkv4Decision::StopCandidate(_));
                    if unverified_stop {
                        return Err("semantic unverified path stopped a candidate".into());
                    }
                    agg.unknown_unverified_stop_semantic += u64::from(unverified_stop);
                    agg.unknown_unverified_cycles_semantic += u64::from(unverified.depth);
                    writeln!(
                        text,
                        "ROW,{K},{seed},{fact},{kind},unknown,semantic_safe_reads={safe_reads},semantic_safe_stop={safe_stop},semantic_unverified_stop={unverified_stop},semantic_unverified_depth={},projection_coordinates={projected_coords},key_bits={key_bits}",
                        unverified.depth,
                    )?;
                }
            }
        }

        writeln!(
            text,
            "INGEST,{K},{seed},updates={},prediction_macs={},weight_updates={},saturations={},semantic_bits={}",
            ingest.updates,
            ingest.read_macs,
            ingest.weight_updates,
            ingest.saturations,
            memory.semantic_bits()
        )?;
    }
    Ok(())
}

pub fn report() -> R<String> {
    let mut text = String::from(
        "PROTOCOL,SML-DKV-4C-SEMANTIC-IMPACT-PREREGISTRATION
STATUS,DEVELOPMENT_NOT_MODEL_QUALIFICATION
",
    );
    let mut summaries = BTreeMap::new();
    run_k::<64>(&mut text, &mut summaries)?;
    run_k::<256>(&mut text, &mut summaries)?;

    text.push_str("SUMMARY_HEADER,K,kind,rows,written_correct_dkv4,written_stop_dkv4,written_abstain_dkv4,written_cycles_dkv4,written_macs_dkv4,written_correct_semantic,written_stop_semantic,written_abstain_semantic,written_cycles_semantic,written_macs_semantic,written_correct_fixed1,written_correct_fixed4,written_correct_stability,written_cycles_stability,unknown_safe_reads_semantic,unknown_safe_stop_semantic,unknown_unverified_stop_semantic,unknown_unverified_cycles_semantic,projection_coordinates,transient_key_bits
");
    for ((k, kind), a) in &summaries {
        writeln!(
            text,
            "SUMMARY,{k},{kind},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            a.rows,
            a.written_correct_dkv4,
            a.written_stop_dkv4,
            a.written_abstain_dkv4,
            a.written_cycles_dkv4,
            a.written_macs_dkv4,
            a.written_correct_semantic,
            a.written_stop_semantic,
            a.written_abstain_semantic,
            a.written_cycles_semantic,
            a.written_macs_semantic,
            a.written_correct_fixed1,
            a.written_correct_fixed4,
            a.written_correct_stability,
            a.written_cycles_stability,
            a.unknown_safe_reads_semantic,
            a.unknown_safe_stop_semantic,
            a.unknown_unverified_stop_semantic,
            a.unknown_unverified_cycles_semantic,
            a.projection_coordinates,
            a.transient_key_bits,
        )?;
    }

    let rows: u64 = summaries.values().map(|a| a.rows).sum();
    if rows != (2 * SEEDS as usize * FACTS * (2 + HAMMING.len())) as u64 {
        return Err("incomplete paired dimension grid".into());
    }
    text.push_str(
        "PROMOTION,BLOCKED,semantic_impact_development_panel
",
    );
    Ok(text)
}

#[allow(dead_code)]
fn main() -> R<()> {
    print!("{}", report()?);
    Ok(())
}
