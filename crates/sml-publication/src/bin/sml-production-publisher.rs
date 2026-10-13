use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::Value;
use sml_core::elastic_weights::WeightPrecision;
use sml_core::PageId;
use sml_publication::{sha256, PublishedWeight, WeightPublicationManifest};
use sml_runtime::build_weight_artifact;

const PRINCIPAL: &str = "memorithm-sml-production";

fn owner(executable: &Path, root: &Path, args: &[&str]) -> Result<Value, String> {
    let output = Command::new(executable)
        .args(args)
        .current_dir(root)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .output()
        .map_err(|error| format!("failed to start authentication owner: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "authentication owner refused operation: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("invalid owner response: {error}"))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 5 {
        return Err(
            "usage: sml-production-publisher DOSSIER OWNER KEY CODE_SHA PUBLICATION_ID".into(),
        );
    }
    let root = Path::new(&args[0]);
    let owner_binary = Path::new(&args[1]).canonicalize()?;
    let identity = Path::new(&args[2]).canonicalize()?;
    let code_sha = &args[3];
    let publication_id = &args[4];
    if code_sha.len() != 40
        || !code_sha.chars().all(|value| value.is_ascii_hexdigit())
        || publication_id.is_empty()
        || publication_id.len() > 128
        || !publication_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || b"-_.@".contains(&value))
    {
        return Err("invalid production publication identity".into());
    }

    fs::create_dir_all(root.join("weights"))?;
    let bytes = build_weight_artifact(PageId(0), 0, WeightPrecision::Boolean, 8, &[0xa5])?;
    fs::write(root.join("weights/page0.smlw"), &bytes)?;
    let manifest = WeightPublicationManifest {
        schema_version: 1,
        repository: "Memorithm/SML-GENIUS".to_owned(),
        code_sha: code_sha.to_owned(),
        publication_id: publication_id.to_owned(),
        publisher: PRINCIPAL.to_owned(),
        weights: vec![PublishedWeight {
            path: "page0.smlw".to_owned(),
            sha256: sha256(&bytes)?,
        }],
    };
    fs::write(
        root.join("publication.json"),
        serde_json::to_vec(&manifest)?,
    )?;
    fs::write(
        root.join("scirust-verify.toml"),
        r#"schema_version = 1
[artifact]
name = "SML production weight publication"
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
    )?;

    let report = owner(
        &owner_binary,
        root,
        &["verify", root.to_str().ok_or("invalid dossier path")?, "--json"],
    )?;
    if report["overall_verdict"] != "PASS" {
        return Err("production dossier verification did not pass".into());
    }
    let run = report["run_id"]
        .as_str()
        .ok_or("owner omitted run id")?
        .to_owned();
    owner(
        &owner_binary,
        root,
        &[
            "attest",
            &run,
            "--principal",
            PRINCIPAL,
            "--identity",
            identity.to_str().ok_or("invalid identity path")?,
            "--json",
        ],
    )?;
    println!("{run}");
    Ok(())
}
