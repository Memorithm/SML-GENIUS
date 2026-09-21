use sml_core::{
    mor::{ExpertProposal, MixtureOfRouters, MorPolicy, PreviousRoute, RouterId, MOR_SCALE_PPM},
    mor_affinity::{
        MorAffinityError, MorAffinityPolicy, NativeAffinityBank, NativeRouterSpec,
        RouterSignatureAtom, SmlFingerprintPlan,
    },
    BooleanWorkspace, PageId,
};

fn atom(bit: u16, expected: bool, weight_ppm: u32) -> RouterSignatureAtom {
    RouterSignatureAtom {
        fingerprint_bit: bit,
        expected,
        weight_ppm,
    }
}

fn router(id: u32, atoms: Vec<RouterSignatureAtom>, proposals: &[(u32, u32)]) -> NativeRouterSpec {
    NativeRouterSpec {
        router_id: RouterId(id),
        atoms,
        reliability_ppm: MOR_SCALE_PPM,
        confidence_ppm: MOR_SCALE_PPM,
        balance_bias_ppm: 0,
        proposals: proposals
            .iter()
            .map(|(page, score)| ExpertProposal {
                page_id: PageId(*page),
                score_ppm: *score,
            })
            .collect(),
    }
}

#[test]
fn fingerprint_samples_only_declared_state_positions() {
    let policy = MorAffinityPolicy::default();
    let plan = SmlFingerprintPlan::new(vec![0, 2, 5], policy).unwrap();
    let state = BooleanWorkspace::from_bits(&[true, false, true, true, false, false, true]);

    let fingerprint = plan.fingerprint(&state).unwrap();

    assert_eq!(fingerprint.len(), 3);
    assert_eq!(fingerprint.get(0), Some(true));
    assert_eq!(fingerprint.get(1), Some(true));
    assert_eq!(fingerprint.get(2), Some(false));
}

#[test]
fn weighted_signature_affinity_is_exact_in_ppm() {
    let policy = MorAffinityPolicy::default();
    let plan = SmlFingerprintPlan::new(vec![0, 1, 2], policy).unwrap();
    let bank = NativeAffinityBank::new(
        plan,
        vec![
            router(
                0,
                vec![
                    atom(0, true, 500_000),
                    atom(1, false, 250_000),
                    atom(2, true, 250_000),
                ],
                &[(0, 900_000)],
            ),
            router(
                1,
                vec![
                    atom(0, false, 500_000),
                    atom(1, false, 250_000),
                    atom(2, true, 250_000),
                ],
                &[(1, 900_000)],
            ),
        ],
        policy,
    )
    .unwrap();
    let state = BooleanWorkspace::from_bits(&[true, false, true]);

    let batch = bank.materialize(&state).unwrap();

    assert_eq!(batch.evidence[0].affinity_ppm, MOR_SCALE_PPM);
    assert_eq!(batch.evidence[0].matched_atoms, 3);
    assert_eq!(batch.evidence[1].affinity_ppm, 500_000);
    assert_eq!(batch.routers[0].affinity_ppm, MOR_SCALE_PPM);
    assert_eq!(batch.routers[1].affinity_ppm, 500_000);
}

#[test]
fn native_affinity_drives_router_competition_end_to_end() {
    let policy = MorAffinityPolicy::default();
    let plan = SmlFingerprintPlan::new(vec![0, 1], policy).unwrap();
    let bank = NativeAffinityBank::new(
        plan,
        vec![
            router(
                0,
                vec![atom(0, true, 500_000), atom(1, false, 500_000)],
                &[(7, 900_000)],
            ),
            router(
                1,
                vec![atom(0, false, 500_000), atom(1, true, 500_000)],
                &[(9, 900_000)],
            ),
        ],
        policy,
    )
    .unwrap();
    let engine = MixtureOfRouters::new(MorPolicy {
        min_routers: 1,
        max_routers: 1,
        router_mass_threshold_ppm: MOR_SCALE_PPM,
        ..MorPolicy::default()
    })
    .unwrap();
    let state = BooleanWorkspace::from_bits(&[true, false]);

    let (decision, batch) = bank
        .route(&engine, &state, &[], &PreviousRoute::default())
        .unwrap();

    assert_eq!(batch.evidence[0].affinity_ppm, MOR_SCALE_PPM);
    assert_eq!(batch.evidence[1].affinity_ppm, 0);
    assert_eq!(decision.active_routers.len(), 1);
    assert_eq!(decision.active_routers[0].router_id, RouterId(0));
    assert_eq!(decision.ranked_experts[0].page_id, PageId(7));
}

#[test]
fn affinity_bank_reports_exact_signature_accounting() {
    let policy = MorAffinityPolicy::default();
    let plan = SmlFingerprintPlan::new(vec![0, 2, 4, 6], policy).unwrap();
    let bank = NativeAffinityBank::new(
        plan,
        vec![
            router(0, vec![atom(0, true, 1), atom(3, false, 1)], &[(0, 1)]),
            router(1, vec![atom(1, true, 1)], &[(1, 1)]),
        ],
        policy,
    )
    .unwrap();

    let accounting = bank.accounting();
    assert_eq!(accounting.fingerprint_bits, 4);
    assert_eq!(accounting.router_count, 2);
    assert_eq!(accounting.signature_atoms, 3);
    assert_eq!(accounting.expected_value_bits, 3);
    assert_eq!(accounting.weight_bits, 96);
    assert_eq!(accounting.position_metadata_bits, 6);
    assert_eq!(accounting.average_atoms_milli, 1_500);
}

#[test]
fn malformed_signature_graph_fails_closed() {
    let policy = MorAffinityPolicy::default();
    assert_eq!(
        SmlFingerprintPlan::new(vec![0, 0], policy).unwrap_err(),
        MorAffinityError::DuplicateFingerprintPosition(0)
    );

    let plan = SmlFingerprintPlan::new(vec![0, 1], policy).unwrap();
    let duplicate_atom = NativeAffinityBank::new(
        plan.clone(),
        vec![router(
            0,
            vec![atom(0, true, 1), atom(0, false, 1)],
            &[(0, 1)],
        )],
        policy,
    )
    .unwrap_err();
    assert_eq!(
        duplicate_atom,
        MorAffinityError::DuplicateSignatureBit {
            router_id: RouterId(0),
            bit: 0,
        }
    );

    let out_of_range = NativeAffinityBank::new(
        plan,
        vec![router(0, vec![atom(2, true, 1)], &[(0, 1)])],
        policy,
    )
    .unwrap_err();
    assert_eq!(
        out_of_range,
        MorAffinityError::SignatureBitOutOfRange {
            router_id: RouterId(0),
            bit: 2,
            fingerprint_width: 2,
        }
    );
}

#[test]
fn workspace_bounds_fail_closed() {
    let policy = MorAffinityPolicy::default();
    let plan = SmlFingerprintPlan::new(vec![0, 3], policy).unwrap();
    let state = BooleanWorkspace::from_bits(&[true, false]);

    assert_eq!(
        plan.fingerprint(&state).unwrap_err(),
        MorAffinityError::WorkspaceTooSmall {
            required_index: 3,
            actual_bits: 2,
        }
    );
}
