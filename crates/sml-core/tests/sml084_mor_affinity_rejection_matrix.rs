//! Table-driven rejection matrix for every fail-closed branch of
//! `MorAffinityPolicy::validate`, `SmlFingerprintPlan::new`,
//! `SmlFingerprintPlan::fingerprint`, `NativeAffinityBank::new`,
//! `NativeAffinityBank::materialize` and `NativeAffinityBank::route`.
//! Tests only: no behavior change.

use sml_core::{
    mor::{
        ExpertProposal, ExpertRuntimeHint, MixtureOfRouters, MorError, MorPolicy, PreviousRoute,
        RouterId, MOR_SCALE_PPM,
    },
    mor_affinity::{
        MorAffinityError, MorAffinityPolicy, NativeAffinityBank, NativeRouterSpec,
        RouterSignatureAtom, SmlFingerprintPlan,
    },
    BooleanWorkspace, PageId,
};

const BASE: MorAffinityPolicy = MorAffinityPolicy {
    max_fingerprint_bits: 4,
    max_routers: 3,
    max_atoms_per_router: 2,
    max_total_atoms: 4,
};

fn atom(bit: u16, expected: bool, weight_ppm: u32) -> RouterSignatureAtom {
    RouterSignatureAtom {
        fingerprint_bit: bit,
        expected,
        weight_ppm,
    }
}

fn proposal(page: u32, score_ppm: u32) -> ExpertProposal {
    ExpertProposal {
        page_id: PageId(page),
        score_ppm,
    }
}

fn router(id: u32, atoms: Vec<RouterSignatureAtom>) -> NativeRouterSpec {
    NativeRouterSpec {
        router_id: RouterId(id),
        atoms,
        reliability_ppm: MOR_SCALE_PPM,
        confidence_ppm: MOR_SCALE_PPM,
        balance_bias_ppm: 0,
        proposals: vec![proposal(id, 900_000)],
    }
}

fn plan(width: u32) -> SmlFingerprintPlan {
    SmlFingerprintPlan::new((0..width).collect(), BASE).unwrap()
}

fn ok_routers() -> Vec<NativeRouterSpec> {
    vec![
        router(0, vec![atom(0, true, 500_000), atom(1, false, 500_000)]),
        router(1, vec![atom(0, false, 500_000), atom(1, true, 500_000)]),
    ]
}

fn ok_bank() -> NativeAffinityBank {
    NativeAffinityBank::new(plan(2), ok_routers(), BASE).unwrap()
}

#[test]
fn policy_validate_rejection_matrix() {
    let cases = [
        (
            "zero fingerprint budget",
            MorAffinityPolicy {
                max_fingerprint_bits: 0,
                ..BASE
            },
            MorAffinityError::ZeroFingerprintBudget,
        ),
        (
            "zero router budget",
            MorAffinityPolicy {
                max_routers: 0,
                ..BASE
            },
            MorAffinityError::ZeroRouterBudget,
        ),
        (
            "zero atom budget",
            MorAffinityPolicy {
                max_atoms_per_router: 0,
                ..BASE
            },
            MorAffinityError::ZeroAtomBudget,
        ),
        (
            "zero total atom budget",
            MorAffinityPolicy {
                max_total_atoms: 0,
                ..BASE
            },
            MorAffinityError::ZeroTotalAtomBudget,
        ),
    ];
    for (name, policy, expected) in cases {
        assert_eq!(policy.validate().unwrap_err(), expected, "{name}");
        // Both constructors must propagate the policy rejection first.
        assert_eq!(
            SmlFingerprintPlan::new(vec![0], policy).unwrap_err(),
            expected,
            "{name} (plan)"
        );
        assert_eq!(
            NativeAffinityBank::new(plan(2), ok_routers(), policy).unwrap_err(),
            expected,
            "{name} (bank)"
        );
    }
    // Positive control: minimal all-ones policy is valid.
    let minimal = MorAffinityPolicy {
        max_fingerprint_bits: 1,
        max_routers: 1,
        max_atoms_per_router: 1,
        max_total_atoms: 1,
    };
    assert_eq!(minimal.validate(), Ok(minimal));
    assert_eq!(BASE.validate(), Ok(BASE));
}

#[test]
fn fingerprint_plan_rejection_matrix() {
    let cases: Vec<(&str, Vec<u32>, MorAffinityError)> = vec![
        ("empty", vec![], MorAffinityError::EmptyFingerprint),
        (
            "over budget",
            vec![0, 1, 2, 3, 4],
            MorAffinityError::FingerprintBudget {
                actual: 5,
                maximum: 4,
            },
        ),
        (
            "duplicate adjacent",
            vec![3, 3],
            MorAffinityError::DuplicateFingerprintPosition(3),
        ),
        (
            "duplicate non-adjacent",
            vec![7, 1, 7],
            MorAffinityError::DuplicateFingerprintPosition(7),
        ),
        (
            "duplicate u32::MAX",
            vec![u32::MAX, u32::MAX],
            MorAffinityError::DuplicateFingerprintPosition(u32::MAX),
        ),
    ];
    for (name, positions, expected) in cases {
        assert_eq!(
            SmlFingerprintPlan::new(positions, BASE).unwrap_err(),
            expected,
            "{name}"
        );
    }
    // Positive boundary control: exactly at budget, unsorted positions kept in order.
    let at_budget = SmlFingerprintPlan::new(vec![9, 0, 5, 2], BASE).unwrap();
    assert_eq!(at_budget.positions(), &[9, 0, 5, 2]);
    assert_eq!(at_budget.width(), 4);
}

#[test]
fn fingerprint_workspace_rejection_matrix() {
    let cases: Vec<(&str, Vec<u32>, usize, MorAffinityError)> = vec![
        (
            "empty workspace",
            vec![0],
            0,
            MorAffinityError::WorkspaceTooSmall {
                required_index: 0,
                actual_bits: 0,
            },
        ),
        (
            "off by one",
            vec![0, 4],
            4,
            MorAffinityError::WorkspaceTooSmall {
                required_index: 4,
                actual_bits: 4,
            },
        ),
        (
            "first missing position reported",
            vec![1, 9, 6],
            4,
            MorAffinityError::WorkspaceTooSmall {
                required_index: 9,
                actual_bits: 4,
            },
        ),
    ];
    for (name, positions, bits, expected) in cases {
        let plan = SmlFingerprintPlan::new(positions, BASE).unwrap();
        let state = BooleanWorkspace::from_bits(&vec![true; bits]);
        assert_eq!(plan.fingerprint(&state).unwrap_err(), expected, "{name}");
    }
    // Positive boundary control: last valid index.
    let plan = SmlFingerprintPlan::new(vec![3], BASE).unwrap();
    let state = BooleanWorkspace::from_bits(&[false, false, false, true]);
    let fp = plan.fingerprint(&state).unwrap();
    assert_eq!((fp.len(), fp.get(0), fp.get(1)), (1, Some(true), None));
}

#[test]
fn bank_construction_rejection_matrix() {
    let wide = SmlFingerprintPlan::new(vec![0, 1, 2, 3], BASE).unwrap();
    let r = |f: fn(&mut NativeRouterSpec)| {
        let mut routers = ok_routers();
        f(&mut routers[1]);
        routers
    };
    let cases: Vec<(
        &str,
        SmlFingerprintPlan,
        Vec<NativeRouterSpec>,
        MorAffinityPolicy,
        MorAffinityError,
    )> = vec![
        (
            "plan wider than bank policy",
            wide,
            ok_routers(),
            MorAffinityPolicy {
                max_fingerprint_bits: 3,
                ..BASE
            },
            MorAffinityError::FingerprintBudget {
                actual: 4,
                maximum: 3,
            },
        ),
        (
            "empty router bank",
            plan(2),
            vec![],
            BASE,
            MorAffinityError::EmptyRouterBank,
        ),
        (
            "router budget",
            plan(2),
            vec![
                router(0, vec![atom(0, true, 1)]),
                router(1, vec![atom(0, true, 1)]),
                router(2, vec![atom(0, true, 1)]),
                router(3, vec![atom(0, true, 1)]),
            ],
            BASE,
            MorAffinityError::RouterBudget {
                actual: 4,
                maximum: 3,
            },
        ),
        (
            "duplicate router",
            plan(2),
            r(|x| x.router_id = RouterId(0)),
            BASE,
            MorAffinityError::DuplicateRouter(RouterId(0)),
        ),
        (
            "reliability ppm out of range",
            plan(2),
            r(|x| x.reliability_ppm = MOR_SCALE_PPM + 1),
            BASE,
            MorAffinityError::InputPpmOutOfRange(MOR_SCALE_PPM + 1),
        ),
        (
            "confidence ppm out of range",
            plan(2),
            r(|x| x.confidence_ppm = u32::MAX),
            BASE,
            MorAffinityError::InputPpmOutOfRange(u32::MAX),
        ),
        (
            "empty signature",
            plan(2),
            r(|x| x.atoms.clear()),
            BASE,
            MorAffinityError::EmptySignature(RouterId(1)),
        ),
        (
            "per-router atom budget",
            plan(4),
            r(|x| x.atoms.push(atom(2, true, 1))),
            BASE,
            MorAffinityError::AtomBudget {
                router_id: RouterId(1),
                actual: 3,
                maximum: 2,
            },
        ),
        (
            "empty proposals",
            plan(2),
            r(|x| x.proposals.clear()),
            BASE,
            MorAffinityError::EmptyProposals(RouterId(1)),
        ),
        (
            "signature bit == width",
            plan(2),
            r(|x| x.atoms[1].fingerprint_bit = 2),
            BASE,
            MorAffinityError::SignatureBitOutOfRange {
                router_id: RouterId(1),
                bit: 2,
                fingerprint_width: 2,
            },
        ),
        (
            "signature bit u16::MAX",
            plan(2),
            r(|x| x.atoms[0].fingerprint_bit = u16::MAX),
            BASE,
            MorAffinityError::SignatureBitOutOfRange {
                router_id: RouterId(1),
                bit: u16::MAX as usize,
                fingerprint_width: 2,
            },
        ),
        (
            "duplicate signature bit",
            plan(2),
            r(|x| x.atoms[1].fingerprint_bit = 0),
            BASE,
            MorAffinityError::DuplicateSignatureBit {
                router_id: RouterId(1),
                bit: 0,
            },
        ),
        (
            "zero atom weight",
            plan(2),
            r(|x| x.atoms[0].weight_ppm = 0),
            BASE,
            MorAffinityError::InvalidAtomWeight {
                router_id: RouterId(1),
                bit: 0,
                weight_ppm: 0,
            },
        ),
        (
            "atom weight above scale",
            plan(2),
            r(|x| x.atoms[1].weight_ppm = MOR_SCALE_PPM + 1),
            BASE,
            MorAffinityError::InvalidAtomWeight {
                router_id: RouterId(1),
                bit: 1,
                weight_ppm: MOR_SCALE_PPM + 1,
            },
        ),
        (
            "proposal ppm out of range",
            plan(2),
            r(|x| x.proposals[0].score_ppm = MOR_SCALE_PPM + 1),
            BASE,
            MorAffinityError::ProposalPpmOutOfRange {
                router_id: RouterId(1),
                page_id: PageId(1),
                score_ppm: MOR_SCALE_PPM + 1,
            },
        ),
        (
            "duplicate proposal",
            plan(2),
            r(|x| x.proposals.push(proposal(1, 1))),
            BASE,
            MorAffinityError::DuplicateProposal {
                router_id: RouterId(1),
                page_id: PageId(1),
            },
        ),
        (
            "total atom budget",
            plan(2),
            vec![
                router(0, vec![atom(0, true, 1), atom(1, true, 1)]),
                router(1, vec![atom(0, true, 1), atom(1, true, 1)]),
                router(2, vec![atom(0, true, 1)]),
            ],
            BASE,
            MorAffinityError::TotalAtomBudget {
                actual: 5,
                maximum: 4,
            },
        ),
    ];
    for (name, plan, routers, policy, expected) in cases {
        assert_eq!(
            NativeAffinityBank::new(plan, routers, policy).unwrap_err(),
            expected,
            "{name}"
        );
    }
}

#[test]
fn bank_construction_positive_boundary_control() {
    // Every budget exactly saturated, extreme-but-valid ppm values.
    let routers = vec![
        NativeRouterSpec {
            reliability_ppm: MOR_SCALE_PPM,
            confidence_ppm: 0,
            balance_bias_ppm: i32::MIN,
            proposals: vec![proposal(0, MOR_SCALE_PPM), proposal(1, 0)],
            ..router(0, vec![atom(0, true, MOR_SCALE_PPM), atom(3, false, 1)])
        },
        router(1, vec![atom(1, true, 1)]),
        router(2, vec![atom(2, false, 1)]),
    ];
    let bank = NativeAffinityBank::new(plan(4), routers.clone(), BASE).unwrap();
    assert_eq!(bank.routers(), routers.as_slice());
    assert_eq!(bank.policy(), BASE);
    let acc = bank.accounting();
    assert_eq!(acc.router_count, 3);
    assert_eq!(acc.signature_atoms, 4);
    assert_eq!(acc.weight_bits, 128);
    assert_eq!(acc.position_metadata_bits, 8);
    assert_eq!(acc.average_atoms_milli, 1_333);
}

#[test]
fn materialize_and_route_rejections_leave_bank_unchanged() {
    let bank = ok_bank();
    let snapshot = bank.clone();
    let engine = MixtureOfRouters::new(MorPolicy {
        min_routers: 1,
        max_routers: 1,
        router_mass_threshold_ppm: MOR_SCALE_PPM,
        ..MorPolicy::default()
    })
    .unwrap();
    let short = BooleanWorkspace::from_bits(&[true]);
    let good = BooleanWorkspace::from_bits(&[true, false]);
    let short_err = MorAffinityError::WorkspaceTooSmall {
        required_index: 1,
        actual_bits: 1,
    };

    assert_eq!(bank.materialize(&short).unwrap_err(), short_err);
    assert_eq!(bank, snapshot);
    assert_eq!(
        bank.route(&engine, &short, &[], &PreviousRoute::default())
            .unwrap_err(),
        short_err
    );
    assert_eq!(bank, snapshot);

    let hint = ExpertRuntimeHint {
        page_id: PageId(0),
        balance_bias_ppm: 0,
        residency_penalty_ppm: 0,
    };
    assert_eq!(
        bank.route(&engine, &good, &[hint, hint], &PreviousRoute::default())
            .unwrap_err(),
        MorAffinityError::Mor(MorError::DuplicateRuntimeHint(PageId(0)))
    );
    assert_eq!(bank, snapshot);

    let strict = MixtureOfRouters::new(MorPolicy {
        min_routers: 3,
        max_routers: 3,
        ..MorPolicy::default()
    })
    .unwrap();
    {
        assert!(matches!(
            bank.route(&strict, &good, &[], &PreviousRoute::default())
                .unwrap_err(),
            MorAffinityError::Mor(MorError::InsufficientRouters { .. })
        ));
        assert_eq!(bank, snapshot);
    }

    // Positive control after rejections: routing still works and is deterministic.
    let (d1, b1) = bank
        .route(&engine, &good, &[], &PreviousRoute::default())
        .unwrap();
    let (d2, b2) = bank
        .route(&engine, &good, &[], &PreviousRoute::default())
        .unwrap();
    assert_eq!((&d1, &b1), (&d2, &b2));
    assert_eq!(b1.evidence[0].affinity_ppm, MOR_SCALE_PPM);
    assert_eq!(b1.evidence[1].affinity_ppm, 0);
    assert_eq!(d1.active_routers[0].router_id, RouterId(0));
    assert_eq!(bank.materialize(&good).unwrap(), b1);
    assert_eq!(bank, snapshot);
}

#[test]
fn later_invalid_router_is_not_masked_by_earlier_valid_ones() {
    // The first failing router (in declaration order) is reported; earlier
    // valid routers do not mask later invalid ones.
    let mut routers = ok_routers();
    routers.push(router(2, vec![]));
    assert_eq!(
        NativeAffinityBank::new(plan(2), routers, BASE).unwrap_err(),
        MorAffinityError::EmptySignature(RouterId(2))
    );
}
