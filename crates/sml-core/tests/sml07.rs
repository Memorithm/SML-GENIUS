use sml_core::{
    elastic_weights::{
        ElasticWeightError, ElasticWeightPage, ElasticWeightPlanner, ElasticWeightPolicy,
        ResidencyTier, WeightPrecision,
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

fn pages() -> Vec<ElasticWeightPage> {
    vec![
        ElasticWeightPage::new(
            PageId(0),
            128,
            WeightPrecision::Boolean,
            ResidencyTier::Disk,
            900_000,
        )
        .unwrap(),
        ElasticWeightPage::new(
            PageId(1),
            256,
            WeightPrecision::Boolean,
            ResidencyTier::Disk,
            600_000,
        )
        .unwrap(),
        ElasticWeightPage::new(
            PageId(2),
            512,
            WeightPrecision::Boolean,
            ResidencyTier::Disk,
            100_000,
        )
        .unwrap(),
    ]
}

#[test]
fn precision_classes_have_exact_parameter_and_storage_accounting() {
    let boolean = ElasticWeightPage::new(
        PageId(0),
        65,
        WeightPrecision::Boolean,
        ResidencyTier::Disk,
        0,
    )
    .unwrap();
    let ternary = ElasticWeightPage::new(
        PageId(1),
        65,
        WeightPrecision::Ternary,
        ResidencyTier::Disk,
        0,
    )
    .unwrap();
    let residual = ElasticWeightPage::new(
        PageId(2),
        65,
        WeightPrecision::Residual4,
        ResidencyTier::Disk,
        0,
    )
    .unwrap();

    assert_eq!(boolean.payload_bits(), 65);
    assert_eq!(boolean.payload_bytes_ceil().unwrap(), 9);
    assert_eq!(boolean.parameter_budget().boolean, 65);

    assert_eq!(ternary.payload_bits(), 130);
    assert_eq!(ternary.payload_bytes_ceil().unwrap(), 17);
    assert_eq!(ternary.parameter_budget().ternary, 65);

    assert_eq!(residual.payload_bits(), 260);
    assert_eq!(residual.payload_bytes_ceil().unwrap(), 33);
    assert_eq!(residual.parameter_budget().residual4, 65);
}

#[test]
fn cycle_plan_is_non_mutating_and_assigns_precision_residency_and_activation() {
    let planner = ElasticWeightPlanner::new(pages(), policy()).unwrap();
    let before = planner.clone();
    let plan = planner.plan_cycle(&[PageId(0), PageId(1)], 7).unwrap();

    assert_eq!(planner, before);
    assert_eq!(plan.active_parameters, 384);
    assert_eq!(plan.active_storage_bits, 1_024);
    assert_eq!(plan.ram_bytes, 64);
    assert_eq!(plan.vram_bytes, 64);
    assert_eq!(plan.bytes_moved_to_ram, 64);
    assert_eq!(plan.bytes_moved_to_vram, 64);
    assert_eq!(plan.precision_changes, 2);

    let hot = plan
        .transitions
        .iter()
        .find(|transition| transition.page_id == PageId(0))
        .unwrap();
    assert_eq!(hot.to_precision, WeightPrecision::Residual4);
    assert_eq!(hot.to_residency, ResidencyTier::Vram);
    assert!(hot.to_active);

    let warm = plan
        .transitions
        .iter()
        .find(|transition| transition.page_id == PageId(1))
        .unwrap();
    assert_eq!(warm.to_precision, WeightPrecision::Ternary);
    assert_eq!(warm.to_residency, ResidencyTier::Ram);
    assert!(warm.to_active);

    let cold = plan
        .transitions
        .iter()
        .find(|transition| transition.page_id == PageId(2))
        .unwrap();
    assert_eq!(cold.to_precision, WeightPrecision::Boolean);
    assert_eq!(cold.to_residency, ResidencyTier::Disk);
    assert!(!cold.to_active);
}

#[test]
fn commit_freezes_plan_and_preserves_exact_accounting() {
    let mut planner = ElasticWeightPlanner::new(pages(), policy()).unwrap();
    let plan = planner.plan_cycle(&[PageId(0), PageId(1)], 11).unwrap();
    planner.commit(&plan).unwrap();

    assert_eq!(planner.generation(), 1);
    let accounting = planner.accounting().unwrap();
    assert_eq!(accounting.total_parameters, 896);
    assert_eq!(accounting.boolean_parameters, 512);
    assert_eq!(accounting.ternary_parameters, 256);
    assert_eq!(accounting.residual4_parameters, 128);
    assert_eq!(accounting.disk_bytes, 64);
    assert_eq!(accounting.ram_bytes, 64);
    assert_eq!(accounting.vram_bytes, 64);
    assert_eq!(accounting.active_parameters, 384);
    assert_eq!(accounting.active_storage_bits, 1_024);

    assert_eq!(planner.pages()[0].representation_version, 1);
    assert_eq!(planner.pages()[1].representation_version, 1);
    assert_eq!(planner.pages()[2].representation_version, 0);
    assert_eq!(planner.pages()[0].last_access_epoch, 11);
    assert_eq!(planner.pages()[1].last_access_epoch, 11);
}

#[test]
fn stale_plan_cannot_be_committed_twice() {
    let mut planner = ElasticWeightPlanner::new(pages(), policy()).unwrap();
    let plan = planner.plan_cycle(&[PageId(0)], 1).unwrap();
    planner.commit(&plan).unwrap();

    assert_eq!(
        planner.commit(&plan).unwrap_err(),
        ElasticWeightError::StalePlan {
            expected_generation: 1,
            actual_generation: 0,
        }
    );
}

#[test]
fn active_parameter_budget_fails_closed() {
    let mut constrained = policy();
    constrained.max_active_parameters = 127;
    let planner = ElasticWeightPlanner::new(pages(), constrained).unwrap();

    assert_eq!(
        planner.plan_cycle(&[PageId(0)], 0).unwrap_err(),
        ElasticWeightError::ActiveParameterBudget {
            actual: 128,
            maximum: 127,
        }
    );
}

#[test]
fn residency_budget_fails_closed_when_selected_page_cannot_fit() {
    let mut constrained = policy();
    constrained.ram_limit_bytes = 0;
    constrained.vram_limit_bytes = 0;
    let planner = ElasticWeightPlanner::new(pages(), constrained).unwrap();

    assert_eq!(
        planner.plan_cycle(&[PageId(0)], 0).unwrap_err(),
        ElasticWeightError::ActiveResidencyBudget {
            page_id: PageId(0),
            payload_bytes: 64,
            ram_remaining: 0,
            vram_remaining: 0,
        }
    );
}

#[test]
fn access_observation_updates_importance_without_changing_representation() {
    let mut planner = ElasticWeightPlanner::new(pages(), policy()).unwrap();
    planner.record_access(PageId(2), 700_000, 23).unwrap();

    let observed = planner
        .pages()
        .iter()
        .find(|page| page.page_id == PageId(2))
        .unwrap();
    assert_eq!(observed.importance_ppm, 700_000);
    assert_eq!(observed.access_count, 1);
    assert_eq!(observed.last_access_epoch, 23);
    assert_eq!(observed.precision, WeightPrecision::Boolean);
    assert_eq!(observed.residency, ResidencyTier::Disk);
    assert!(!observed.active);
}

#[test]
fn duplicate_page_ids_and_duplicate_selections_are_rejected() {
    let duplicate = pages()[0];
    assert_eq!(
        ElasticWeightPlanner::new(vec![duplicate, duplicate], policy()).unwrap_err(),
        ElasticWeightError::DuplicatePageId(PageId(0))
    );

    let planner = ElasticWeightPlanner::new(pages(), policy()).unwrap();
    assert_eq!(
        planner.plan_cycle(&[PageId(1), PageId(1)], 0).unwrap_err(),
        ElasticWeightError::DuplicateSelection(PageId(1))
    );
}
