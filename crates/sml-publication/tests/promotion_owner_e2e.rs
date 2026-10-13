//! Dedicated CI executes real primary GitHub + real owner authentication.
use serde_json::{json, Value};
use sml_publication::{
    promotion_review::{
        observe_github, qualification_assert_merged, qualification_pr_for_commit,
        review_authenticated_evidence,
    },
    sha256,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn owner(executable: &Path, root: &Path, args: &[&str]) -> Value {
    let output = Command::new(executable)
        .current_dir(root)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "owner refused: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
#[ignore = "requires exact owner, primary GitHub read token and PR/commit identity; mandatory dedicated CI executes it"]
fn primary_github_and_authenticated_aggregate_records_feed_human_review_only() {
    let token = std::env::var("GITHUB_TOKEN").expect("primary read token required");
    let source = std::env::var("SML_REVIEW_SOURCE_SHA").expect("exact build source required");
    let pr: u64 = std::env::var("SML_REVIEW_PR")
        .expect("PR selection required")
        .parse()
        .unwrap();
    if pr == 0 {
        let merged = qualification_pr_for_commit(&source, Some(&token)).unwrap();
        qualification_assert_merged(merged, &source, Some(&token)).unwrap();
        assert!(observe_github(merged, Some(&token)).is_err());
        let receipt = json!({"source_sha":source,"repository":"Memorithm/SML-GENIUS","pull_request":merged,
            "primary_integrated_identity":"verified","closed_PR_refused":true,"merge_authorization":false,
            "scientific_qualification":"not_evaluated"});
        println!("integrated_review_receipt={receipt}");
        if let Some(path) = std::env::var_os("SML_REVIEW_EVIDENCE") {
            fs::write(path, serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
        }
        return;
    }
    let observed = observe_github(pr, Some(&token)).unwrap();
    assert_eq!(observed.pr_head_sha, source);
    let executable =
        PathBuf::from(std::env::var_os("SML_PUBLICATION_OWNER").expect("exact owner required"))
            .canonicalize()
            .unwrap();
    let folder = std::env::temp_dir().join(format!(
        "sml-review-owner-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&folder).unwrap();
    let temp = Temp(folder);
    let root = temp.0.join("dossier");
    fs::create_dir(&root).unwrap();
    fs::create_dir(root.join("aggregates")).unwrap();
    let key = temp.0.join("fixture-key");
    assert!(Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-f"])
        .arg(&key)
        .status()
        .unwrap()
        .success());
    let signers = temp.0.join("allowed_signers");
    fs::write(
        &signers,
        format!(
            "development-evaluator {}",
            fs::read_to_string(key.with_extension("pub")).unwrap()
        ),
    )
    .unwrap();
    let policy_file = temp.0.join("operator-policy.json");
    let policy = json!({"schema_version":1,"repository":"Memorithm/SML-GENIUS","pull_request":pr,
        "evaluators":["development-evaluator"],"verifier_sha256":sha256(&fs::read(&executable).unwrap()).unwrap(),
        "allowed_signers_sha256":sha256(&fs::read(&signers).unwrap()).unwrap()});
    fs::write(&policy_file, serde_json::to_vec(&policy).unwrap()).unwrap();
    let evidence=[("build","selection"),("required_tests","selection"),("determinism","auxiliary"),("other","protected")]
        .into_iter().enumerate().map(|(i,(kind,role))| {
            let data=serde_json::to_vec(&json!({"schema_version":1,"scope":"development_fixture",
                "candidate_head_sha":source,"kind":kind,"source_evidence_sha256":sha256(format!("development-fixture-{i}").as_bytes()).unwrap(),"passed":1,"total":1})).unwrap();
            let name=format!("aggregate-{i}.json");fs::write(root.join("aggregates").join(&name),&data).unwrap();
            json!({"name":format!("development-fixture-{i}"),"kind":kind,"role":role,"path":name,
                "sha256":sha256(&data).unwrap(),"locator":format!("development-qualification:{i}")})
        }).collect::<Vec<_>>();
    let record = json!({"schema_version":1,"contract":"sml.rsi-trusted-review@1.0.0","repository":"Memorithm/SML-GENIUS",
        "pull_request":pr,"parent_main_sha":observed.current_main_sha,"candidate_head_sha":observed.pr_head_sha,
        "candidate_class":"other","branch":observed.branch,"objective":"Development metadata adapter qualification, no model result",
        "max_wall_seconds":30,"max_candidates":1,"max_training_examples":0,"max_storage_bytes":262144,
        "rollback_note":"exact parent","evidence":evidence,"comparisons":[],"known_regressions":[]});
    fs::write(
        root.join("review.json"),
        serde_json::to_vec(&record).unwrap(),
    )
    .unwrap();
    fs::write(
        root.join("scirust-verify.toml"),
        r#"schema_version = 1
[artifact]
name = "SML Development trusted review metadata"
[verification]
profile = "basic"
timeout_secs = 30
stdout_max_bytes = 262144
stderr_max_bytes = 65536
[cargo]
enabled = false
[[custom_checks]]
id = "rsi-promotion-review"
program = "/usr/bin/cat"
args = ["review.json"]
level = "required"
"#,
    )
    .unwrap();
    let sealed = owner(
        &executable,
        &root,
        &["verify", root.to_str().unwrap(), "--json"],
    );
    assert_eq!(sealed["overall_verdict"], "PASS");
    let run = sealed["run_id"].as_str().unwrap();
    owner(
        &executable,
        &root,
        &[
            "attest",
            run,
            "--principal",
            "development-evaluator",
            "--identity",
            key.to_str().unwrap(),
            "--json",
        ],
    );
    let review = || {
        review_authenticated_evidence(
            &root,
            run,
            &executable,
            &policy_file,
            &signers,
            Some(&token),
        )
    };
    let receipt = review().unwrap();
    assert!(!receipt.is_merge_authorization());
    let value = serde_json::to_value(&receipt).unwrap();
    assert_eq!(value["status"], "EligibleForHumanReview");
    assert_eq!(value["evidence_scope"], "development_fixture");
    assert_eq!(value["scientific_qualification"], "not_evaluated");
    assert_eq!(value["aggregate_records"], 4);
    let mut refused = policy.clone();
    refused["evaluators"] = json!(["unrelated-evaluator"]);
    fs::write(&policy_file, serde_json::to_vec(&refused).unwrap()).unwrap();
    assert!(review().is_err());
    fs::write(&policy_file, serde_json::to_vec(&policy).unwrap()).unwrap();
    let bytes = fs::read(root.join("aggregates/aggregate-3.json")).unwrap();
    fs::write(root.join("aggregates/aggregate-3.json"), b"changed").unwrap();
    assert!(review().is_err());
    fs::write(root.join("aggregates/aggregate-3.json"), bytes).unwrap();
    let envelope = root.join(format!(".scirust-verify/attestations/{run}.json"));
    fs::remove_file(envelope).unwrap();
    assert!(review().is_err());
    println!("trusted_review_receipt={value}");
    if let Some(path) = std::env::var_os("SML_REVIEW_EVIDENCE") {
        fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    }
    println!("qualified live primary GitHub + authenticated aggregate fixture + unchanged RSI eligibility library; no merge or model authority");
}
