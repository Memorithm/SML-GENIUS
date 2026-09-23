//! DKV-2c frozen dynamic re-keying mechanism evidence.
//! Protocol: docs/SML-DKV-2C-PREREGISTRATION.md.

use std::{error::Error, fmt::Debug, io};

use sml_core::{
    delta_kv::{DeltaKvCounters, DeltaKvMemory, DELTA_KV_SCALE},
    delta_kv_dynamic::DeltaKvDynamicRekeyKernel,
    delta_kv_recurrent::{DeltaKvRecurrentKernel, DeltaKvRecurrentPolicy},
    delta_kv_semantic::{
        DeltaKvSemanticKeyEvidence, DeltaKvSemanticProjectionCounters,
        DeltaKvSemanticProjectionPolicy, DeltaKvSemanticProjector,
    },
    mor::MorDecision,
    semantic::{
        run_semantic_recurrent, SemanticHaltReason, SemanticRecurrentOutcome, SemanticRewriteStep,
        SemanticRewriter, SemanticState,
    },
    HaltingPolicy, PageId,
};

const PROTOCOL: &str = "sml-dkv-2c-dynamic-rekey/v1";
const BITS: usize = 8;
const KEY_DIM: usize = 8;
const VALUE_DIM: usize = 2;
const PROJECTOR_SEED: u64 = 17;
const AMBIGUITY_THRESHOLD_Q12: i32 = 128;
const TARGET_MAGNITUDE_Q12: i16 = 2_000;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FixtureKind {
    F0Single,
    F1Chain,
    F2Reversal,
    F3MissingSuccessor,
}

impl FixtureKind {
    const fn label(self) -> &'static str {
        match self {
            Self::F0Single => "F0_single",
            Self::F1Chain => "F1_two_state_chain",
            Self::F2Reversal => "F2_reversal_chain",
            Self::F3MissingSuccessor => "F3_missing_successor",
        }
    }

    const fn final_target(self) -> Option<[bool; VALUE_DIM]> {
        match self {
            Self::F0Single => Some([true, false]),
            Self::F1Chain => Some([true, true]),
            Self::F2Reversal => Some([false, true]),
            Self::F3MissingSuccessor => None,
        }
    }

    const fn successor_target(self) -> Option<[bool; VALUE_DIM]> {
        match self {
            Self::F1Chain => Some([true, true]),
            Self::F2Reversal => Some([false, true]),
            Self::F0Single | Self::F3MissingSuccessor => None,
        }
    }
}

#[derive(Debug, Clone)]
struct Fixture {
    kind: FixtureKind,
    initial: SemanticState<BITS>,
    memory: DeltaKvMemory,
    initial_key: DeltaKvSemanticKeyEvidence,
    projector: DeltaKvSemanticProjector<KEY_DIM>,
    mor: MorDecision,
    seed_counters: DeltaKvCounters,
}

#[derive(Debug, Clone)]
struct ArmEvidence {
    fixture: &'static str,
    arm: &'static str,
    final_v0: bool,
    final_v1: bool,
    target_exact: Option<bool>,
    depth: u32,
    halt_reason: SemanticHaltReason,
    projections: usize,
    projection_counters: DeltaKvSemanticProjectionCounters,
    reads: u64,
    read_macs: u64,
    distinct_keys: usize,
    key_changes: usize,
    ambiguous_total: usize,
    seed_updates: u64,
    seed_saturations: u64,
    key_digests: String,
    ambiguous_by_cycle: String,
}

impl ArmEvidence {
    fn csv(&self) -> String {
        format!(
            "{},{},{},{},{},{},{:?},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            self.fixture,
            self.arm,
            u8::from(self.final_v0),
            u8::from(self.final_v1),
            self.target_exact
                .map_or_else(|| "NA".to_owned(), |value| u8::from(value).to_string()),
            self.depth,
            self.halt_reason,
            self.projections,
            self.projection_counters.semantic_coordinates_read,
            self.projection_counters.semantic_fields_read,
            self.projection_counters.mor_selected_experts_read,
            self.projection_counters.bucket_accumulations,
            self.projection_counters.quantized_key_coordinates,
            self.reads,
            self.read_macs,
            self.distinct_keys,
            self.key_changes,
            self.ambiguous_total,
            self.seed_updates,
            self.seed_saturations,
            self.key_digests,
            self.ambiguous_by_cycle,
        )
    }
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

fn projector() -> Result<DeltaKvSemanticProjector<KEY_DIM>> {
    Ok(DeltaKvSemanticProjector::<KEY_DIM>::new(
        DeltaKvSemanticProjectionPolicy {
            semantic_weight: 1,
            mor_weight: 32,
        },
        PROJECTOR_SEED,
    )?)
}

fn recurrent_policy() -> DeltaKvRecurrentPolicy {
    DeltaKvRecurrentPolicy {
        ambiguity_threshold_q12: AMBIGUITY_THRESHOLD_Q12,
    }
}

fn adaptive_policy() -> HaltingPolicy {
    HaltingPolicy {
        min_depth: 1,
        max_depth: 4,
        stable_hamming_threshold: 0,
    }
}

fn fixed4_policy() -> HaltingPolicy {
    HaltingPolicy {
        min_depth: 4,
        max_depth: 4,
        stable_hamming_threshold: 0,
    }
}

fn initial_state() -> Result<SemanticState<BITS>> {
    let mut state = SemanticState::<BITS>::zeroed()?;
    state.routing.set(3, true)?;
    state.meaning_mask.set(0, true)?;
    state.meaning_mask.set(1, true)?;
    Ok(state)
}

fn apply_target(
    mut state: SemanticState<BITS>,
    target: [bool; VALUE_DIM],
) -> Result<SemanticState<BITS>> {
    for (coordinate, value) in target.into_iter().enumerate() {
        state.value.set(coordinate, value)?;
        state.ambiguity.set(coordinate, false)?;
    }
    Ok(state)
}

fn target_q12(target: [bool; VALUE_DIM]) -> [i16; VALUE_DIM] {
    target.map(|value| {
        if value {
            TARGET_MAGNITUDE_Q12
        } else {
            -TARGET_MAGNITUDE_Q12
        }
    })
}

fn build_fixture(kind: FixtureKind) -> Result<Fixture> {
    let initial = initial_state()?;
    let projector = projector()?;
    let mor = mor();
    let initial_key = projector.project(&initial, &mor, None)?;
    let first_target = [true, false];
    let first_state = apply_target(initial.clone(), first_target)?;
    let successor_key = projector.project(&first_state, &mor, None)?;

    if kind.successor_target().is_some() && successor_key.key_q12 == initial_key.key_q12 {
        return Err(io::Error::other("DKV-2c successor key collapsed to K0").into());
    }

    let mut memory = DeltaKvMemory::new(KEY_DIM, VALUE_DIM)?;
    let mut seed_counters = DeltaKvCounters::default();
    memory.update(
        &initial_key.key_q12,
        &target_q12(first_target),
        DELTA_KV_SCALE as u16,
        &mut seed_counters,
    )?;

    if let Some(target) = kind.successor_target() {
        memory.update(
            &successor_key.key_q12,
            &target_q12(target),
            DELTA_KV_SCALE as u16,
            &mut seed_counters,
        )?;
    }

    Ok(Fixture {
        kind,
        initial,
        memory,
        initial_key,
        projector,
        mor,
        seed_counters,
    })
}

fn target_exact(
    outcome: &SemanticRecurrentOutcome<BITS>,
    target: Option<[bool; VALUE_DIM]>,
) -> Result<Option<bool>> {
    target
        .map(|target| {
            Ok(outcome.state.value.get(0)? == target[0]
                && outcome.state.value.get(1)? == target[1])
        })
        .transpose()
}

fn key_digest(key: &[i16]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for value in key {
        for byte in value.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

fn boxed_debug<E: Debug>(label: &str, error: E) -> Box<dyn Error> {
    io::Error::other(format!("{label}: {error:?}")).into()
}

fn frozen_arm(fixture: &Fixture) -> Result<ArmEvidence> {
    let mut kernel = DeltaKvRecurrentKernel::<BITS>::new(
        fixture.memory.clone(),
        fixture.initial_key.clone(),
        vec![0, 1],
        recurrent_policy(),
    )?;
    let outcome = run_semantic_recurrent(fixture.initial.clone(), &mut kernel, adaptive_policy())
        .map_err(|error| boxed_debug("frozen recurrence", error))?;

    let ambiguous_total = kernel
        .cycle_evidence()
        .iter()
        .map(|cycle| cycle.ambiguous_values)
        .sum();
    let digest = format!("{:016x}", key_digest(&fixture.initial_key.key_q12));

    Ok(ArmEvidence {
        fixture: fixture.kind.label(),
        arm: "frozen_adaptive",
        final_v0: outcome.state.value.get(0)?,
        final_v1: outcome.state.value.get(1)?,
        target_exact: target_exact(&outcome, fixture.kind.final_target())?,
        depth: outcome.depth,
        halt_reason: outcome.halt_reason,
        projections: 1,
        projection_counters: fixture.initial_key.counters,
        reads: kernel.counters().reads,
        read_macs: kernel.counters().read_macs,
        distinct_keys: 1,
        key_changes: 0,
        ambiguous_total,
        seed_updates: fixture.seed_counters.updates,
        seed_saturations: fixture.seed_counters.saturations,
        key_digests: digest,
        ambiguous_by_cycle: kernel
            .cycle_evidence()
            .iter()
            .map(|cycle| cycle.ambiguous_values.to_string())
            .collect::<Vec<_>>()
            .join(";"),
    })
}

fn dynamic_arm(fixture: &Fixture, arm: &'static str, policy: HaltingPolicy) -> Result<ArmEvidence> {
    let mut kernel = DeltaKvDynamicRekeyKernel::<BITS, KEY_DIM>::new(
        fixture.memory.clone(),
        fixture.projector,
        fixture.mor.clone(),
        None,
        vec![0, 1],
        recurrent_policy(),
    )?;
    let outcome = run_semantic_recurrent(fixture.initial.clone(), &mut kernel, policy)
        .map_err(|error| boxed_debug("dynamic recurrence", error))?;

    let cycles = kernel.cycle_evidence();
    let key_digests = cycles
        .iter()
        .map(|cycle| format!("{:016x}", key_digest(&cycle.key_q12)))
        .collect::<Vec<_>>()
        .join(";");
    let ambiguous_by_cycle = cycles
        .iter()
        .map(|cycle| cycle.ambiguous_values.to_string())
        .collect::<Vec<_>>()
        .join(";");
    let ambiguous_total = cycles.iter().map(|cycle| cycle.ambiguous_values).sum();

    Ok(ArmEvidence {
        fixture: fixture.kind.label(),
        arm,
        final_v0: outcome.state.value.get(0)?,
        final_v1: outcome.state.value.get(1)?,
        target_exact: target_exact(&outcome, fixture.kind.final_target())?,
        depth: outcome.depth,
        halt_reason: outcome.halt_reason,
        projections: cycles.len(),
        projection_counters: kernel.projection_counters(),
        reads: kernel.counters().reads,
        read_macs: kernel.counters().read_macs,
        distinct_keys: kernel.distinct_keys().len(),
        key_changes: kernel.key_change_count(),
        ambiguous_total,
        seed_updates: fixture.seed_counters.updates,
        seed_saturations: fixture.seed_counters.saturations,
        key_digests,
        ambiguous_by_cycle,
    })
}

#[derive(Debug, Clone, Copy)]
struct IdentityRewriter;

impl SemanticRewriter<BITS> for IdentityRewriter {
    type Error = ();

    fn rewrite(
        &mut self,
        state: &SemanticState<BITS>,
        _depth: u32,
    ) -> std::result::Result<SemanticRewriteStep<BITS>, Self::Error> {
        Ok(SemanticRewriteStep {
            state: state.clone(),
            stability_blocked: false,
        })
    }
}

fn no_delta_arm(fixture: &Fixture) -> Result<ArmEvidence> {
    let mut identity = IdentityRewriter;
    let outcome = run_semantic_recurrent(fixture.initial.clone(), &mut identity, adaptive_policy())
        .map_err(|error| boxed_debug("identity recurrence", error))?;
    Ok(ArmEvidence {
        fixture: fixture.kind.label(),
        arm: "no_delta",
        final_v0: outcome.state.value.get(0)?,
        final_v1: outcome.state.value.get(1)?,
        target_exact: target_exact(&outcome, fixture.kind.final_target())?,
        depth: outcome.depth,
        halt_reason: outcome.halt_reason,
        projections: 0,
        projection_counters: DeltaKvSemanticProjectionCounters::default(),
        reads: 0,
        read_macs: 0,
        distinct_keys: 0,
        key_changes: 0,
        ambiguous_total: 0,
        seed_updates: fixture.seed_counters.updates,
        seed_saturations: fixture.seed_counters.saturations,
        key_digests: "-".to_owned(),
        ambiguous_by_cycle: "-".to_owned(),
    })
}

fn run() -> Result<Vec<String>> {
    if PROTOCOL != "sml-dkv-2c-dynamic-rekey/v1" {
        return Err(io::Error::other("DKV-2c protocol identity drifted").into());
    }

    let mut rows = Vec::new();
    for kind in [
        FixtureKind::F0Single,
        FixtureKind::F1Chain,
        FixtureKind::F2Reversal,
        FixtureKind::F3MissingSuccessor,
    ] {
        let fixture = build_fixture(kind)?;
        rows.push(frozen_arm(&fixture)?.csv());
        rows.push(dynamic_arm(&fixture, "dynamic_adaptive", adaptive_policy())?.csv());
        rows.push(dynamic_arm(&fixture, "dynamic_fixed4", fixed4_policy())?.csv());
        rows.push(no_delta_arm(&fixture)?.csv());
    }
    Ok(rows)
}

fn main() -> Result<()> {
    println!(
        "fixture,arm,final_v0,final_v1,target_exact,depth,halt_reason,projections,semantic_coordinates_read,semantic_fields_read,mor_selected_experts_read,bucket_accumulations,quantized_key_coordinates,dkv_reads,read_macs,distinct_keys,key_changes,ambiguous_total,seed_updates,seed_saturations,key_digests,ambiguous_by_cycle"
    );
    for row in run()? {
        println!("{row}");
    }
    Ok(())
}
