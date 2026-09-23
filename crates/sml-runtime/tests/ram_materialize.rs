use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use sml_core::{
    elastic_weights::{
        ElasticWeightError, ElasticWeightPage, ElasticWeightPlanner, ElasticWeightPolicy,
        ResidencyTier, WeightPrecision,
    },
    PageId,
};
use sml_runtime::{
    build_weight_artifact, ElasticRamRuntime, RamMaterializationBudget, RamMaterializationError,
    WeightArtifactCatalog, WeightArtifactEntry,
};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "sml-runtime-ram-{}-{id}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self { path }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn cold_planner(
    importance0: u32,
    importance1: u32,
    ram_limit_bytes: u64,
    vram_limit_bytes: u64,
    max_active_parameters: u64,
    vram_threshold_ppm: u32,
) -> ElasticWeightPlanner {
    ElasticWeightPlanner::new(
        vec![
            ElasticWeightPage::new(
                PageId(0),
                8,
                WeightPrecision::Boolean,
                ResidencyTier::Disk,
                importance0,
            )
            .unwrap(),
            ElasticWeightPage::new(
                PageId(1),
                8,
                WeightPrecision::Boolean,
                ResidencyTier::Disk,
                importance1,
            )
            .unwrap(),
        ],
        ElasticWeightPolicy {
            ram_limit_bytes,
            vram_limit_bytes,
            max_active_parameters,
            ternary_threshold_ppm: 600_000,
            residual4_threshold_ppm: 900_000,
            vram_threshold_ppm,
        },
    )
    .unwrap()
}

fn write_artifact(path: &Path, page_id: PageId, payload: u8) {
    let bytes =
        build_weight_artifact(page_id, 0, WeightPrecision::Boolean, 8, &[payload]).unwrap();
    fs::write(path, bytes).unwrap();
}

fn catalog(temp: &TempDir) -> WeightArtifactCatalog {
    let page0 = temp.path("page0.smlw");
    let page1 = temp.path("page1.smlw");
    write_artifact(&page0, PageId(0), 0xa5);
    write_artifact(&page1, PageId(1), 0x5a);
    WeightArtifactCatalog::new(vec![
        WeightArtifactEntry {
            page_id: PageId(0),
            representation_version: 0,
            precision: WeightPrecision::Boolean,
            path: page0,
        },
        WeightArtifactEntry {
            page_id: PageId(1),
            representation_version: 0,
            precision: WeightPrecision::Boolean,
            path: page1,
        },
    ])
    .unwrap()
}

fn ram_runtime() -> ElasticRamRuntime {
    ElasticRamRuntime::new(cold_planner(500_000, 500_000, 2, 0, 16, 900_000)).unwrap()
}

#[test]
fn disk_artifact_is_read_verified_staged_then_committed_to_ram() {
    let temp = TempDir::new();
    let catalog = catalog(&temp);
    let mut runtime = ram_runtime();
    let plan = runtime.planner().plan_cycle(&[PageId(0)], 1).unwrap();

    let stage = runtime
        .stage(&plan, &catalog, RamMaterializationBudget::default())
        .unwrap();
    assert_eq!(stage.report().files_read, 1);
    assert_eq!(stage.report().artifacts_reused_from_ram, 0);
    assert_eq!(stage.report().artifact_bytes_read, 65);
    assert_eq!(stage.report().ram_pages, 1);
    assert_eq!(stage.report().ram_payload_bytes, 1);
    assert_eq!(stage.page(PageId(0)).unwrap().payload(), &[0xa5]);

    // Staging is non-mutating.
    assert_eq!(runtime.planner().generation(), 0);
    assert_eq!(runtime.ram_page_count(), 0);
    assert_eq!(runtime.planner().accounting().unwrap().active_parameters, 0);
    runtime.verify_stage(&stage).unwrap();

    let committed = runtime.commit(stage).unwrap();
    assert_eq!(committed.generation, 1);
    assert_eq!(committed.ram_pages, 1);
    assert_eq!(committed.ram_payload_bytes, 1);
    assert_eq!(runtime.ram_page(PageId(0)).unwrap().payload(), &[0xa5]);
    assert_eq!(runtime.planner().accounting().unwrap().active_parameters, 8);
    assert_eq!(runtime.planner().accounting().unwrap().ram_bytes, 1);
}

#[test]
fn switching_active_page_loads_new_artifact_and_drops_old_ram_payload() {
    let temp = TempDir::new();
    let catalog = catalog(&temp);
    let mut runtime = ram_runtime();

    let first = runtime.planner().plan_cycle(&[PageId(0)], 1).unwrap();
    runtime
        .commit(
            runtime
                .stage(&first, &catalog, RamMaterializationBudget::default())
                .unwrap(),
        )
        .unwrap();

    let second = runtime.planner().plan_cycle(&[PageId(1)], 2).unwrap();
    let stage = runtime
        .stage(&second, &catalog, RamMaterializationBudget::default())
        .unwrap();
    assert_eq!(stage.report().files_read, 1);
    assert_eq!(stage.report().artifacts_reused_from_ram, 0);
    assert!(stage.page(PageId(0)).is_none());
    assert_eq!(stage.page(PageId(1)).unwrap().payload(), &[0x5a]);

    runtime.commit(stage).unwrap();
    assert!(runtime.ram_page(PageId(0)).is_none());
    assert_eq!(runtime.ram_page(PageId(1)).unwrap().payload(), &[0x5a]);
    assert_eq!(runtime.planner().generation(), 2);
}

#[test]
fn unchanged_active_ram_page_is_reused_without_disk_read() {
    let temp = TempDir::new();
    let catalog = catalog(&temp);
    let mut runtime = ram_runtime();

    let first = runtime.planner().plan_cycle(&[PageId(0)], 1).unwrap();
    let stage = runtime
        .stage(&first, &catalog, RamMaterializationBudget::default())
        .unwrap();
    runtime.commit(stage).unwrap();

    let second = runtime.planner().plan_cycle(&[PageId(0)], 2).unwrap();
    let empty_catalog = WeightArtifactCatalog::new(vec![]).unwrap();
    let stage = runtime
        .stage(&second, &empty_catalog, RamMaterializationBudget::default())
        .unwrap();
    assert_eq!(stage.report().files_read, 0);
    assert_eq!(stage.report().artifacts_reused_from_ram, 1);
    assert_eq!(stage.report().artifact_bytes_read, 0);
    assert_eq!(stage.page(PageId(0)).unwrap().payload(), &[0xa5]);

    runtime.commit(stage).unwrap();
    assert_eq!(runtime.planner().generation(), 2);
}

#[test]
fn corrupted_payload_fails_before_planner_or_ram_mutation() {
    let temp = TempDir::new();
    let path = temp.path("bad.smlw");
    let mut bytes =
        build_weight_artifact(PageId(0), 0, WeightPrecision::Boolean, 8, &[0xa5]).unwrap();
    *bytes.last_mut().unwrap() ^= 1;
    fs::write(&path, bytes).unwrap();
    let catalog = WeightArtifactCatalog::new(vec![WeightArtifactEntry {
        page_id: PageId(0),
        representation_version: 0,
        precision: WeightPrecision::Boolean,
        path,
    }])
    .unwrap();
    let runtime = ram_runtime();
    let plan = runtime.planner().plan_cycle(&[PageId(0)], 1).unwrap();

    assert!(matches!(
        runtime.stage(&plan, &catalog, RamMaterializationBudget::default()),
        Err(RamMaterializationError::Artifact(
            sml_runtime::WeightArtifactError::PayloadChecksumMismatch { .. }
        ))
    ));
    assert_eq!(runtime.planner().generation(), 0);
    assert_eq!(runtime.ram_page_count(), 0);
}

#[test]
fn artifact_header_must_match_the_exact_elastic_transition() {
    let temp = TempDir::new();
    let path = temp.path("wrong-page.smlw");
    write_artifact(&path, PageId(1), 0xa5);
    let catalog = WeightArtifactCatalog::new(vec![WeightArtifactEntry {
        page_id: PageId(0),
        representation_version: 0,
        precision: WeightPrecision::Boolean,
        path,
    }])
    .unwrap();
    let runtime = ram_runtime();
    let plan = runtime.planner().plan_cycle(&[PageId(0)], 1).unwrap();

    assert_eq!(
        runtime.stage(&plan, &catalog, RamMaterializationBudget::default()),
        Err(RamMaterializationError::ArtifactTransitionMismatch(PageId(0)))
    );
}

#[test]
fn missing_artifact_and_materialization_budget_fail_closed() {
    let temp = TempDir::new();
    let catalog = catalog(&temp);
    let runtime = ram_runtime();
    let plan = runtime.planner().plan_cycle(&[PageId(0)], 1).unwrap();

    assert_eq!(
        runtime.stage(
            &plan,
            &WeightArtifactCatalog::new(vec![]).unwrap(),
            RamMaterializationBudget::default(),
        ),
        Err(RamMaterializationError::MissingArtifact {
            page_id: PageId(0),
            representation_version: 0,
            precision: WeightPrecision::Boolean,
        })
    );

    assert_eq!(
        runtime.stage(
            &plan,
            &catalog,
            RamMaterializationBudget {
                max_files_read: 0,
                ..RamMaterializationBudget::default()
            },
        ),
        Err(RamMaterializationError::MaterializationBudget)
    );
}

#[test]
fn precision_promotion_is_rejected_instead_of_synthesizing_richer_weights() {
    let temp = TempDir::new();
    let catalog = catalog(&temp);
    let runtime =
        ElasticRamRuntime::new(cold_planner(950_000, 500_000, 8, 0, 16, 900_000)).unwrap();
    let plan = runtime.planner().plan_cycle(&[PageId(0)], 1).unwrap();

    assert_eq!(
        runtime.stage(&plan, &catalog, RamMaterializationBudget::default()),
        Err(RamMaterializationError::UnsupportedPrecisionTransition {
            page_id: PageId(0),
            from: WeightPrecision::Boolean,
            to: WeightPrecision::Residual4,
        })
    );
    assert_eq!(runtime.planner().generation(), 0);
}

#[test]
fn vram_target_is_rejected_instead_of_being_emulated_with_host_memory() {
    let temp = TempDir::new();
    let catalog = catalog(&temp);
    let runtime =
        ElasticRamRuntime::new(cold_planner(500_000, 500_000, 0, 1, 16, 0)).unwrap();
    let plan = runtime.planner().plan_cycle(&[PageId(0)], 1).unwrap();

    assert_eq!(
        runtime.stage(&plan, &catalog, RamMaterializationBudget::default()),
        Err(RamMaterializationError::UnsupportedTargetResidency {
            page_id: PageId(0),
            residency: ResidencyTier::Vram,
        })
    );
    assert_eq!(runtime.ram_page_count(), 0);
}

#[test]
fn rollback_discards_stage_without_mutating_planner_or_ram() {
    let temp = TempDir::new();
    let catalog = catalog(&temp);
    let runtime = ram_runtime();
    let plan = runtime.planner().plan_cycle(&[PageId(0)], 1).unwrap();
    let stage = runtime
        .stage(&plan, &catalog, RamMaterializationBudget::default())
        .unwrap();

    let rollback = runtime.rollback(stage);
    assert_eq!(rollback.planner_generation, 0);
    assert_eq!(rollback.discarded_pages, 1);
    assert_eq!(rollback.discarded_payload_bytes, 1);
    assert_eq!(runtime.planner().generation(), 0);
    assert_eq!(runtime.ram_page_count(), 0);
    assert_eq!(runtime.planner().accounting().unwrap().active_parameters, 0);
}

#[test]
fn stale_staged_plan_cannot_replace_committed_ram_generation() {
    let temp = TempDir::new();
    let catalog = catalog(&temp);
    let mut runtime = ram_runtime();

    let first_plan = runtime.planner().plan_cycle(&[PageId(0)], 1).unwrap();
    let stale_plan = runtime.planner().plan_cycle(&[PageId(1)], 2).unwrap();
    let first_stage = runtime
        .stage(&first_plan, &catalog, RamMaterializationBudget::default())
        .unwrap();
    let stale_stage = runtime
        .stage(&stale_plan, &catalog, RamMaterializationBudget::default())
        .unwrap();

    runtime.commit(first_stage).unwrap();
    assert_eq!(
        runtime.commit(stale_stage),
        Err(RamMaterializationError::Elastic(
            ElasticWeightError::StalePlan {
                expected_generation: 1,
                actual_generation: 0,
            }
        ))
    );
    assert_eq!(runtime.planner().generation(), 1);
    assert_eq!(runtime.ram_page(PageId(0)).unwrap().payload(), &[0xa5]);
    assert!(runtime.ram_page(PageId(1)).is_none());
}

#[test]
fn duplicate_catalog_keys_and_non_cold_initial_state_are_rejected() {
    let path = PathBuf::from("same");
    assert!(matches!(
        WeightArtifactCatalog::new(vec![
            WeightArtifactEntry {
                page_id: PageId(0),
                representation_version: 0,
                precision: WeightPrecision::Boolean,
                path: path.clone(),
            },
            WeightArtifactEntry {
                page_id: PageId(0),
                representation_version: 0,
                precision: WeightPrecision::Boolean,
                path,
            },
        ]),
        Err(RamMaterializationError::DuplicateArtifact {
            page_id: PageId(0),
            ..
        })
    ));

    let mut planner = cold_planner(500_000, 500_000, 2, 0, 16, 900_000);
    let plan = planner.plan_cycle(&[PageId(0)], 1).unwrap();
    planner.commit(&plan).unwrap();
    assert_eq!(
        ElasticRamRuntime::new(planner),
        Err(RamMaterializationError::NonColdInitialPlanner)
    );
}
