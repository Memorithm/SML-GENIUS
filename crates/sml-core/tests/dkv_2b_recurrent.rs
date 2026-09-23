use sml_core::{
    delta_kv::{DeltaKvCounters, DeltaKvMemory, DELTA_KV_SCALE},
    delta_kv_recurrent::{
        compare_delta_kv_recurrent, DeltaKvRecurrentError, DeltaKvRecurrentKernel,
        DeltaKvRecurrentPolicy,
    },
    delta_kv_semantic::{
        DeltaKvSemanticProjectionPolicy, DeltaKvSemanticProjector, DeltaKvSourceProvenance,
    },
    memory::{BoundedAssociativeMemory, ReadOutcome, ReplacementPolicy},
    mor::MorDecision,
    semantic::{run_semantic_recurrent, SemanticHaltReason, SemanticState},
    BooleanWorkspace, ExecutionCounters, HaltingPolicy, PageId,
};

fn decision(page: u32) -> MorDecision {
    MorDecision {
        active_routers: Vec::new(),
        ranked_experts: Vec::new(),
        selected_experts: vec![PageId(page)],
        noise_ppm: 0,
        structural_disagreement_ppm: 0,
        expert_mass_threshold_ppm: 0,
        request_precision_escalation: false,
        hysteresis_retained_previous_primary: false,
    }
}

fn initial_state() -> SemanticState<8> {
    let mut state = SemanticState::<8>::zeroed().unwrap();
    state.routing.set(3, true).unwrap();
    state.meaning_mask.set(0, true).unwrap();
    state.meaning_mask.set(1, true).unwrap();
    state
}

fn kernel(seed: bool) -> DeltaKvRecurrentKernel<8> {
    let state = initial_state();
    let projector = DeltaKvSemanticProjector::<8>::new(
        DeltaKvSemanticProjectionPolicy {
            semantic_weight: 1,
            mor_weight: 32,
        },
        17,
    )
    .unwrap();
    let provenance = DeltaKvSourceProvenance::new(77, vec![1000, 1001]).unwrap();
    let key = projector
        .project(&state, &decision(5), Some(&provenance))
        .unwrap();
    let memory = DeltaKvMemory::new(8, 2).unwrap();
    let mut kernel = DeltaKvRecurrentKernel::<8>::new(
        memory,
        key,
        vec![0, 1],
        DeltaKvRecurrentPolicy {
            ambiguity_threshold_q12: 128,
        },
    )
    .unwrap();
    if seed {
        kernel
            .seed_target(&[2000, -2000], DELTA_KV_SCALE as u16)
            .unwrap();
    }
    kernel
}

#[test]
fn seeded_delta_kv_changes_state_then_adaptive_recurrence_halts_on_stability() {
    let comparison = compare_delta_kv_recurrent(
        initial_state(),
        kernel(true),
        HaltingPolicy {
            min_depth: 1,
            max_depth: 6,
            stable_hamming_threshold: 0,
        },
        3,
    )
    .unwrap();

    assert_eq!(comparison.adaptive.depth, 2);
    assert_eq!(comparison.adaptive.halt_reason, SemanticHaltReason::Stable);
    assert_eq!(comparison.adaptive.trace[0].delta.value, 1);
    assert!(comparison.adaptive.trace[1].delta.is_zero());
    assert_eq!(comparison.adaptive_cycles.len(), 2);

    assert_eq!(comparison.fixed_depth.depth, 3);
    assert_eq!(comparison.fixed_depth.halt_reason, SemanticHaltReason::Stable);
    assert_eq!(comparison.fixed_cycles.len(), 3);

    assert_eq!(comparison.no_delta.depth, 1);
    assert_eq!(comparison.no_delta.halt_reason, SemanticHaltReason::Stable);
    assert!(comparison.no_delta.trace[0].delta.is_zero());

    assert_eq!(comparison.adaptive.state.value.get(0), Ok(true));
    assert_eq!(comparison.adaptive.state.value.get(1), Ok(false));
}

#[test]
fn unresolved_zero_read_blocks_stability_but_not_hard_depth_cap() {
    let comparison = compare_delta_kv_recurrent(
        initial_state(),
        kernel(false),
        HaltingPolicy {
            min_depth: 1,
            max_depth: 3,
            stable_hamming_threshold: 0,
        },
        3,
    )
    .unwrap();

    assert_eq!(comparison.adaptive.depth, 3);
    assert_eq!(comparison.adaptive.halt_reason, SemanticHaltReason::MaxDepth);
    assert!(comparison
        .adaptive
        .trace
        .iter()
        .all(|entry| entry.stability_blocked));
    assert!(comparison
        .adaptive_cycles
        .iter()
        .all(|cycle| cycle.ambiguous_values == 2));
}

#[test]
fn recurrent_kernel_retains_external_provenance_without_storing_it_in_matrix_api() {
    let kernel = kernel(true);
    let provenance = kernel.key_evidence().provenance.as_ref().unwrap();

    assert_eq!(provenance.semantic_record_id, 77);
    assert_eq!(provenance.raw_segment_ids, vec![1000, 1001]);
    assert_eq!(kernel.key_evidence().key_q12.len(), 8);
}

#[test]
fn seed_and_recurrent_reads_have_explicit_delta_kv_counters() {
    let mut kernel = kernel(true);
    let outcome = run_semantic_recurrent(
        initial_state(),
        &mut kernel,
        HaltingPolicy {
            min_depth: 1,
            max_depth: 4,
            stable_hamming_threshold: 0,
        },
    )
    .unwrap();

    assert_eq!(outcome.depth, 2);
    let counters = kernel.counters();
    assert_eq!(counters.updates, 1);
    assert_eq!(counters.reads, 2);
    assert_eq!(kernel.cycle_evidence().len(), 2);
    assert!(kernel
        .cycle_evidence()
        .iter()
        .all(|cycle| cycle.read_macs_delta == 16));
    assert_eq!(counters.read_macs, 48);
}

#[test]
fn value_coordinates_must_be_unique_in_range_and_match_memory_width() {
    let state = initial_state();
    let projector =
        DeltaKvSemanticProjector::<8>::new(DeltaKvSemanticProjectionPolicy::default(), 1).unwrap();
    let key = projector.project(&state, &decision(1), None).unwrap();

    assert_eq!(
        DeltaKvRecurrentKernel::<8>::new(
            DeltaKvMemory::new(8, 2).unwrap(),
            key.clone(),
            vec![0],
            DeltaKvRecurrentPolicy::default(),
        ),
        Err(DeltaKvRecurrentError::ValueDimensionMismatch {
            expected: 2,
            actual: 1,
        })
    );
    assert_eq!(
        DeltaKvRecurrentKernel::<8>::new(
            DeltaKvMemory::new(8, 2).unwrap(),
            key.clone(),
            vec![0, 0],
            DeltaKvRecurrentPolicy::default(),
        ),
        Err(DeltaKvRecurrentError::DuplicateValueCoordinate)
    );
    assert_eq!(
        DeltaKvRecurrentKernel::<8>::new(
            DeltaKvMemory::new(8, 2).unwrap(),
            key,
            vec![0, 8],
            DeltaKvRecurrentPolicy::default(),
        ),
        Err(DeltaKvRecurrentError::ValueCoordinateOutOfRange)
    );
}

#[test]
fn key_width_mismatch_and_negative_ambiguity_threshold_fail_closed() {
    let state = initial_state();
    let projector =
        DeltaKvSemanticProjector::<4>::new(DeltaKvSemanticProjectionPolicy::default(), 1).unwrap();
    let key = projector.project(&state, &decision(1), None).unwrap();

    assert_eq!(
        DeltaKvRecurrentKernel::<8>::new(
            DeltaKvMemory::new(8, 2).unwrap(),
            key,
            vec![0, 1],
            DeltaKvRecurrentPolicy::default(),
        ),
        Err(DeltaKvRecurrentError::KeyDimensionMismatch {
            expected: 8,
            actual: 4,
        })
    );

    let good =
        DeltaKvSemanticProjector::<8>::new(DeltaKvSemanticProjectionPolicy::default(), 1)
            .unwrap()
            .project(&state, &decision(1), None)
            .unwrap();
    assert_eq!(
        DeltaKvRecurrentKernel::<8>::new(
            DeltaKvMemory::new(8, 2).unwrap(),
            good,
            vec![0, 1],
            DeltaKvRecurrentPolicy {
                ambiguity_threshold_q12: -1,
            },
        ),
        Err(DeltaKvRecurrentError::InvalidPolicy)
    );
}

#[test]
fn direct_associative_memory_remains_an_independent_coexisting_mechanism() {
    let mut direct =
        BoundedAssociativeMemory::new(8, 2, ReplacementPolicy::RejectCollision).unwrap();
    let mut counters = ExecutionCounters::default();
    direct
        .write(
            42,
            BooleanWorkspace::from_bits(&[true, false]),
            &mut counters,
        )
        .unwrap();

    let _comparison = compare_delta_kv_recurrent(
        initial_state(),
        kernel(true),
        HaltingPolicy {
            min_depth: 1,
            max_depth: 4,
            stable_hamming_threshold: 0,
        },
        3,
    )
    .unwrap();

    match direct.read(42, &mut counters) {
        ReadOutcome::Hit { value, .. } => {
            assert_eq!(value.get(0), Some(true));
            assert_eq!(value.get(1), Some(false));
        }
        other => panic!("direct memory unexpectedly changed: {other:?}"),
    }
}

#[test]
fn fixed_query_projection_is_reused_across_recurrent_cycles() {
    let mut kernel = kernel(true);
    let key_before = kernel.key_evidence().key_q12.clone();
    let _ = run_semantic_recurrent(
        initial_state(),
        &mut kernel,
        HaltingPolicy {
            min_depth: 1,
            max_depth: 4,
            stable_hamming_threshold: 0,
        },
    )
    .unwrap();

    assert_eq!(kernel.key_evidence().key_q12, key_before);
    assert_eq!(kernel.cycle_evidence().len(), 2);
}

#[test]
fn unused_counter_type_remains_compatible_with_reference_memory_evidence() {
    let counters = DeltaKvCounters::default();
    assert_eq!(counters.reads, 0);
    assert_eq!(counters.updates, 0);
}
