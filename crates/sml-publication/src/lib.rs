#![forbid(unsafe_code)]
//! Optional external weight admission tool. The model/runtime does not depend
//! on this crate or SciRust-Verify. Signatures are verified by their owner,
//! never by producer-supplied booleans or an imported authentication receipt.

use std::{
    collections::BTreeSet,
    error::Error,
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use sml_core::elastic_weights::ElasticWeightPlan;
use sml_runtime::{
    ElasticRamRuntime, MaterializedRamStage, RamMaterializationBudget, WeightArtifactCatalog,
};
use sml_store::raw_context::sha256_raw_bytes;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
const JSON_LIMIT: usize = 262_144;
const WEIGHT_LIMIT: usize = 64 * 1024 * 1024;
const RECORD: &str = "logs/custom-weight-publication.out.log";
const REPOSITORY: &str = "Memorithm/SML-GENIUS";
static NEXT: AtomicU64 = AtomicU64::new(0);

pub mod promotion_review;

pub(crate) struct RecordTrust<'a> {
    pub principals: &'a BTreeSet<String>,
    pub verifier_sha256: &'a str,
    pub allowed_signers_sha256: &'a str,
}

pub(crate) struct AuthenticatedRecord {
    pub principal: String,
    pub bundle_sha256: String,
    pub bytes: Vec<u8>,
}

pub(crate) fn read_authenticated_record(
    root: &Path,
    run: &str,
    owner: &Path,
    signers: &Path,
    policy: RecordTrust<'_>,
    record_name: &str,
) -> Result<AuthenticatedRecord> {
    if !identifier(run)
        || !run.starts_with("run-")
        || policy.principals.is_empty()
        || policy.principals.len() > 64
        || policy.principals.iter().any(|x| !identifier(x))
        || !hex(policy.verifier_sha256, 64)
        || !hex(policy.allowed_signers_sha256, 64)
    {
        return Err("invalid independent evidence trust policy".into());
    }
    let owner = independent(owner, root)?;
    let signers = independent(signers, root)?;
    if sha256(&read_bounded(&owner, WEIGHT_LIMIT)?)? != policy.verifier_sha256 {
        return Err("owner executable pin mismatch".into());
    }
    let trust_bytes = read_bounded(&signers, JSON_LIMIT)?;
    if sha256(&trust_bytes)? != policy.allowed_signers_sha256 {
        return Err("publisher trust root pin mismatch".into());
    }
    let trust = TrustSnapshot::new(&trust_bytes)?;
    let auth = authenticate(&owner, root, run, &trust.path())?;
    if auth["run_id"] != run
        || auth["authenticity"]["status"] != "verified"
        || auth["integrity"]["status"] != "verified"
        || auth["authenticity"]["namespace"] != "scirust-verify-evidence-v1"
    {
        return Err("owner did not authenticate this sealed evidence record".into());
    }
    let principal = auth["authenticity"]["principal"]
        .as_str()
        .ok_or("missing authenticated principal")?;
    if !policy.principals.contains(principal) {
        return Err("principal not admitted by independent ACL".into());
    }
    let folder = relative_file(root, &format!(".scirust-verify/runs/{run}"))?;
    let bundle_bytes = read_bounded(&relative_file(&folder, "bundle.json")?, JSON_LIMIT)?;
    let bundle_digest = sha256(&bundle_bytes)?;
    if auth["bundle_sha256"] != bundle_digest {
        return Err("authenticated bundle changed".into());
    }
    let bundle: serde_json::Value = serde_json::from_slice(&bundle_bytes)?;
    if bundle["schema_version"] != 1 || bundle["algorithm"] != "sha256" {
        return Err("unsupported owner bundle".into());
    }
    let bytes = read_bounded(&relative_file(&folder, record_name)?, JSON_LIMIT)?;
    if bundle["files"][record_name] != sha256(&bytes)? {
        return Err("sealed evidence record changed".into());
    }
    Ok(AuthenticatedRecord {
        principal: principal.into(),
        bundle_sha256: bundle_digest,
        bytes,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    schema_version: u32,
    code_sha: String,
    publication_id: String,
    publishers: BTreeSet<String>,
    verifier_sha256: String,
    allowed_signers_sha256: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WeightPublicationManifest {
    pub schema_version: u32,
    pub repository: String,
    pub code_sha: String,
    pub publication_id: String,
    pub publisher: String,
    pub weights: Vec<PublishedWeight>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedWeight {
    /// Relative artifact path below the dossier's weights/ directory.
    pub path: String,
    pub sha256: String,
}

#[derive(Serialize)]
pub struct AdmissionReceipt {
    pub repository: String,
    pub code_sha: String,
    pub publication_id: String,
    pub publisher: String,
    pub run_id: String,
    pub bundle_sha256: String,
    pub manifest_sha256: String,
    pub artifacts: usize,
    pub artifact_bytes: usize,
    pub scientific_qualification: &'static str,
    pub merge_authorization: bool,
}

/// Only live owner authentication and independently configured publication ACL
/// can create this object. Its bytes stay immutable after admission.
pub struct AdmittedWeights {
    catalog: WeightArtifactCatalog,
    receipt: AdmissionReceipt,
}

impl AdmittedWeights {
    pub fn receipt(&self) -> &AdmissionReceipt {
        &self.receipt
    }

    pub fn stage(
        &self,
        runtime: &ElasticRamRuntime,
        plan: &ElasticWeightPlan,
        budget: RamMaterializationBudget,
    ) -> Result<MaterializedRamStage> {
        // Existing RAM pages might belong to a different publication. Admit
        // external weights only into a cold runtime, never silently reuse them.
        if runtime.ram_page_count() != 0 {
            return Err("external admission requires a cold runtime".into());
        }
        Ok(runtime.stage(plan, &self.catalog, budget)?)
    }
}

pub fn sha256(bytes: &[u8]) -> Result<String> {
    Ok(sha256_raw_bytes(bytes)?.hex())
}

fn hex(text: &str, length: usize) -> bool {
    text.len() == length
        && text
            .bytes()
            .all(|x| x.is_ascii_digit() || (b'a'..=b'f').contains(&x))
}

fn identifier(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 128
        && text
            .bytes()
            .all(|x| x.is_ascii_alphanumeric() || b"-_.@".contains(&x))
}

fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() > limit as u64 {
        return Err("regular file exceeded admission bound or was a symlink".into());
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err("file grew beyond admission bound".into());
    }
    Ok(bytes)
}

fn relative_file(root: &Path, name: &str) -> Result<PathBuf> {
    if name.is_empty() || name.len() > 256 || name.contains('\\') {
        return Err("invalid publication path".into());
    }
    let mut path = root.to_owned();
    for component in Path::new(name).components() {
        let Component::Normal(value) = component else {
            return Err("publication path must be relative without traversal".into());
        };
        path.push(value);
        if fs::symlink_metadata(&path)?.file_type().is_symlink() {
            return Err("symlink in publication path".into());
        }
    }
    if !path.canonicalize()?.starts_with(root) {
        return Err("publication path escaped its root".into());
    }
    Ok(path)
}

fn independent(path: &Path, root: &Path) -> Result<PathBuf> {
    let canonical = path.canonicalize()?;
    if canonical.starts_with(root) {
        return Err("trust policy/verifier must be configured outside the producer dossier".into());
    }
    Ok(canonical)
}

struct TrustSnapshot(PathBuf);

impl TrustSnapshot {
    fn new(bytes: &[u8]) -> Result<Self> {
        for _ in 0..128 {
            let folder = std::env::temp_dir().join(format!(
                "sml-publication-trust-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&folder) {
                Ok(()) => {
                    let snapshot = Self(folder);
                    let mut file = fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(snapshot.path())?;
                    file.write_all(bytes)?;
                    return Ok(snapshot);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err("cannot reserve independent trust snapshot".into())
    }

    fn path(&self) -> PathBuf {
        self.0.join("allowed_signers")
    }
}

impl Drop for TrustSnapshot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn capture(mut pipe: impl Read) -> std::io::Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        let n = pipe.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        let keep = n.min((JSON_LIMIT + 1).saturating_sub(output.len()));
        output.extend_from_slice(&buffer[..keep]);
    }
    Ok(output)
}

fn authenticate(owner: &Path, root: &Path, run: &str, trust: &Path) -> Result<serde_json::Value> {
    let envelope = relative_file(root, &format!(".scirust-verify/attestations/{run}.json"))?;
    let mut child = Command::new(owner)
        .current_dir(root)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .arg("authenticate")
        .arg(run)
        .arg("--envelope")
        .arg(envelope)
        .arg("--allowed-signers")
        .arg(trust)
        .arg("--json")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().ok_or("owner stdout unavailable")?;
    let stderr = child.stderr.take().ok_or("owner stderr unavailable")?;
    let out = thread::spawn(move || capture(stdout));
    let err = thread::spawn(move || capture(stderr));
    let start = Instant::now();
    let (status, timed_out) = loop {
        if let Some(status) = child.try_wait()? {
            break (status, false);
        }
        if start.elapsed() >= Duration::from_secs(30) {
            child.kill()?;
            break (child.wait()?, true);
        }
        thread::sleep(Duration::from_millis(10));
    };
    let output = out.join().map_err(|_| "owner stdout reader failed")??;
    let errors = err.join().map_err(|_| "owner stderr reader failed")??;
    if timed_out || !status.success() || output.len() > JSON_LIMIT || errors.len() > JSON_LIMIT {
        return Err("live owner authentication refused or exceeded process bounds".into());
    }
    Ok(serde_json::from_slice(&output)?)
}

/// The caller supplies trusted operator paths; no path, principal, code identity
/// or success flag in the producer payload selects the publication authority.
pub fn admit_external_weights(
    dossier: &Path,
    run: &str,
    owner: &Path,
    policy_file: &Path,
    allowed_signers: &Path,
) -> Result<AdmittedWeights> {
    if !identifier(run) || !run.starts_with("run-") {
        return Err("invalid owner run id".into());
    }
    let root = dossier.canonicalize()?;
    let owner = independent(owner, &root)?;
    let policy_file = independent(policy_file, &root)?;
    let allowed_signers = independent(allowed_signers, &root)?;
    let policy: Policy = serde_json::from_slice(&read_bounded(&policy_file, JSON_LIMIT)?)?;
    if policy.schema_version != 1
        || !hex(&policy.code_sha, 40)
        || !identifier(&policy.publication_id)
        || policy.publishers.is_empty()
        || policy.publishers.len() > 64
        || policy.publishers.iter().any(|x| !identifier(x))
        || !hex(&policy.verifier_sha256, 64)
        || !hex(&policy.allowed_signers_sha256, 64)
    {
        return Err("invalid independent publication policy".into());
    }
    let record = read_authenticated_record(
        &root,
        run,
        &owner,
        &allowed_signers,
        RecordTrust {
            principals: &policy.publishers,
            verifier_sha256: &policy.verifier_sha256,
            allowed_signers_sha256: &policy.allowed_signers_sha256,
        },
        RECORD,
    )?;
    let principal = record.principal;
    let bundle_digest = record.bundle_sha256;
    let manifest_bytes = record.bytes;
    let manifest_digest = sha256(&manifest_bytes)?;
    let manifest: WeightPublicationManifest = serde_json::from_slice(&manifest_bytes)?;
    if manifest.schema_version != 1
        || manifest.repository != REPOSITORY
        || manifest.code_sha != policy.code_sha
        || manifest.publication_id != policy.publication_id
        || manifest.publisher != principal
        || manifest.weights.is_empty()
        || manifest.weights.len() > 64
    {
        return Err("publication identity/code/ACL binding mismatch".into());
    }
    let weights_root = relative_file(&root, "weights")?;
    let mut paths = BTreeSet::new();
    let mut artifacts = Vec::new();
    let mut total = 0;
    for weight in &manifest.weights {
        if !hex(&weight.sha256, 64) || !paths.insert(&weight.path) {
            return Err("duplicate weight or invalid digest".into());
        }
        let bytes = read_bounded(
            &relative_file(&weights_root, &weight.path)?,
            WEIGHT_LIMIT - total,
        )?;
        if sha256(&bytes)? != weight.sha256 {
            return Err("external weight content mismatch".into());
        }
        total += bytes.len();
        artifacts.push(bytes);
    }
    let catalog = WeightArtifactCatalog::from_retained_bytes(artifacts)?;
    Ok(AdmittedWeights {
        catalog,
        receipt: AdmissionReceipt {
            repository: manifest.repository,
            code_sha: manifest.code_sha,
            publication_id: manifest.publication_id,
            publisher: manifest.publisher,
            run_id: run.into(),
            bundle_sha256: bundle_digest,
            manifest_sha256: manifest_digest,
            artifacts: manifest.weights.len(),
            artifact_bytes: total,
            scientific_qualification: "not_evaluated",
            merge_authorization: false,
        },
    })
}
