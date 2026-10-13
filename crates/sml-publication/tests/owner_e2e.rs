use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use sml_core::{
    elastic_weights::{
        ElasticWeightPage, ElasticWeightPlanner, ElasticWeightPolicy, ResidencyTier,
        WeightPrecision,
    },
    PageId,
};
use sml_publication::{admit_external_weights, sha256, PublishedWeight, WeightPublicationManifest};
use sml_runtime::{build_weight_artifact, ElasticRamRuntime, RamMaterializationBudget};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "sml-publication-e2e-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn owner(executable: &Path, root: &Path, args: &[&str]) -> serde_json::Value {
    let output = Command::new(executable)
        .args(args)
        .current_dir(root)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn seal(
    executable: &Path,
    root: &Path,
    key: &Path,
    manifest: &WeightPublicationManifest,
) -> String {
    fs::write(
        root.join("publication.json"),
        serde_json::to_vec(manifest).unwrap(),
    )
    .unwrap();
    let report = owner(
        executable,
        root,
        &["verify", root.to_str().unwrap(), "--json"],
    );
    assert_eq!(report["overall_verdict"], "PASS");
    let run = report["run_id"].as_str().unwrap().to_owned();
    owner(
        executable,
        root,
        &[
            "attest",
            &run,
            "--principal",
            "development-publisher",
            "--identity",
            key.to_str().unwrap(),
            "--json",
        ],
    );
    run
}

fn runtime() -> ElasticRamRuntime {
    ElasticRamRuntime::new(
        ElasticWeightPlanner::new(
            vec![ElasticWeightPage::new(
                PageId(0),
                8,
                WeightPrecision::Boolean,
                ResidencyTier::Disk,
                500_000,
            )
            .unwrap()],
            ElasticWeightPolicy {
                ram_limit_bytes: 1,
                vram_limit_bytes: 0,
                max_active_parameters: 8,
                ternary_threshold_ppm: 600_000,
                residual4_threshold_ppm: 900_000,
                vram_threshold_ppm: 900_000,
            },
        )
        .unwrap(),
    )
    .unwrap()
}

/// The dedicated CI gate supplies an exact pinned, compiled owner. This test
/// makes real SSHSIG signatures, then calls the public admission and RAM APIs.
#[test]
#[ignore = "requires SML_PUBLICATION_OWNER; mandatory dedicated owner-admission CI executes it"]
fn authenticated_publisher_binds_exact_code_bytes_and_immutable_ram_activation() {
    let executable =
        PathBuf::from(std::env::var_os("SML_PUBLICATION_OWNER").expect("owner required"))
            .canonicalize()
            .unwrap();
    let code_sha = std::env::var("SML_PUBLICATION_CODE_SHA").expect("exact source SHA required");
    let temp = Temp::new();
    let root = temp.0.join("dossier");
    fs::create_dir(&root).unwrap();
    fs::create_dir(root.join("weights")).unwrap();
    let key = temp.0.join("key");
    assert!(Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-f"])
        .arg(&key)
        .status()
        .unwrap()
        .success());
    let trust = temp.0.join("allowed_signers");
    fs::write(
        &trust,
        format!(
            "development-publisher {}",
            fs::read_to_string(key.with_extension("pub")).unwrap()
        ),
    )
    .unwrap();
    let policy_file = temp.0.join("policy.json");
    let policy = serde_json::json!({
        "schema_version":1, "code_sha":code_sha, "publication_id":"development-fixture-v1",
        "publishers":["development-publisher"], "verifier_sha256":sha256(&fs::read(&executable).unwrap()).unwrap(),
        "allowed_signers_sha256":sha256(&fs::read(&trust).unwrap()).unwrap()
    });
    fs::write(&policy_file, serde_json::to_vec(&policy).unwrap()).unwrap();
    fs::write(
        root.join("scirust-verify.toml"),
        r#"schema_version = 1
[artifact]
name = "SML development publication qualification"
[verification]
profile = "basic"
timeout_secs = 30
stdout_max_bytes = 262144
stderr_max_bytes = 65536
[cargo]
enabled = false
[[custom_checks]]
id = "weight-publication"
program = "/usr/bin/cat"
args = ["publication.json"]
level = "required"
"#,
    )
    .unwrap();
    let bytes = build_weight_artifact(PageId(0), 0, WeightPrecision::Boolean, 8, &[0xa5]).unwrap();
    let weight_path = root.join("weights/page0.smlw");
    fs::write(&weight_path, &bytes).unwrap();
    let mut manifest = WeightPublicationManifest {
        schema_version: 1,
        repository: "Memorithm/SML-GENIUS".into(),
        code_sha,
        publication_id: "development-fixture-v1".into(),
        publisher: "development-publisher".into(),
        weights: vec![PublishedWeight {
            path: "page0.smlw".into(),
            sha256: sha256(&bytes).unwrap(),
        }],
    };
    let run = seal(&executable, &root, &key, &manifest);
    let admit = |run: &str| admit_external_weights(&root, run, &executable, &policy_file, &trust);
    let admitted = admit(&run).unwrap();
    assert_eq!(admitted.receipt().publisher, "development-publisher");
    assert_eq!(admitted.receipt().scientific_qualification, "not_evaluated");
    assert!(!admitted.receipt().merge_authorization);
    println!(
        "admission_receipt={}",
        serde_json::to_string(admitted.receipt()).unwrap()
    );
    // Changed payload with a newly valid CRC cannot change authenticated bytes.
    let replacement =
        build_weight_artifact(PageId(0), 0, WeightPrecision::Boolean, 8, &[0x5a]).unwrap();
    fs::write(&weight_path, replacement).unwrap();
    assert!(admit(&run).is_err());
    let mut runtime = runtime();
    let plan = runtime.planner().plan_cycle(&[PageId(0)], 1).unwrap();
    let stage = admitted
        .stage(&runtime, &plan, RamMaterializationBudget::default())
        .unwrap();
    assert_eq!(stage.page(PageId(0)).unwrap().payload(), &[0xa5]);
    assert_eq!(stage.report().files_read, 0);
    assert_eq!(stage.report().retained_artifacts_read, 1);
    runtime.commit(stage).unwrap();
    assert_eq!(runtime.ram_page(PageId(0)).unwrap().payload(), &[0xa5]);
    assert!(admitted
        .stage(&runtime, &plan, RamMaterializationBudget::default())
        .is_err());
    fs::write(&weight_path, &bytes).unwrap();
    for (field, replacement) in [
        ("code_sha", serde_json::json!("b".repeat(40))),
        ("publication_id", serde_json::json!("old-publication")),
        ("publishers", serde_json::json!(["another-publisher"])),
        ("allowed_signers_sha256", serde_json::json!("0".repeat(64))),
        ("verifier_sha256", serde_json::json!("0".repeat(64))),
    ] {
        let mut rejected = policy.clone();
        rejected[field] = replacement;
        fs::write(&policy_file, serde_json::to_vec(&rejected).unwrap()).unwrap();
        assert!(admit(&run).is_err(), "accepted mismatched {field}");
    }
    fs::write(&policy_file, serde_json::to_vec(&policy).unwrap()).unwrap();
    let envelope_path = root.join(format!(".scirust-verify/attestations/{run}.json"));
    let envelope = fs::read(&envelope_path).unwrap();
    fs::remove_file(&envelope_path).unwrap();
    assert!(admit(&run).is_err());
    let mut corrupted: serde_json::Value = serde_json::from_slice(&envelope).unwrap();
    corrupted["signature"] = "invalid signature".into();
    fs::write(&envelope_path, serde_json::to_vec(&corrupted).unwrap()).unwrap();
    assert!(admit(&run).is_err());
    fs::write(&envelope_path, envelope).unwrap();
    let record = root.join(format!(
        ".scirust-verify/runs/{run}/logs/custom-weight-publication.out.log"
    ));
    let saved = fs::read(&record).unwrap();
    fs::write(&record, b"{\"authenticated\":true}").unwrap();
    assert!(admit(&run).is_err());
    fs::write(record, saved).unwrap();
    // ACL must never come from inside the producer dossier.
    fs::write(
        root.join("policy.json"),
        serde_json::to_vec(&policy).unwrap(),
    )
    .unwrap();
    assert!(
        admit_external_weights(&root, &run, &executable, &root.join("policy.json"), &trust)
            .is_err()
    );
    for bad_path in ["../publication.json", "/etc/passwd", "page0.smlw\\bad"] {
        manifest.weights[0].path = bad_path.into();
        let rejected_run = seal(&executable, &root, &key, &manifest);
        assert!(admit(&rejected_run).is_err(), "accepted {bad_path}");
    }
    manifest.weights[0].path = "page0.smlw".into();
    manifest.weights.push(PublishedWeight {
        path: "page0.smlw".into(),
        sha256: sha256(&bytes).unwrap(),
    });
    let duplicate = seal(&executable, &root, &key, &manifest);
    assert!(admit(&duplicate).is_err());
    manifest.weights.pop();
    #[cfg(unix)]
    {
        fs::remove_file(&weight_path).unwrap();
        std::os::unix::fs::symlink(root.join("publication.json"), &weight_path).unwrap();
        assert!(admit(&run).is_err());
    }
    println!("qualified: real SSHSIG + independent ACL + exact code/release/bytes + immutable RAM; negative controls refused; no scientific promotion");
}

#[test]
fn publication_json_refuses_unknown_and_duplicate_authority_fields() {
    let bad = [
        r#"{"schema_version":1,"repository":"x","code_sha":"x","publication_id":"x","publisher":"x","weights":[],"authenticated":true}"#,
        r#"{"schema_version":1,"repository":"x","code_sha":"x","publication_id":"x","publisher":"x","publisher":"y","weights":[]}"#,
        r#"{"schema_version":1,"repository":"x","code_sha":"x","publication_id":"x","publisher":"x","weights":[{"path":"x","sha256":"x","authorized":true}]}"#,
    ];
    for json in bad {
        assert!(serde_json::from_str::<WeightPublicationManifest>(json).is_err());
    }
}
