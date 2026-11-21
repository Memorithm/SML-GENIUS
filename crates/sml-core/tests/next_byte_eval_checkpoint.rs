//! `sml-next-byte-eval --checkpoint`: the Q4 arm read through the fail-closed
//! `Q4Checkpoint` loader (#1016). Synthetic fixtures only; no number here is a
//! model measurement.
use sml_core::{
    byte_factorization::ByteFactorization,
    elastic_ngram_paged_sha::{hex, sha256},
    q4_checkpoint::{CheckpointSchema, STREAM_METRICS_HEADER, STREAM_SCHEMA},
    sparse_gradient_atlas::{QuantizedSparseAtlas, SparseConfig, SparseGradientAtlas},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static SERIAL: AtomicU64 = AtomicU64::new(0);

const CONFIG: SparseConfig = SparseConfig {
    pages: 8,
    max_cached_pages: 2,
    learning_rate: 0.15,
};

fn text(seed: u8, len: usize) -> Vec<u8> {
    let words: [&[u8]; 6] = [b"the ", b"cat ", b"sat ", b"on ", b"a ", b"mat. "];
    let mut out = Vec::with_capacity(len);
    let mut state = u32::from(seed).wrapping_mul(2_654_435_761).wrapping_add(1);
    while out.len() < len {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        out.extend_from_slice(words[(state % 6) as usize]);
    }
    out.truncate(len);
    out
}

fn sha_file(path: &Path) -> String {
    hex(&sha256(&fs::read(path).unwrap()))
}

struct Fixture {
    dir: PathBuf,
    data: PathBuf,
    manifest: PathBuf,
    ckpt: PathBuf,
    dev_q4_bpc: String,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.dir).ok();
    }
}

/// Manifest + data, and a stream-fit-shaped output trained on the TRAIN rows
/// whose `metrics.tsv` records the grouped DEV Q4 value of this exact roster.
fn fixture() -> Fixture {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "sml-nbe-ckpt-{}-{nanos}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let data = dir.join("data");
    fs::create_dir_all(&data).unwrap();
    let train = [text(1, 600), text(2, 600)];
    let dev = [text(3, 300), text(4, 150)];
    let mut body = String::from("role\tpath\tbytes\tsha256\trights_review\n");
    for (role, name, bytes) in [
        ("TRAIN", "t1.txt", &train[0]),
        ("TRAIN", "t2.txt", &train[1]),
        ("DEV", "d1.txt", &dev[0]),
        ("DEV", "d2.txt", &dev[1]),
    ] {
        fs::write(data.join(name), bytes).unwrap();
        body.push_str(&format!(
            "{role}\t{name}\t{}\t{}\tOWNED_REVIEWED\n",
            bytes.len(),
            hex(&sha256(bytes))
        ));
    }
    let manifest = dir.join("manifest.tsv");
    fs::write(&manifest, body).unwrap();

    let ckpt = dir.join("model");
    let factorization = ByteFactorization::MsbTree;
    let mut atlas =
        SparseGradientAtlas::create_with_factorization(&ckpt, CONFIG, factorization).unwrap();
    for doc in &train {
        atlas.train_document_grouped(doc, 4096).unwrap();
    }
    atlas.checkpoint(&ckpt).unwrap();
    let q4 = ckpt.join("weights.q4");
    atlas.export_q4(&q4).unwrap();
    let mut quantized =
        QuantizedSparseAtlas::open_with_factorization(&q4, CONFIG, factorization).unwrap();
    let (mut f32_loss, mut q4_loss, mut events) = (0f64, 0f64, 0u64);
    for doc in &dev {
        let (l, e) = atlas.evaluate_document_grouped(doc, 4096).unwrap();
        f32_loss += l;
        events += e;
        q4_loss += quantized.evaluate_document_grouped(doc, 4096).unwrap().0;
    }
    let (f32_bpc, q4_bpc) = (f32_loss / events as f64, q4_loss / events as f64);
    let dev_q4_bpc = format!("{q4_bpc:.9}");
    let counters = atlas.counters();
    let row = format!(
        "{STREAM_SCHEMA}\t{}\t{}\t{}\t{}\t2\t1200\t1196\t1.000000000\t2\t450\t{events}\t{f32_bpc:.9}\t{dev_q4_bpc}\t{:.9}\t0.001\t1.000\t{}\t{}\tmsb_tree\t{}\t{}\tUNOPENED\tNOT_PROMOTED",
        sha_file(&manifest),
        CONFIG.total_parameters(),
        counters.distinct_updated_parameters,
        counters.parameter_updates,
        q4_bpc - f32_bpc,
        CONFIG.train_master_bytes(),
        CONFIG.packed_q4_bytes(),
        sha_file(&q4),
        sha_file(&ckpt.join("snapshot.tsv")),
    );
    fs::write(
        ckpt.join("metrics.tsv"),
        format!("{STREAM_METRICS_HEADER}\n{row}\n"),
    )
    .unwrap();
    seal(&ckpt);
    Fixture {
        dir,
        data,
        manifest,
        ckpt,
        dev_q4_bpc,
    }
}

fn seal(ckpt: &Path) {
    fs::write(
        ckpt.join("COMPLETE"),
        format!(
            "{}\t{}\n",
            CheckpointSchema::Stream.as_str(),
            sha_file(&ckpt.join("metrics.tsv"))
        ),
    )
    .unwrap();
}

fn cli(fx: &Fixture, mode: &str, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sml-next-byte-eval"))
        .args([
            "--mode",
            mode,
            "--data-root",
            fx.data.to_str().unwrap(),
            "--manifest",
            fx.manifest.to_str().unwrap(),
        ])
        .args(extra)
        .output()
        .unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8(out.stdout.clone()).unwrap()
}

#[test]
fn checkpoint_arm_gates_on_its_recorded_dev_value_and_reports_its_digests() {
    let fx = fixture();
    let ckpt = fx.ckpt.to_str().unwrap();
    let ok = cli(&fx, "synthetic", &["--checkpoint", ckpt]);
    assert!(
        ok.status.success(),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
    let out = stdout(&ok);
    assert!(out.contains("mode=SYNTHETIC_UNPINNED"));
    let line = out
        .lines()
        .find(|l| l.starts_with("CHECKPOINT_V1 "))
        .unwrap();
    assert!(line.contains(&format!(
        "metrics_sha256={}",
        sha_file(&fx.ckpt.join("metrics.tsv"))
    )));
    assert!(line.contains(&format!(
        "q4_sha256={}",
        sha_file(&fx.ckpt.join("weights.q4"))
    )));
    assert!(line.contains("pages=8 cache_pages=2 factorization=MsbTree"));
    assert!(out.contains(&format!(
        "REPRODUCTION_GATE measured_dev_q4_bpc={} expected={} status=PASS",
        fx.dev_q4_bpc, fx.dev_q4_bpc
    )));
    for arm in [
        "q4_atlas_s2a",
        "order2_dirichlet_backoff_train",
        "unigram_add1_train",
    ] {
        let row = out.lines().find(|l| l.starts_with(arm)).unwrap();
        assert_eq!(row.split('\t').nth(2), Some("446"));
    }
    // The --checkpoint path and the raw synthetic --q4 path score identically.
    let raw = cli(
        &fx,
        "synthetic",
        &[
            "--q4",
            fx.ckpt.join("weights.q4").to_str().unwrap(),
            "--pages",
            "8",
            "--expected-q4-bpc",
            &fx.dev_q4_bpc,
        ],
    );
    assert!(raw.status.success());
    let row = |s: &str| {
        s.lines()
            .find(|l| l.starts_with("q4_atlas_s2a\t"))
            .unwrap()
            .split('\t')
            .take(4)
            .collect::<Vec<_>>()
            .join("\t")
    };
    assert_eq!(row(&out), row(&stdout(&raw)));
    // An explicit wrong expectation still fails the gate closed.
    let bad = cli(
        &fx,
        "synthetic",
        &["--checkpoint", ckpt, "--expected-q4-bpc", "0.000000001"],
    );
    assert!(!bad.status.success());
    assert!(!stdout(&bad).contains("q4_atlas_s2a\t"));
}

#[test]
fn checkpoint_flag_combinations_and_pinned_mode_fail_closed() {
    let fx = fixture();
    let ckpt = fx.ckpt.to_str().unwrap();
    let q4 = fx.ckpt.join("weights.q4");
    let q4 = q4.to_str().unwrap();
    for (mode, extra) in [
        // Fixture is not the pre-registered S2a checkpoint.
        ("pinned", vec!["--checkpoint", ckpt]),
        // Pinned mode never accepts a headerless Q4 file.
        ("pinned", vec!["--q4", q4]),
        ("synthetic", vec!["--checkpoint", ckpt, "--q4", q4]),
        ("synthetic", vec!["--checkpoint", ckpt, "--pages", "8"]),
        (
            "synthetic",
            vec!["--checkpoint", ckpt, "--factorization", "bits"],
        ),
        ("synthetic", vec!["--arms", "unigram", "--checkpoint", ckpt]),
    ] {
        let out = cli(&fx, mode, &extra);
        assert!(!out.status.success(), "{mode} {extra:?} must be refused");
        assert!(!stdout(&out).contains("q4_atlas_s2a\t"));
    }
}

#[test]
fn checkpoint_from_another_manifest_or_roster_or_unsealed_is_refused() {
    let fx = fixture();
    let ckpt = fx.ckpt.to_str().unwrap();
    let metrics = fs::read_to_string(fx.ckpt.join("metrics.tsv")).unwrap();
    let manifest_sha = sha_file(&fx.manifest);
    // Unsealed edit: COMPLETE no longer matches metrics.tsv.
    fs::write(
        fx.ckpt.join("metrics.tsv"),
        metrics.replace("\t2\t450\t", "\t1\t450\t"),
    )
    .unwrap();
    assert!(!cli(&fx, "synthetic", &["--checkpoint", ckpt])
        .status
        .success());
    // Resealed but scored on a different manifest or DEV roster.
    for edited in [
        metrics.replace(&manifest_sha, &"e".repeat(64)),
        metrics.replace("\t2\t450\t", "\t1\t450\t"),
    ] {
        fs::write(fx.ckpt.join("metrics.tsv"), &edited).unwrap();
        seal(&fx.ckpt);
        let out = cli(&fx, "synthetic", &["--checkpoint", ckpt]);
        assert!(!out.status.success());
        assert!(!stdout(&out).contains("REPRODUCTION_GATE"));
    }
    // Restored and resealed: accepted again.
    fs::write(fx.ckpt.join("metrics.tsv"), &metrics).unwrap();
    seal(&fx.ckpt);
    assert!(cli(&fx, "synthetic", &["--checkpoint", ckpt])
        .status
        .success());
    // Q4 altered after sealing.
    let q4 = fx.ckpt.join("weights.q4");
    let mut bytes = fs::read(&q4).unwrap();
    bytes[0] ^= 0x11;
    fs::write(&q4, bytes).unwrap();
    assert!(!cli(&fx, "synthetic", &["--checkpoint", ckpt])
        .status
        .success());
}
