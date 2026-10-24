//! HLE-P1 on the S5h manifest (`docs/HLE-P1-S5H-PREREGISTRATION.md`).
//!
//! Streams documents one at a time from a `role\tpath\tbytes\tsha256\trights_review`
//! manifest (TRAIN rows, then DEV rows; TEST is never admitted), checking
//! length, SHA-256 and UTF-8 of every document as it is read. Each arm runs
//! on its own thread: one prequential pass over a TRAIN prefix, then a frozen
//! DEV pass. Research only, NOT_PROMOTED.
//!
//! ```text
//! sml-hle-p1-s5h run <data_root> <manifest> <out_dir> <train_bytes|full> <seeds> <threads> [expected_manifest_sha]
//! sml-hle-p1-s5h proxy <docs_dir> <out_dir>
//! ```

use sml_core::{
    elastic_ngram_paged_sha::{hex, parse_hex, sha256},
    hle_p1::{preregistered_arms, ArmSpec, Model, Tally, Q4_BUDGET_BYTES},
};
use std::{
    collections::HashSet,
    error::Error,
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
    time::Instant,
};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
const HEADER: &str = "role\tpath\tbytes\tsha256\trights_review";
const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;
const MAX_FILES: usize = 100_000;
const MIN_DOCUMENT_BYTES: u64 = 128;
const MAX_DOCUMENT_BYTES: u64 = 2 * 1024 * 1024;
const PROGRESS_BYTES: u64 = 64 << 20;
const RIGHTS: [&str; 7] = [
    "OWNED_REVIEWED",
    "PUBLIC_DOMAIN_REVIEWED",
    "CC0_REVIEWED",
    "CC_BY_ATTRIBUTION_REVIEWED",
    "FR_OPEN_LICENSE_V2_ATTRIBUTION_REVIEWED",
    "APACHE_2_0_ATTRIBUTION_REVIEWED",
    "MIT_ATTRIBUTION_REVIEWED",
];
const COLUMNS: &str = "schema\tmanifest_sha256\tarm\tseed\tstate_bytes\ttrain_files\ttrain_bytes\ttrain_events\ttrain_prequential_bpc\tdev_files\tdev_bytes\tdev_events\tdev_bpc\telapsed_s\ttest\tpublication";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Train,
    Dev,
}

struct Document {
    index: usize,
    role: Role,
    path: PathBuf,
    length: u64,
    digest: [u8; 32],
}

struct Roster {
    docs: Vec<Document>,
    manifest_sha: String,
}

fn checked_path(root: &Path, relative: &str) -> Result<PathBuf> {
    let rel = Path::new(relative);
    if rel.is_absolute() || rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err("manifest path must be relative and normal".into());
    }
    let path = fs::canonicalize(root.join(rel))?;
    if !path.starts_with(root) {
        return Err("manifest source escaped the data directory".into());
    }
    Ok(path)
}

/// Same admission rules as the GB-11 P1 S5h reader.
fn load_roster(root: &Path, manifest: &Path) -> Result<Roster> {
    if fs::symlink_metadata(root)?.file_type().is_symlink() || !root.is_dir() {
        return Err("data root must be a directory, not a symlink".into());
    }
    let root = fs::canonicalize(root)?;
    let info = fs::symlink_metadata(manifest)?;
    if !info.is_file() || info.len() == 0 || info.len() > MAX_MANIFEST_BYTES {
        return Err("manifest must be a bounded regular file".into());
    }
    let contents = fs::read(manifest)?;
    let text = std::str::from_utf8(&contents)?;
    let mut lines = text.lines();
    if lines.next() != Some(HEADER) {
        return Err("invalid document manifest header".into());
    }
    let mut docs = Vec::new();
    let mut seen_paths = HashSet::new();
    let mut seen_digests = HashSet::new();
    let mut dev_started = false;
    for line in lines {
        if docs.len() == MAX_FILES {
            return Err("too many documents".into());
        }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() != 5 {
            return Err("invalid manifest record".into());
        }
        let role = match f[0] {
            "TRAIN" if !dev_started => Role::Train,
            "DEV" => {
                dev_started = true;
                Role::Dev
            }
            _ => return Err("invalid document role/order (TEST is never admitted)".into()),
        };
        if !RIGHTS.contains(&f[4]) {
            return Err("document rights have not been declared reviewed".into());
        }
        let length: u64 = f[2].parse()?;
        if !(MIN_DOCUMENT_BYTES..=MAX_DOCUMENT_BYTES).contains(&length) {
            return Err("document length outside budget".into());
        }
        let digest = parse_hex(f[3]).ok_or("invalid SHA256 field")?;
        if f[3] != hex(&digest) {
            return Err("noncanonical SHA256 field".into());
        }
        let path = checked_path(&root, f[1])?;
        if !seen_digests.insert(digest) || !seen_paths.insert(path.clone()) {
            return Err("duplicate document path or digest".into());
        }
        docs.push(Document {
            index: docs.len(),
            role,
            path,
            length,
            digest,
        });
    }
    if !dev_started || docs.first().is_none_or(|d| d.role != Role::Train) {
        return Err("manifest needs TRAIN then DEV rows".into());
    }
    Ok(Roster {
        docs,
        manifest_sha: hex(&sha256(&contents)),
    })
}

/// Proxy roster: every `*.md` directly in `dir`, sorted; every 8th is DEV.
fn proxy_roster(dir: &Path) -> Result<Roster> {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "md") && p.is_file())
        .collect();
    paths.sort();
    let mut train = Vec::new();
    let mut dev = Vec::new();
    for (i, path) in paths.into_iter().enumerate() {
        let data = fs::read(&path)?;
        if data.len() < MIN_DOCUMENT_BYTES as usize || std::str::from_utf8(&data).is_err() {
            continue;
        }
        let (role, list) = if i % 8 == 7 {
            (Role::Dev, &mut dev)
        } else {
            (Role::Train, &mut train)
        };
        list.push(Document {
            index: 0,
            role,
            path,
            length: data.len() as u64,
            digest: sha256(&data),
        });
    }
    if train.is_empty() || dev.is_empty() {
        return Err("proxy directory needs at least 8 markdown documents".into());
    }
    let mut docs = train;
    docs.extend(dev);
    let mut listing = String::new();
    for (i, d) in docs.iter_mut().enumerate() {
        d.index = i;
        listing.push_str(&format!("{}\t{}\n", d.length, hex(&d.digest)));
    }
    Ok(Roster {
        docs,
        manifest_sha: hex(&sha256(listing.as_bytes())),
    })
}

fn load(doc: &Document) -> Result<Vec<u8>> {
    let mut data = Vec::with_capacity(doc.length as usize);
    File::open(&doc.path)?.read_to_end(&mut data)?;
    if data.len() as u64 != doc.length || sha256(&data) != doc.digest {
        return Err(format!("document {} changed or is invalid", doc.index).into());
    }
    std::str::from_utf8(&data)?;
    Ok(data)
}

fn run_arm(roster: &Roster, spec: ArmSpec, train_cap: u64) -> Result<String> {
    let start = Instant::now();
    let (name, seed) = (spec.name.clone(), spec.seed);
    let mut model = Model::new(spec);
    let mut train = Tally::default();
    let mut next_report = PROGRESS_BYTES;
    for doc in roster.docs.iter().filter(|d| d.role == Role::Train) {
        if train.bytes >= train_cap {
            break;
        }
        model.code_document(&load(doc)?, &mut train);
        if train.bytes >= next_report {
            next_report += PROGRESS_BYTES;
            eprintln!(
                "SML_HLEP1_PROGRESS arm={name} train_bytes={} train_bpc={:.4} elapsed_s={:.0}",
                train.bytes,
                train.bpc(),
                start.elapsed().as_secs_f64()
            );
        }
        if !train.bpc().is_finite() && train.events > 0 {
            return Err(format!("arm {name}: non-finite TRAIN cost (stop rule S1)").into());
        }
    }
    model.frozen = true;
    let mut dev = Tally::default();
    for doc in roster.docs.iter().filter(|d| d.role == Role::Dev) {
        model.code_document(&load(doc)?, &mut dev);
    }
    Ok(format!(
        "SML_HLEP1_V1\t{}\t{name}\t{seed}\t{}\t{}\t{}\t{}\t{:.9}\t{}\t{}\t{}\t{:.9}\t{:.1}\tUNOPENED\tNOT_PROMOTED",
        roster.manifest_sha,
        model.state_bytes(),
        train.files,
        train.bytes,
        train.events,
        train.bpc(),
        dev.files,
        dev.bytes,
        dev.events,
        dev.bpc(),
        start.elapsed().as_secs_f64()
    ))
}

fn run_all(
    roster: &Roster,
    arms: Vec<ArmSpec>,
    train_cap: u64,
    threads: usize,
    out: &Path,
) -> Result<Vec<String>> {
    fs::create_dir_all(out)?;
    let tsv = Mutex::new(File::create(out.join("hle-p1-results.tsv"))?);
    writeln!(tsv.lock().expect("lock"), "{COLUMNS}")?;
    println!("{COLUMNS}");
    // Circuit arms are the slowest: start them first.
    let mut arms = arms;
    arms.sort_by_key(|a| a.circuit.is_none());
    let next = AtomicUsize::new(0);
    let rows = Mutex::new(Vec::new());
    let failed = Mutex::new(None);
    std::thread::scope(|s| {
        for _ in 0..threads.max(1) {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some(spec) = arms.get(i).cloned() else {
                    break;
                };
                match run_arm(roster, spec, train_cap) {
                    Ok(row) => {
                        println!("{row}");
                        let mut f = tsv.lock().expect("lock");
                        let _ = writeln!(f, "{row}");
                        let _ = f.flush();
                        rows.lock().expect("lock").push(row);
                    }
                    Err(e) => {
                        eprintln!("SML_HLEP1_ARM_FAILED {e}");
                        *failed.lock().expect("lock") = Some(e.to_string());
                    }
                }
            });
        }
    });
    if let Some(e) = failed.into_inner().expect("lock") {
        return Err(e.into());
    }
    Ok(rows.into_inner().expect("lock"))
}

fn cli(args: &[String]) -> Result<()> {
    let start = Instant::now();
    match args {
        [_, mode, root, manifest, out, cap, seeds, threads, rest @ ..]
            if mode == "run" && rest.len() <= 1 =>
        {
            let roster = load_roster(Path::new(root), Path::new(manifest))?;
            if let Some(expected) = rest.first() {
                if *expected != roster.manifest_sha {
                    return Err(format!(
                        "manifest SHA256 {} differs from expected",
                        roster.manifest_sha
                    )
                    .into());
                }
            }
            let cap = if cap == "full" {
                u64::MAX
            } else {
                cap.parse()?
            };
            let seeds: Vec<u64> = seeds
                .split(',')
                .map(str::parse)
                .collect::<std::result::Result<_, _>>()?;
            let arms = preregistered_arms(&seeds, Q4_BUDGET_BYTES);
            let rows = run_all(&roster, arms, cap, threads.parse()?, Path::new(out))?;
            println!(
                "SML_HLEP1_DONE manifest={} train_cap={cap} arms={} total_s={:.1} test=UNOPENED model_promoted=false",
                roster.manifest_sha,
                rows.len(),
                start.elapsed().as_secs_f64()
            );
            Ok(())
        }
        [_, mode, docs, out] if mode == "proxy" => {
            let roster = proxy_roster(Path::new(docs))?;
            let arms = preregistered_arms(&[1], Q4_BUDGET_BYTES);
            let rows = run_all(&roster, arms, u64::MAX, 4, Path::new(out))?;
            println!(
                "SML_HLEP1_PROXY_DONE arms={} total_s={:.1} corpus=docs_proxy test=UNOPENED",
                rows.len(),
                start.elapsed().as_secs_f64()
            );
            Ok(())
        }
        _ => Err("usage: sml-hle-p1-s5h run <data_root> <manifest> <out_dir> <train_bytes|full> <seeds> <threads> [expected_manifest_sha] | proxy <docs_dir> <out_dir>".into()),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Err(e) = cli(&args) {
        eprintln!("SML_HLEP1_FAILED {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("hlep1-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn doc_text(i: usize) -> String {
        format!("Document {i}. ").repeat(20) + &"the cat sat on the mat. ".repeat(i % 5 + 3)
    }

    #[test]
    fn manifest_run_streams_and_rejects_test_rows() {
        let root = tmp("data");
        let mut manifest = format!("{HEADER}\n");
        for i in 0..10 {
            let body = doc_text(i);
            fs::write(root.join(format!("d{i}.txt")), &body).unwrap();
            let role = if i < 8 { "TRAIN" } else { "DEV" };
            manifest.push_str(&format!(
                "{role}\td{i}.txt\t{}\t{}\tOWNED_REVIEWED\n",
                body.len(),
                hex(&sha256(body.as_bytes()))
            ));
        }
        let mpath = root.join("manifest.tsv");
        fs::write(&mpath, &manifest).unwrap();
        let roster = load_roster(&root, &mpath).unwrap();
        let arms: Vec<_> = preregistered_arms(&[1], 8 << 20)
            .into_iter()
            .filter(|a| a.name == "order2" || a.name.starts_with("recurrent_tbptt4"))
            .collect();
        let rows = run_all(&roster, arms, 2000, 2, &root.join("out")).unwrap();
        assert_eq!(rows.len(), 2);
        for row in &rows {
            let f: Vec<&str> = row.split('\t').collect();
            assert_eq!(f[14], "UNOPENED");
            assert_eq!(f[9], "2");
            assert!(f[6].parse::<u64>().unwrap() >= 2000);
            assert!(f[12].parse::<f64>().unwrap() < 8.0);
        }
        let bad = manifest.replacen("DEV", "TEST", 1);
        fs::write(&mpath, bad).unwrap();
        assert!(load_roster(&root, &mpath).is_err());
        let _ = fs::remove_dir_all(&root);
    }
}
