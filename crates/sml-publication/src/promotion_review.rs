//! Live GitHub observations plus authenticated aggregate evidence feed the
//! existing RSI eligibility library. This module has no merge/promote operation.
use crate::{
    hex, independent, read_authenticated_record, read_bounded, relative_file, sha256, RecordTrust,
    Result, JSON_LIMIT, REPOSITORY,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sml_rsi::{
    manifest::{
        CandidateBudget, CandidateClass, EvidenceKind, EvidenceRef, KnownRegression,
        MetricComparison, MetricDirection, RegressionSeverity, RsiCandidateManifest,
    },
    promotion::{evaluate_promotion_eligibility, PromotionPolicy, PromotionReviewContext},
};
use std::{collections::BTreeSet, io::Read, path::Path, time::Duration};

const REVIEW_RECORD: &str = "logs/custom-rsi-promotion-review.out.log";
const CONTRACT: &str = "sml.rsi-trusted-review@1.0.0";
const REPOSITORY_ID: u64 = 1_378_772_339;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewPolicy {
    schema_version: u32,
    repository: String,
    pull_request: u64,
    evaluators: BTreeSet<String>,
    verifier_sha256: String,
    allowed_signers_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewRecord {
    schema_version: u32,
    contract: String,
    repository: String,
    pull_request: u64,
    parent_main_sha: String,
    candidate_head_sha: String,
    candidate_class: String,
    branch: String,
    objective: String,
    max_wall_seconds: u64,
    max_candidates: u64,
    max_training_examples: u64,
    max_storage_bytes: u64,
    rollback_note: String,
    evidence: Vec<ReviewEvidence>,
    comparisons: Vec<Comparison>,
    known_regressions: Vec<Regression>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewEvidence {
    name: String,
    kind: String,
    role: String,
    /// SHA-256-bound aggregate validation metadata, never raw holdout labels.
    path: String,
    sha256: String,
    locator: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AggregateRecord {
    schema_version: u32,
    scope: String,
    candidate_head_sha: String,
    kind: String,
    source_evidence_sha256: String,
    passed: u64,
    total: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Comparison {
    metric: String,
    direction: String,
    parent_value: i128,
    candidate_value: i128,
    hard_gate: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Regression {
    area: String,
    description: String,
    severity: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitHubObservation {
    pub repository: String,
    pub pull_request: u64,
    pub current_main_sha: String,
    pub pr_head_sha: String,
    pub branch: String,
}

fn parse_observation(pr: u64, branch: &Value, pull: &Value) -> Result<GitHubObservation> {
    let main = branch["commit"]["sha"]
        .as_str()
        .ok_or("missing primary main identity")?;
    let head = pull["head"]["sha"]
        .as_str()
        .ok_or("missing primary PR identity")?;
    let name = pull["head"]["ref"]
        .as_str()
        .ok_or("missing primary branch identity")?;
    if pull["number"] != pr
        || pull["state"] != "open"
        || pull["merged"] != false
        || pull["draft"] != false
        || pull["base"]["ref"] != "main"
        || pull["base"]["sha"] != main
        || pull["base"]["repo"]["id"] != REPOSITORY_ID
        || pull["head"]["repo"]["id"] != REPOSITORY_ID
        || pull["base"]["repo"]["full_name"] != REPOSITORY
        || pull["head"]["repo"]["full_name"] != REPOSITORY
        || !hex(main, 40)
        || !hex(head, 40)
    {
        return Err(
            "primary GitHub PR is stale, closed, draft, forked or not the pinned SML main/head"
                .into(),
        );
    }
    Ok(GitHubObservation {
        repository: REPOSITORY.into(),
        pull_request: pr,
        current_main_sha: main.into(),
        pr_head_sha: head.into(),
        branch: name.into(),
    })
}

fn github_json(path: &str, token: Option<&str>) -> Result<Value> {
    let agent = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout(Duration::from_secs(15))
        .build();
    let mut request = agent
        .get(&format!("https://api.github.com/repos/{REPOSITORY}/{path}"))
        .set("User-Agent", "memorithm-sml-trusted-review")
        .set("Accept", "application/vnd.github+json")
        .set("X-GitHub-Api-Version", "2022-11-28")
        .set("Cache-Control", "no-cache");
    if let Some(token) = token {
        if token.is_empty() || token.len() > 4096 || token.bytes().any(|x| !x.is_ascii_graphic()) {
            return Err("invalid operator GitHub transport credential".into());
        }
        request = request.set("Authorization", &format!("Bearer {token}"));
    }
    // The provider, endpoint and TLS policy are fixed; no producer URL, HTTP
    // redirect, caller JSON snapshot or producer credential selects authority.
    let response = request
        .call()
        .map_err(|_| "primary GitHub observation failed")?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(JSON_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > JSON_LIMIT {
        return Err("primary GitHub response exceeded bound".into());
    }
    Ok(serde_json::from_slice(&bytes)?)
}

/// Read-only primary-provider observation. Returned metadata is not permission
/// to merge; the public review path independently re-observes it at completion.
pub fn observe_github(pr: u64, token: Option<&str>) -> Result<GitHubObservation> {
    if pr == 0 || pr > 1_000_000 {
        return Err("invalid operator PR selection".into());
    }
    parse_observation(
        pr,
        &github_json("branches/main", token)?,
        &github_json(&format!("pulls/{pr}"), token)?,
    )
}

fn evidence_kind(value: &str) -> Result<EvidenceKind> {
    Ok(match value {
        "build" => EvidenceKind::Build,
        "required_tests" => EvidenceKind::RequiredTests,
        "determinism" => EvidenceKind::Determinism,
        "quality" => EvidenceKind::Quality,
        "calibration" => EvidenceKind::Calibration,
        "context" => EvidenceKind::Context,
        "benchmark" => EvidenceKind::Benchmark,
        "resource_budget" => EvidenceKind::ResourceBudget,
        "provenance" => EvidenceKind::Provenance,
        "policy" => EvidenceKind::Policy,
        "other" => EvidenceKind::Other,
        _ => return Err("unknown evidence kind".into()),
    })
}

#[derive(Serialize)]
pub struct TrustedEligibilityReceipt {
    observation: GitHubObservation,
    evaluator: String,
    bundle_sha256: String,
    record_sha256: String,
    status: String,
    reasons: Vec<String>,
    aggregate_records: usize,
    evidence_scope: String,
    scientific_qualification: &'static str,
    merge_authorization: bool,
}

impl TrustedEligibilityReceipt {
    pub const fn is_merge_authorization(&self) -> bool {
        false
    }
}

/// Qualification-only lookup for an integrated commit; never a promotion gate.
pub fn qualification_pr_for_commit(sha: &str, token: Option<&str>) -> Result<u64> {
    if !hex(sha, 40) {
        return Err("invalid qualification commit identity".into());
    }
    let rows = github_json(&format!("commits/{sha}/pulls"), token)?;
    let matches = rows
        .as_array()
        .ok_or("invalid primary commit/PR collection")?
        .iter()
        .filter(|x| {
            x["merge_commit_sha"] == sha
                && x["base"]["repo"]["id"] == REPOSITORY_ID
                && x["state"] == "closed"
        })
        .filter_map(|x| x["number"].as_u64())
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err("ambiguous or absent primary integrated PR identity".into());
    }
    Ok(matches[0])
}

pub fn qualification_assert_merged(pr: u64, sha: &str, token: Option<&str>) -> Result<()> {
    if pr == 0 || pr > 1_000_000 || !hex(sha, 40) {
        return Err("invalid qualification identity".into());
    }
    let pull = github_json(&format!("pulls/{pr}"), token)?;
    if pull["state"] != "closed"
        || pull["merged"] != true
        || pull["merge_commit_sha"] != sha
        || pull["base"]["repo"]["id"] != REPOSITORY_ID
    {
        return Err("primary integrated qualification identity mismatch".into());
    }
    Ok(())
}

fn evaluate_record(
    root: &Path,
    bytes: &[u8],
    observed: &GitHubObservation,
) -> Result<(sml_rsi::promotion::PromotionEligibilityReport, String)> {
    let wire: ReviewRecord = serde_json::from_slice(bytes)?;
    if wire.schema_version != 1
        || wire.contract != CONTRACT
        || wire.repository != REPOSITORY
        || wire.pull_request != observed.pull_request
        || wire.parent_main_sha != observed.current_main_sha
        || wire.candidate_head_sha != observed.pr_head_sha
        || wire.branch != observed.branch
        || wire.evidence.is_empty()
        || wire.evidence.len() > 64
        || wire.comparisons.len() > 64
        || wire.known_regressions.len() > 64
    {
        return Err(
            "authenticated review does not bind current primary GitHub identity or bounds".into(),
        );
    }
    let evidence_root = relative_file(root, "aggregates")?;
    let mut selection = BTreeSet::new();
    let mut protected = BTreeSet::new();
    let mut evidence = Vec::new();
    let mut paths = BTreeSet::new();
    let mut sources = BTreeSet::new();
    let mut scopes = BTreeSet::new();
    for item in &wire.evidence {
        if !hex(&item.sha256, 64) || !paths.insert(&item.path) {
            return Err("duplicate/invalid evidence identity".into());
        }
        let data = read_bounded(&relative_file(&evidence_root, &item.path)?, 16_384)?;
        if sha256(&data)? != item.sha256 {
            return Err("aggregate evidence content changed".into());
        }
        let record: AggregateRecord = serde_json::from_slice(&data)?;
        if record.schema_version != 1
            || !["development_fixture", "aggregate_only"].contains(&record.scope.as_str())
            || record.candidate_head_sha != observed.pr_head_sha
            || record.kind != item.kind
            || !hex(&record.source_evidence_sha256, 64)
            || !sources.insert(record.source_evidence_sha256.clone())
            || record.total == 0
            || record.passed != record.total
        {
            return Err("aggregate validation record not compatible or not successful".into());
        }
        scopes.insert(record.scope);
        match item.role.as_str() {
            "selection" => {
                selection.insert(item.sha256.clone());
            }
            "protected" => {
                protected.insert(item.sha256.clone());
            }
            "auxiliary" => {}
            _ => return Err("unknown evidence partition role".into()),
        }
        evidence.push(EvidenceRef::new(
            evidence_kind(&item.kind)?,
            item.name.clone(),
            item.locator.clone(),
            item.sha256.clone(),
        )?);
    }
    if scopes.len() != 1 {
        return Err("mixed fixture and real aggregate evidence scopes".into());
    }
    let class = match wire.candidate_class.as_str() {
        "data_curriculum" => CandidateClass::DataCurriculum,
        "model_or_expert" => CandidateClass::ModelOrExpert,
        "routing_memory_context" => CandidateClass::RoutingMemoryContext,
        "topology_or_wiring" => CandidateClass::TopologyOrWiring,
        "runtime_policy" => CandidateClass::RuntimePolicy,
        "evaluator_or_loop" => CandidateClass::EvaluatorOrLoop,
        "other" => CandidateClass::Other,
        _ => return Err("unknown candidate class".into()),
    };
    if scopes.contains("development_fixture") && class != CandidateClass::Other {
        return Err("Development fixtures cannot qualify model or architecture candidates".into());
    }
    let comparisons = wire
        .comparisons
        .into_iter()
        .map(|x| {
            Ok(MetricComparison {
                metric: x.metric,
                direction: match x.direction.as_str() {
                    "higher" => MetricDirection::HigherIsBetter,
                    "lower" => MetricDirection::LowerIsBetter,
                    _ => return Err("unknown metric direction".into()),
                },
                parent_value: x.parent_value,
                candidate_value: x.candidate_value,
                hard_gate: x.hard_gate,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let regressions = wire
        .known_regressions
        .into_iter()
        .map(|x| {
            Ok(KnownRegression {
                area: x.area,
                description: x.description,
                severity: match x.severity.as_str() {
                    "informational" => RegressionSeverity::Informational,
                    "material" => RegressionSeverity::Material,
                    "critical" => RegressionSeverity::Critical,
                    _ => return Err("unknown regression severity".into()),
                },
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let manifest = RsiCandidateManifest::new(
        wire.parent_main_sha,
        wire.candidate_head_sha,
        wire.branch,
        class,
        wire.objective,
        CandidateBudget {
            max_wall_seconds: wire.max_wall_seconds,
            max_candidates: wire.max_candidates,
            max_training_examples: wire.max_training_examples,
            max_storage_bytes: wire.max_storage_bytes,
        },
        evidence,
        comparisons,
        regressions,
        wire.rollback_note,
    )?;
    let context = PromotionReviewContext {
        current_main_sha: observed.current_main_sha.clone(),
        pr_head_sha: observed.pr_head_sha.clone(),
        selection_evidence_sha256: selection,
        protected_evidence_sha256: protected,
    };
    Ok((
        evaluate_promotion_eligibility(&manifest, &context, &PromotionPolicy::baseline()),
        scopes.pop_first().ok_or("missing scope")?,
    ))
}

pub fn review_authenticated_evidence(
    dossier: &Path,
    run: &str,
    owner: &Path,
    policy_file: &Path,
    signers: &Path,
    github_token: Option<&str>,
) -> Result<TrustedEligibilityReceipt> {
    let root = dossier.canonicalize()?;
    let policy_path = independent(policy_file, &root)?;
    let policy: ReviewPolicy = serde_json::from_slice(&read_bounded(&policy_path, JSON_LIMIT)?)?;
    if policy.schema_version != 1 || policy.repository != REPOSITORY {
        return Err("invalid independent SML review policy".into());
    }
    let observed = observe_github(policy.pull_request, github_token)?;
    let record = read_authenticated_record(
        &root,
        run,
        owner,
        signers,
        RecordTrust {
            principals: &policy.evaluators,
            verifier_sha256: &policy.verifier_sha256,
            allowed_signers_sha256: &policy.allowed_signers_sha256,
        },
        REVIEW_RECORD,
    )?;
    let (report, scope) = evaluate_record(&root, &record.bytes, &observed)?;
    if report.is_merge_authorization() {
        return Err("eligibility library unexpectedly claimed merge authority".into());
    }
    if observe_github(policy.pull_request, github_token)? != observed {
        return Err("primary GitHub main/head changed during evidence review".into());
    }
    Ok(TrustedEligibilityReceipt {
        observation: observed,
        evaluator: record.principal,
        bundle_sha256: record.bundle_sha256,
        record_sha256: sha256(&record.bytes)?,
        status: format!("{:?}", report.status),
        reasons: report.reasons.iter().map(|x| format!("{x:?}")).collect(),
        aggregate_records: report.checked_evidence,
        evidence_scope: scope,
        scientific_qualification: "not_evaluated",
        merge_authorization: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    fn observed() -> GitHubObservation {
        GitHubObservation {
            repository: REPOSITORY.into(),
            pull_request: 560,
            current_main_sha: "1".repeat(40),
            pr_head_sha: "2".repeat(40),
            branch: "audit/fixture".into(),
        }
    }
    fn records(root: &Path) -> Value {
        fs::create_dir(root.join("aggregates")).unwrap();
        let evidence=[("build","selection"),("required_tests","selection"),("determinism","auxiliary"),("other","protected")]
            .into_iter().enumerate().map(|(i,(kind,role))| {
                let bytes=serde_json::to_vec(&json!({"schema_version":1,"scope":"development_fixture",
                    "candidate_head_sha":"2".repeat(40),"kind":kind,"source_evidence_sha256":sha256(format!("underlying-development-fixture-{i}").as_bytes()).unwrap(),"passed":1,"total":1})).unwrap();
                let name=format!("record-{i}.json");fs::write(root.join("aggregates").join(&name),&bytes).unwrap();
                json!({"name":format!("fixture-{i}"),"kind":kind,"role":role,"path":name,"sha256":sha256(&bytes).unwrap(),"locator":format!("development-fixture:{i}")})
            }).collect::<Vec<_>>();
        json!({"schema_version":1,"contract":CONTRACT,"repository":REPOSITORY,"pull_request":560,
            "parent_main_sha":"1".repeat(40),"candidate_head_sha":"2".repeat(40),"candidate_class":"other",
            "branch":"audit/fixture","objective":"Development metadata contract qualification only",
            "max_wall_seconds":30,"max_candidates":1,"max_training_examples":0,"max_storage_bytes":262144,
            "rollback_note":"exact parent","evidence":evidence,"comparisons":[],"known_regressions":[]})
    }
    #[test]
    fn primary_snapshot_rejects_closed_draft_fork_missing_and_incoherent_sha() {
        let branch = json!({"commit":{"sha":"1".repeat(40)}});
        let pull = json!({"number":560,"state":"open","merged":false,"draft":false,
            "base":{"ref":"main","sha":"1".repeat(40),"repo":{"id":REPOSITORY_ID,"full_name":REPOSITORY}},
            "head":{"sha":"2".repeat(40),"ref":"audit/fixture","repo":{"id":REPOSITORY_ID,"full_name":REPOSITORY}}});
        assert_eq!(parse_observation(560, &branch, &pull).unwrap(), observed());
        for (pointer, value) in [
            ("/state", json!("closed")),
            ("/draft", json!(true)),
            ("/merged", json!(true)),
            ("/head/repo/id", json!(0)),
            ("/base/sha", json!("3".repeat(40))),
            ("/head/sha", Value::Null),
        ] {
            let mut changed = pull.clone();
            *changed.pointer_mut(pointer).unwrap() = value;
            assert!(
                parse_observation(560, &branch, &changed).is_err(),
                "accepted {pointer}"
            );
        }
    }
    #[test]
    fn authenticated_aggregate_adapter_uses_rsi_library_without_merge_authority() {
        let root = std::env::temp_dir().join(format!(
            "sml-review-contract-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let wire = records(&root);
        let bytes = serde_json::to_vec(&wire).unwrap();
        let (report, scope) = evaluate_record(&root, &bytes, &observed()).unwrap();
        assert!(report.eligible_for_human_review());
        assert!(!report.is_merge_authorization());
        assert_eq!(scope, "development_fixture");
        let mut model = wire.clone();
        model["candidate_class"] = "model_or_expert".into();
        assert!(evaluate_record(&root, &serde_json::to_vec(&model).unwrap(), &observed()).is_err());
        let mut regression = wire.clone();
        regression["comparisons"] = json!([{"metric":"fixture-hard-gate","direction":"higher","parent_value":8,"candidate_value":7,"hard_gate":true}]);
        let (refused, _) = evaluate_record(
            &root,
            &serde_json::to_vec(&regression).unwrap(),
            &observed(),
        )
        .unwrap();
        assert!(!refused.eligible_for_human_review());
        let mut stale = observed();
        stale.current_main_sha = "3".repeat(40);
        assert!(evaluate_record(&root, &bytes, &stale).is_err());
        let mut drift = wire.clone();
        drift["evidence"][3]["path"] = "../outside".into();
        assert!(evaluate_record(&root, &serde_json::to_vec(&drift).unwrap(), &observed()).is_err());
        let mut unknown = wire.clone();
        unknown["authenticated"] = true.into();
        assert!(
            evaluate_record(&root, &serde_json::to_vec(&unknown).unwrap(), &observed()).is_err()
        );
        let path = root.join("aggregates/record-3.json");
        let original = fs::read(&path).unwrap();
        let mut wrapped: Value = serde_json::from_slice(&original).unwrap();
        let selection: Value =
            serde_json::from_slice(&fs::read(root.join("aggregates/record-0.json")).unwrap())
                .unwrap();
        wrapped["source_evidence_sha256"] = selection["source_evidence_sha256"].clone();
        let changed = serde_json::to_vec(&wrapped).unwrap();
        fs::write(&path, &changed).unwrap();
        let mut repeated = wire.clone();
        repeated["evidence"][3]["sha256"] = sha256(&changed).unwrap().into();
        assert!(
            evaluate_record(&root, &serde_json::to_vec(&repeated).unwrap(), &observed()).is_err()
        );
        fs::write(&path, original).unwrap();
        fs::write(root.join("aggregates/record-3.json"), b"changed").unwrap();
        assert!(evaluate_record(&root, &bytes, &observed()).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
