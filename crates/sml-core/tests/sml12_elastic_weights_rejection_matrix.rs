//! Table-driven rejection matrix for elastic-weight page planning.
//!
//! Covers every public error path of `elastic_weights.rs`: page construction,
//! policy validation, planner construction, importance/access updates,
//! `plan_cycle`, the self-contained `validate_contract_v1` check on forged
//! plans, and `commit`.

use sml_core::{
    elastic_weights::{
        ElasticWeightError, ElasticWeightPage, ElasticWeightPlan, ElasticWeightPlanField,
        ElasticWeightPlanner, ElasticWeightPolicy, ResidencyTier, WeightPrecision,
        ELASTIC_WEIGHT_PLAN_SCHEMA_V1, IMPORTANCE_SCALE_PPM,
    },
    PageId,
};

fn policy() -> ElasticWeightPolicy {
    ElasticWeightPolicy {
        ram_limit_bytes: 64,
        vram_limit_bytes: 64,
        max_active_parameters: 384,
        ternary_threshold_ppm: 500_000,
        residual4_threshold_ppm: 800_000,
        vram_threshold_ppm: 750_000,
    }
}

fn page(id: u32, parameters: u64, importance_ppm: u32) -> ElasticWeightPage {
    ElasticWeightPage::new(
        PageId(id),
        parameters,
        WeightPrecision::Boolean,
        ResidencyTier::Disk,
        importance_ppm,
    )
    .unwrap()
}

fn pages() -> Vec<ElasticWeightPage> {
    vec![
        page(0, 128, 900_000),
        page(1, 256, 600_000),
        page(2, 512, 100_000),
    ]
}

fn planner() -> ElasticWeightPlanner {
    ElasticWeightPlanner::new(pages(), policy()).unwrap()
}

fn valid_plan() -> ElasticWeightPlan {
    planner().plan_cycle(&[PageId(0), PageId(1)], 7).unwrap()
}

#[test]
fn positive_control_valid_inputs_are_accepted_end_to_end() {
    for importance in [0, 1, IMPORTANCE_SCALE_PPM] {
        for precision in [
            WeightPrecision::Boolean,
            WeightPrecision::Ternary,
            WeightPrecision::Residual4,
        ] {
            for residency in [ResidencyTier::Disk, ResidencyTier::Ram, ResidencyTier::Vram] {
                let page =
                    ElasticWeightPage::new(PageId(9), 1, precision, residency, importance).unwrap();
                assert!(!page.active);
                assert_eq!(page.representation_version, 0);
            }
        }
    }

    let boundary = ElasticWeightPolicy {
        ternary_threshold_ppm: IMPORTANCE_SCALE_PPM,
        residual4_threshold_ppm: IMPORTANCE_SCALE_PPM,
        vram_threshold_ppm: IMPORTANCE_SCALE_PPM,
        ..policy()
    };
    assert_eq!(boundary.validate(), Ok(boundary));
    assert_eq!(policy().validate(), Ok(policy()));

    let mut planner = planner();
    planner.record_importance(PageId(2), 0).unwrap();
    planner
        .record_access(PageId(2), IMPORTANCE_SCALE_PPM, 3)
        .unwrap();
    planner.record_importance(PageId(2), 100_000).unwrap();
    let plan = planner.plan_cycle(&[PageId(0), PageId(1)], 7).unwrap();
    assert_eq!(plan.contract, ELASTIC_WEIGHT_PLAN_SCHEMA_V1);
    assert_eq!(plan.validate_contract_v1(), Ok(()));
    planner.commit(&plan).unwrap();
    assert_eq!(planner.generation(), 1);
    assert_eq!(planner.accounting().unwrap().active_parameters, 384);

    let empty = planner.plan_cycle(&[], 8).unwrap();
    assert_eq!(empty.active_parameters, 0);
    planner.commit(&empty).unwrap();
    assert_eq!(planner.generation(), 2);
}

#[test]
fn page_constructor_rejects_zero_parameters_and_out_of_range_importance() {
    let cases = [
        (0, 500_000, ElasticWeightError::ZeroParameters(PageId(4))),
        // Zero parameters is checked before importance.
        (0, u32::MAX, ElasticWeightError::ZeroParameters(PageId(4))),
        (
            1,
            IMPORTANCE_SCALE_PPM + 1,
            ElasticWeightError::ImportanceOutOfRange {
                page_id: PageId(4),
                actual: IMPORTANCE_SCALE_PPM + 1,
            },
        ),
        (
            u64::MAX,
            u32::MAX,
            ElasticWeightError::ImportanceOutOfRange {
                page_id: PageId(4),
                actual: u32::MAX,
            },
        ),
    ];
    for (parameters, importance, expected) in cases {
        for precision in [
            WeightPrecision::Boolean,
            WeightPrecision::Ternary,
            WeightPrecision::Residual4,
        ] {
            assert_eq!(
                ElasticWeightPage::new(
                    PageId(4),
                    parameters,
                    precision,
                    ResidencyTier::Ram,
                    importance
                ),
                Err(expected),
                "parameters={parameters} importance={importance} precision={precision:?}"
            );
        }
    }
}

#[test]
fn policy_validate_rejects_every_threshold_violation() {
    let over = IMPORTANCE_SCALE_PPM + 1;
    let cases = [
        ElasticWeightPolicy {
            ternary_threshold_ppm: over,
            residual4_threshold_ppm: over,
            ..policy()
        },
        ElasticWeightPolicy {
            residual4_threshold_ppm: over,
            ..policy()
        },
        ElasticWeightPolicy {
            vram_threshold_ppm: over,
            ..policy()
        },
        ElasticWeightPolicy {
            vram_threshold_ppm: u32::MAX,
            ..policy()
        },
        // Ternary threshold must not exceed residual4 threshold.
        ElasticWeightPolicy {
            ternary_threshold_ppm: 800_001,
            residual4_threshold_ppm: 800_000,
            ..policy()
        },
    ];
    for bad in cases {
        assert_eq!(
            bad.validate(),
            Err(ElasticWeightError::InvalidPolicyThresholds),
            "{bad:?}"
        );
        assert_eq!(
            ElasticWeightPlanner::new(pages(), bad).unwrap_err(),
            ElasticWeightError::InvalidPolicyThresholds,
            "{bad:?}"
        );
    }
}

#[test]
fn planner_constructor_rejects_forged_or_inconsistent_pages() {
    let base = page(0, 128, 500_000);
    let zero = ElasticWeightPage {
        parameters: 0,
        ..base
    };
    let importance = ElasticWeightPage {
        importance_ppm: IMPORTANCE_SCALE_PPM + 1,
        ..base
    };
    let active_disk = ElasticWeightPage {
        active: true,
        ..base
    };
    let ram_heavy = ElasticWeightPage {
        parameters: 65 * 8,
        residency: ResidencyTier::Ram,
        ..base
    };
    let vram_heavy = ElasticWeightPage {
        parameters: 65 * 8,
        residency: ResidencyTier::Vram,
        ..base
    };
    let active_heavy = ElasticWeightPage {
        parameters: 385,
        residency: ResidencyTier::Vram,
        active: true,
        ..base
    };
    let huge = ElasticWeightPage {
        parameters: u64::MAX,
        ..base
    };
    let huge_other = ElasticWeightPage {
        page_id: PageId(1),
        ..huge
    };

    let cases = [
        (vec![zero], ElasticWeightError::ZeroParameters(PageId(0))),
        (
            vec![importance],
            ElasticWeightError::ImportanceOutOfRange {
                page_id: PageId(0),
                actual: IMPORTANCE_SCALE_PPM + 1,
            },
        ),
        (
            vec![active_disk],
            ElasticWeightError::ActivePageOnDisk(PageId(0)),
        ),
        (
            vec![base, page(1, 8, 0), base],
            ElasticWeightError::DuplicatePageId(PageId(0)),
        ),
        (
            vec![ram_heavy],
            ElasticWeightError::RamBudget {
                actual: 65,
                maximum: 64,
            },
        ),
        (
            vec![vram_heavy],
            ElasticWeightError::VramBudget {
                actual: 65,
                maximum: 64,
            },
        ),
        (
            vec![active_heavy],
            ElasticWeightError::ActiveParameterBudget {
                actual: 385,
                maximum: 384,
            },
        ),
        (
            vec![huge, huge_other],
            ElasticWeightError::ArithmeticOverflow,
        ),
    ];
    for (pages, expected) in cases {
        assert_eq!(
            ElasticWeightPlanner::new(pages.clone(), policy()).unwrap_err(),
            expected,
            "{pages:?}"
        );
    }

    // A single page with u64::MAX parameters fits every byte counter and is accepted.
    assert!(ElasticWeightPlanner::new(vec![huge], policy()).is_ok());
}

#[test]
fn importance_and_access_updates_reject_without_mutation() {
    let mut planner = planner();
    let before = planner.clone();
    let cases = [
        (
            PageId(99),
            500_000,
            ElasticWeightError::UnknownPage(PageId(99)),
        ),
        (
            PageId(0),
            IMPORTANCE_SCALE_PPM + 1,
            ElasticWeightError::ImportanceOutOfRange {
                page_id: PageId(0),
                actual: IMPORTANCE_SCALE_PPM + 1,
            },
        ),
        // Importance range is checked before page lookup.
        (
            PageId(99),
            u32::MAX,
            ElasticWeightError::ImportanceOutOfRange {
                page_id: PageId(99),
                actual: u32::MAX,
            },
        ),
    ];
    for (page_id, importance, expected) in cases {
        assert_eq!(
            planner.record_importance(page_id, importance),
            Err(expected)
        );
        assert_eq!(
            planner.record_access(page_id, importance, 42),
            Err(expected)
        );
        assert_eq!(planner, before);
    }
}

#[test]
fn plan_cycle_rejects_invalid_selections_and_budgets() {
    let planner = planner();
    let tight_active = ElasticWeightPlanner::new(
        pages(),
        ElasticWeightPolicy {
            max_active_parameters: 383,
            ..policy()
        },
    )
    .unwrap();
    let tight_memory = ElasticWeightPlanner::new(
        pages(),
        ElasticWeightPolicy {
            ram_limit_bytes: 16,
            vram_limit_bytes: 16,
            ..policy()
        },
    )
    .unwrap();

    let cases: [(&ElasticWeightPlanner, Vec<PageId>, ElasticWeightError); 5] = [
        (
            &planner,
            vec![PageId(1), PageId(1)],
            ElasticWeightError::DuplicateSelection(PageId(1)),
        ),
        (
            &planner,
            vec![PageId(0), PageId(77)],
            ElasticWeightError::UnknownPage(PageId(77)),
        ),
        // Duplicate detection runs before the unknown-page lookup on the same id.
        (
            &planner,
            vec![PageId(0), PageId(0), PageId(77)],
            ElasticWeightError::DuplicateSelection(PageId(0)),
        ),
        (
            &tight_active,
            vec![PageId(0), PageId(1)],
            ElasticWeightError::ActiveParameterBudget {
                actual: 384,
                maximum: 383,
            },
        ),
        // Page 1 is ternary (256 params -> 64 bytes) and fits neither tier.
        (
            &tight_memory,
            vec![PageId(1)],
            ElasticWeightError::ActiveResidencyBudget {
                page_id: PageId(1),
                payload_bytes: 64,
                ram_remaining: 16,
                vram_remaining: 16,
            },
        ),
    ];
    for (planner, selection, expected) in cases {
        let before = planner.clone();
        assert_eq!(
            planner.plan_cycle(&selection, 1).unwrap_err(),
            expected,
            "{selection:?}"
        );
        assert_eq!(*planner, before);
    }
}

#[test]
fn plan_cycle_rejects_representation_version_overflow() {
    let saturated = ElasticWeightPage {
        representation_version: u64::MAX,
        ..page(0, 128, 900_000)
    };
    let planner = ElasticWeightPlanner::new(vec![saturated], policy()).unwrap();
    // Importance 900_000 promotes Boolean -> Residual4, which needs a version bump.
    assert_eq!(
        planner.plan_cycle(&[], 1).unwrap_err(),
        ElasticWeightError::RepresentationVersionOverflow(PageId(0))
    );

    // No precision change -> no version bump -> accepted.
    let stable = ElasticWeightPage {
        representation_version: u64::MAX,
        ..page(0, 128, 0)
    };
    let planner = ElasticWeightPlanner::new(vec![stable], policy()).unwrap();
    assert!(planner.plan_cycle(&[], 1).is_ok());
}

#[test]
fn validate_contract_v1_rejects_every_forged_transition_field() {
    let plan = valid_plan();
    assert_eq!(plan.validate_contract_v1(), Ok(()));
    let first = plan.transitions[0];

    let mut cases: Vec<(&str, ElasticWeightPlan, ElasticWeightError)> = Vec::new();

    let mut forged = plan.clone();
    forged.contract = "sml.elastic-weight-plan@0.9.0";
    cases.push(("contract", forged, ElasticWeightError::InvalidPlanContract));

    let mut forged = plan.clone();
    forged.transitions[0].parameters = 0;
    cases.push((
        "zero_parameters",
        forged,
        ElasticWeightError::ZeroParameters(first.page_id),
    ));

    let mut forged = plan.clone();
    forged.transitions[1].page_id = first.page_id;
    cases.push((
        "duplicate_page",
        forged,
        ElasticWeightError::DuplicatePlanPageId(first.page_id),
    ));

    let mut forged = plan.clone();
    forged.transitions[0].target_payload_bytes += 1;
    cases.push((
        "payload_bytes",
        forged,
        ElasticWeightError::PlanPayloadBytes {
            page_id: first.page_id,
            expected: first.target_payload_bytes,
            actual: first.target_payload_bytes + 1,
        },
    ));

    let mut forged = plan.clone();
    forged.transitions[0].to_representation_version += 1;
    cases.push((
        "representation_version",
        forged,
        ElasticWeightError::PlanRepresentationVersion {
            page_id: first.page_id,
            expected: first.to_representation_version,
            actual: first.to_representation_version + 1,
        },
    ));

    let mut forged = plan.clone();
    assert_ne!(first.from_precision, first.to_precision);
    forged.transitions[0].from_representation_version = u64::MAX;
    cases.push((
        "representation_version_overflow",
        forged,
        ElasticWeightError::RepresentationVersionOverflow(first.page_id),
    ));

    let mut forged = plan.clone();
    forged.transitions[0].to_residency = ResidencyTier::Disk;
    cases.push((
        "active_on_disk",
        forged,
        ElasticWeightError::ActivePageOnDisk(first.page_id),
    ));

    let mut forged = plan.clone();
    forged.ram_limit_bytes = plan.ram_bytes - 1;
    cases.push((
        "ram_budget",
        forged,
        ElasticWeightError::RamBudget {
            actual: plan.ram_bytes,
            maximum: plan.ram_bytes - 1,
        },
    ));

    let mut forged = plan.clone();
    forged.vram_limit_bytes = plan.vram_bytes - 1;
    cases.push((
        "vram_budget",
        forged,
        ElasticWeightError::VramBudget {
            actual: plan.vram_bytes,
            maximum: plan.vram_bytes - 1,
        },
    ));

    let mut forged = plan.clone();
    forged.max_active_parameters = plan.active_parameters - 1;
    cases.push((
        "active_parameter_budget",
        forged,
        ElasticWeightError::ActiveParameterBudget {
            actual: plan.active_parameters,
            maximum: plan.active_parameters - 1,
        },
    ));

    for (label, forged, expected) in cases {
        assert_eq!(forged.validate_contract_v1(), Err(expected), "{label}");
    }
}

#[test]
fn validate_contract_v1_rejects_every_forged_aggregate_field() {
    let plan = valid_plan();
    type Forge = fn(&mut ElasticWeightPlan);
    let cases: [(ElasticWeightPlanField, Forge, u128, u128); 7] = [
        (
            ElasticWeightPlanField::RamBytes,
            |p| p.ram_bytes += 1,
            plan.ram_bytes as u128,
            plan.ram_bytes as u128 + 1,
        ),
        (
            ElasticWeightPlanField::VramBytes,
            |p| p.vram_bytes += 1,
            plan.vram_bytes as u128,
            plan.vram_bytes as u128 + 1,
        ),
        (
            ElasticWeightPlanField::ActiveParameters,
            |p| p.active_parameters += 1,
            plan.active_parameters as u128,
            plan.active_parameters as u128 + 1,
        ),
        (
            ElasticWeightPlanField::ActiveStorageBits,
            |p| p.active_storage_bits += 1,
            plan.active_storage_bits,
            plan.active_storage_bits + 1,
        ),
        (
            ElasticWeightPlanField::BytesMovedToRam,
            |p| p.bytes_moved_to_ram += 1,
            plan.bytes_moved_to_ram as u128,
            plan.bytes_moved_to_ram as u128 + 1,
        ),
        (
            ElasticWeightPlanField::BytesMovedToVram,
            |p| p.bytes_moved_to_vram += 1,
            plan.bytes_moved_to_vram as u128,
            plan.bytes_moved_to_vram as u128 + 1,
        ),
        (
            ElasticWeightPlanField::PrecisionChanges,
            |p| p.precision_changes += 1,
            plan.precision_changes as u128,
            plan.precision_changes as u128 + 1,
        ),
    ];
    for (field, forge, expected, actual) in cases {
        let mut forged = plan.clone();
        forge(&mut forged);
        assert_eq!(
            forged.validate_contract_v1(),
            Err(ElasticWeightError::PlanAccounting {
                field,
                expected,
                actual,
            }),
            "{field:?}"
        );
    }
}

#[test]
fn commit_rejects_stale_mismatched_or_forged_plans_without_mutation() {
    let plan = valid_plan();
    let mut cases: Vec<(&str, ElasticWeightPlan, ElasticWeightError)> = Vec::new();

    let mut forged = plan.clone();
    forged.contract = "not-a-contract";
    cases.push(("contract", forged, ElasticWeightError::InvalidPlanContract));

    let mut forged = plan.clone();
    forged.base_generation = 1;
    cases.push((
        "stale",
        forged,
        ElasticWeightError::StalePlan {
            expected_generation: 0,
            actual_generation: 1,
        },
    ));

    let mut forged = plan.clone();
    forged.transitions.pop();
    forged.precision_changes = forged
        .transitions
        .iter()
        .filter(|t| t.from_precision != t.to_precision)
        .count() as u64;
    assert_eq!(forged.validate_contract_v1(), Ok(()));
    cases.push((
        "page_count",
        forged,
        ElasticWeightError::PlanPageCount {
            expected: 3,
            actual: 2,
        },
    ));

    let mut forged = plan.clone();
    forged.transitions[2].page_id = PageId(42);
    cases.push((
        "unknown_page",
        forged,
        ElasticWeightError::UnknownPage(PageId(42)),
    ));

    type Forge = fn(&mut ElasticWeightPlan);
    let mismatches: [(&str, Forge); 4] = [
        ("from_precision", |p| {
            p.transitions[2].from_precision = WeightPrecision::Ternary;
            p.transitions[2].to_precision = WeightPrecision::Ternary;
            p.transitions[2].target_payload_bytes = 128;
            p.precision_changes = 2;
        }),
        ("from_residency", |p| {
            p.transitions[2].from_residency = ResidencyTier::Ram;
        }),
        ("from_active", |p| p.transitions[2].from_active = true),
        ("from_representation_version", |p| {
            p.transitions[2].from_representation_version = 5;
            p.transitions[2].to_representation_version = 5;
        }),
    ];
    for (label, forge) in mismatches {
        let mut forged = plan.clone();
        forge(&mut forged);
        assert_eq!(forged.validate_contract_v1(), Ok(()), "{label}");
        cases.push((
            label,
            forged,
            ElasticWeightError::PlanStateMismatch(PageId(2)),
        ));
    }

    for (label, forged, expected) in cases {
        let mut planner = planner();
        let before = planner.clone();
        assert_eq!(planner.commit(&forged), Err(expected), "{label}");
        assert_eq!(planner, before, "{label}: planner mutated on rejection");
    }

    // Replaying an already committed plan is stale.
    let mut planner = planner();
    planner.commit(&plan).unwrap();
    let after = planner.clone();
    assert_eq!(
        planner.commit(&plan),
        Err(ElasticWeightError::StalePlan {
            expected_generation: 1,
            actual_generation: 0,
        })
    );
    assert_eq!(planner, after);
}

/// Forged plans whose own limits exceed the planner policy pass
/// `validate_contract_v1` (which checks the plan against its own limits) and
/// are only caught by the post-apply budget check inside `commit`.
fn over_policy_plans() -> [(&'static str, ElasticWeightPlan, ElasticWeightError); 3] {
    let plan = valid_plan();

    // Page 2 (Boolean, 64 bytes) forged into RAM: RAM total 128 > policy 64.
    let mut ram = plan.clone();
    ram.ram_limit_bytes = u64::MAX;
    ram.transitions[2].to_residency = ResidencyTier::Ram;
    ram.ram_bytes += ram.transitions[2].target_payload_bytes;
    ram.bytes_moved_to_ram += ram.transitions[2].target_payload_bytes;

    // Page 2 forged into VRAM: VRAM total 128 > policy 64.
    let mut vram = plan.clone();
    vram.vram_limit_bytes = u64::MAX;
    vram.transitions[2].to_residency = ResidencyTier::Vram;
    vram.vram_bytes += vram.transitions[2].target_payload_bytes;
    vram.bytes_moved_to_vram += vram.transitions[2].target_payload_bytes;

    // Page 2 forged active in VRAM: 896 active parameters > policy 384, but the
    // VRAM budget is checked first, so VramBudget is reported.
    let mut active = vram.clone();
    active.max_active_parameters = u64::MAX;
    active.transitions[2].to_active = true;
    active.active_parameters += active.transitions[2].parameters;
    active.active_storage_bits += active.transitions[2].parameters as u128;

    [
        (
            "ram",
            ram,
            ElasticWeightError::RamBudget {
                actual: 128,
                maximum: 64,
            },
        ),
        (
            "vram",
            vram,
            ElasticWeightError::VramBudget {
                actual: 128,
                maximum: 64,
            },
        ),
        (
            "active",
            active,
            ElasticWeightError::VramBudget {
                actual: 128,
                maximum: 64,
            },
        ),
    ]
}

#[test]
fn commit_rejects_plans_whose_limits_exceed_planner_policy() {
    for (label, forged, expected) in over_policy_plans() {
        assert_eq!(forged.validate_contract_v1(), Ok(()), "{label}");
        let mut planner = planner();
        assert_eq!(planner.commit(&forged), Err(expected), "{label}");
    }

    // With roomy byte limits, only the active-parameter budget is violated.
    let mut planner = ElasticWeightPlanner::new(
        pages(),
        ElasticWeightPolicy {
            ram_limit_bytes: 1_024,
            vram_limit_bytes: 1_024,
            ..policy()
        },
    )
    .unwrap();
    let mut forged = planner.plan_cycle(&[PageId(0), PageId(1)], 7).unwrap();
    forged.max_active_parameters = u64::MAX;
    forged.transitions[2].to_residency = ResidencyTier::Ram;
    forged.transitions[2].to_active = true;
    forged.ram_bytes += forged.transitions[2].target_payload_bytes;
    forged.bytes_moved_to_ram += forged.transitions[2].target_payload_bytes;
    forged.active_parameters += forged.transitions[2].parameters;
    forged.active_storage_bits += forged.transitions[2].parameters as u128;
    assert_eq!(forged.validate_contract_v1(), Ok(()));
    assert_eq!(
        planner.commit(&forged),
        Err(ElasticWeightError::ActiveParameterBudget {
            actual: 896,
            maximum: 384,
        })
    );
}

/// NEGATIVE FINDING (documented, not fixed in this tests-only PR):
/// `ElasticWeightPlanner::commit` is documented as atomic, but for plans whose
/// own limits exceed the planner policy it applies every transition and bumps
/// the generation *before* `validate_current_budgets` fails, so the planner is
/// left mutated (over budget, generation 1) even though `Err` is returned.
#[test]
#[ignore = "negative finding: commit mutates planner state before the post-apply budget check rejects an over-policy plan; fix belongs in a separate production PR"]
fn commit_is_atomic_for_plans_whose_limits_exceed_planner_policy() {
    for (label, forged, _) in over_policy_plans() {
        let mut planner = planner();
        let before = planner.clone();
        assert!(planner.commit(&forged).is_err(), "{label}");
        assert_eq!(planner, before, "{label}: planner mutated on rejection");
    }
}
