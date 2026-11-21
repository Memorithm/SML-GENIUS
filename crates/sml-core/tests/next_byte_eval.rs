//! Synthetic-data tests for the pre-registered next-byte evaluation harness.
//! No number here is a model measurement: all inputs are generated fixtures.
use sml_core::{
    byte_factorization::ByteFactorization,
    elastic_ngram_paged_sha::{hex, sha256},
    next_byte_eval::{
        check_normalized, evaluate_documents, reproduces_exactly, FrozenManifest, ManifestRole,
        NextBytePredictor, Order2Counts, Q4AtlasPredictor, Result, UnigramCounts,
        FIRST_SCORED_POSITION,
    },
    sparse_gradient_atlas::{QuantizedSparseAtlas, SparseConfig, SparseGradientAtlas},
};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static SERIAL: AtomicU64 = AtomicU64::new(0);

fn temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "sml-next-byte-eval-{label}-{}-{nanos}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn text(seed: u8, len: usize) -> Vec<u8> {
    let alphabet = b"the quick brown fox jumps over a lazy dog. ";
    let mut state = u32::from(seed).wrapping_mul(2_654_435_761).wrapping_add(1);
    (0..len)
        .map(|_| {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            alphabet[(state >> 16) as usize % alphabet.len()]
        })
        .collect()
}

struct Uniform;
impl NextBytePredictor for Uniform {
    fn arm(&self) -> &str {
        "uniform"
    }
    fn begin_document(&mut self) -> Result<()> {
        Ok(())
    }
    fn log2_probability(&mut self, _h: &[u8], _n: u8) -> Result<f64> {
        Ok(-8.0)
    }
    fn top1(&mut self, _h: &[u8]) -> Result<u8> {
        Ok(0)
    }
}

/// Asserts the evaluator's call contract: history grows by one byte per
/// position, `top1` precedes `log2_probability`, reset per document.
#[derive(Default)]
struct ContractProbe {
    expected_len: usize,
    top1_pending: bool,
    documents: usize,
}
impl NextBytePredictor for ContractProbe {
    fn arm(&self) -> &str {
        "contract_probe"
    }
    fn begin_document(&mut self) -> Result<()> {
        self.expected_len = FIRST_SCORED_POSITION;
        self.top1_pending = false;
        self.documents += 1;
        Ok(())
    }
    fn top1(&mut self, history: &[u8]) -> Result<u8> {
        assert_eq!(history.len(), self.expected_len);
        assert!(!self.top1_pending);
        self.top1_pending = true;
        Ok(history[history.len() - 1])
    }
    fn log2_probability(&mut self, history: &[u8], _next: u8) -> Result<f64> {
        assert_eq!(history.len(), self.expected_len);
        assert!(self.top1_pending);
        self.top1_pending = false;
        self.expected_len += 1;
        Ok(-8.0)
    }
}

#[test]
fn uniform_predictor_scores_exactly_eight_bits_on_len_minus_two_positions() {
    let a = text(1, 100);
    let b = text(2, 2); // too short: contributes no position
    let c = text(3, 37);
    let report = evaluate_documents(&mut Uniform, &[&a, &b, &c], true).unwrap();
    assert_eq!(report.documents, 3);
    assert_eq!(report.scored_bytes, 98 + 35);
    assert_eq!(report.bits_per_byte(), Some(8.0));
    assert_eq!(report.page_loads, 0);
    let zeros = a[2..]
        .iter()
        .chain(&c[2..])
        .filter(|&&byte| byte == 0)
        .count() as u64;
    assert_eq!(report.top1_correct, zeros);
    assert_eq!(report.top1_stream_sha256, hex(&sha256(&[0u8; 133])));
    let no_top1 = evaluate_documents(&mut Uniform, &[&a], false).unwrap();
    assert_eq!(no_top1.top1_accuracy(), None);
    assert_eq!(no_top1.tsv_row().split('\t').count(), 11);
}

#[test]
fn evaluator_honours_the_documented_call_contract() {
    let mut probe = ContractProbe::default();
    let docs = [text(4, 50), text(5, 3), text(6, 9)];
    let refs = docs.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let report = evaluate_documents(&mut probe, &refs, true).unwrap();
    assert_eq!(probe.documents, 3);
    assert_eq!(report.scored_bytes, 48 + 1 + 7);
}

#[test]
fn unigram_is_add_one_normalized_and_frozen_after_evaluation_starts() {
    let mut model = UnigramCounts::default();
    model.fit_document(b"xxabca").unwrap(); // scored positions: a b c a
    assert_eq!(model.total(), 4);
    assert!((model.probability(b'a') - 3.0 / 260.0).abs() < 1e-15);
    assert!((model.probability(b'z') - 1.0 / 260.0).abs() < 1e-15);
    assert!((check_normalized(&mut model, b"..").unwrap() - 1.0).abs() < 1e-12);
    assert_eq!(model.top1(b"..").unwrap(), b'a');
    model.begin_document().unwrap();
    assert!(model.fit_document(b"leak").is_err());
}

#[test]
fn order2_backoff_is_normalized_in_seen_and_unseen_contexts() {
    let mut model = Order2Counts::default();
    for seed in 0..4 {
        model.fit_document(&text(seed, 400)).unwrap();
    }
    assert_eq!(model.fitted_events(), 4 * 398);
    for history in [&b"th"[..], b"e ", b"\x00\xff", b"zz"] {
        assert!((check_normalized(&mut model, history).unwrap() - 1.0).abs() < 1e-9);
    }
    // Deterministic corpus: after "ab" always comes "c".
    let mut tiny = Order2Counts::default();
    tiny.fit_document(b"abcabcabcabc").unwrap();
    assert_eq!(tiny.top1(b"ab").unwrap(), b'c');
    assert!(tiny.probability(b'a', b'b', b'c') > 0.5);
    tiny.begin_document().unwrap();
    assert!(tiny.fit_document(b"abd").is_err());
    assert!(tiny.log2_probability(b"a", b'c').is_err());
}

#[test]
fn ties_choose_the_lowest_byte() {
    let mut empty = UnigramCounts::default();
    empty.begin_document().unwrap();
    assert_eq!(empty.top1(b"..").unwrap(), 0);
    let mut empty2 = Order2Counts::default();
    empty2.begin_document().unwrap();
    assert_eq!(empty2.top1(b"qq").unwrap(), 0);
}

fn trained_q4(dir: &std::path::Path, factorization: ByteFactorization) -> (PathBuf, SparseConfig) {
    let config = SparseConfig {
        pages: 8,
        max_cached_pages: 2,
        learning_rate: 0.15,
    };
    let root = dir.join(format!("atlas-{factorization:?}"));
    let mut atlas =
        SparseGradientAtlas::create_with_factorization(&root, config, factorization).unwrap();
    for seed in 0..3 {
        atlas.train_document(&text(seed, 300)).unwrap();
    }
    let q4 = root.join("weights.q4");
    atlas.export_q4(&q4).unwrap();
    (q4, config)
}

#[test]
fn q4_trait_path_matches_the_atlas_scoring_path_on_both_factorizations() {
    let dir = temp_dir("q4");
    let docs = [text(10, 257), text(11, 64), text(12, 2)];
    for factorization in [
        ByteFactorization::IndependentBits,
        ByteFactorization::MsbTree,
    ] {
        let (q4, config) = trained_q4(&dir, factorization);
        let mut reference =
            QuantizedSparseAtlas::open_with_factorization(&q4, config, factorization).unwrap();
        let mut grouped =
            QuantizedSparseAtlas::open_with_factorization(&q4, config, factorization).unwrap();
        let (mut loss, mut events, mut grouped_loss) = (0.0, 0, 0.0);
        for doc in &docs {
            let (l, e) = reference.evaluate_document(doc).unwrap();
            loss += l;
            events += e;
            grouped_loss += grouped.evaluate_document_grouped(doc, 4096).unwrap().0;
        }
        let atlas =
            QuantizedSparseAtlas::open_with_factorization(&q4, config, factorization).unwrap();
        let mut predictor = Q4AtlasPredictor::new("q4_fixture", atlas, factorization);
        let refs = docs.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let report = evaluate_documents(&mut predictor, &refs, true).unwrap();
        assert_eq!(report.scored_bytes, events);
        assert!((report.total_loss_bits - loss).abs() < 1e-9 * loss.max(1.0));
        assert!((report.total_loss_bits - grouped_loss).abs() < 1e-9 * loss.max(1.0));
        assert!(report.page_loads > 0);
        // Top-1 equals the public MAP decoder at every scored position.
        let mut decoder =
            QuantizedSparseAtlas::open_with_factorization(&q4, config, factorization).unwrap();
        let mut correct = 0;
        for doc in &docs {
            for i in 2..doc.len() {
                let byte = decoder
                    .predict_next_byte(doc[i - 2], doc[i - 1])
                    .unwrap()
                    .byte;
                correct += u64::from(byte == doc[i]);
            }
        }
        assert_eq!(report.top1_correct, correct);
        assert!((check_normalized(&mut predictor, b"he").unwrap() - 1.0).abs() < 1e-9);
    }
    fs::remove_dir_all(dir).ok();
}

#[test]
fn reproduction_gate_compares_nine_decimal_rendering_exactly() {
    assert!(reproduces_exactly(4.007_063_135, "4.007063135"));
    assert!(reproduces_exactly(4.007_063_134_6, "4.007063135"));
    assert!(!reproduces_exactly(4.007_063_136, "4.007063135"));
    assert!(!reproduces_exactly(f64::NAN, "NaN"));
}

struct Fixture {
    dir: PathBuf,
    data: PathBuf,
    manifest: PathBuf,
}

impl Fixture {
    fn new(rows: &[(&str, &str, Vec<u8>)]) -> Self {
        let dir = temp_dir("manifest");
        let data = dir.join("data");
        fs::create_dir_all(&data).unwrap();
        let mut body = String::from("role\tpath\tbytes\tsha256\trights_review\n");
        for (role, name, bytes) in rows {
            fs::write(data.join(name), bytes).unwrap();
            body.push_str(&format!(
                "{role}\t{name}\t{}\t{}\tOWNED_REVIEWED\n",
                bytes.len(),
                hex(&sha256(bytes))
            ));
        }
        let manifest = dir.join("manifest.tsv");
        fs::write(&manifest, body).unwrap();
        Self {
            dir,
            data,
            manifest,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.dir).ok();
    }
}

#[test]
fn manifest_rejects_pin_mismatch_test_role_order_and_traversal() {
    let fx = Fixture::new(&[
        ("TRAIN", "t.txt", text(1, 200)),
        ("DEV", "d.txt", text(2, 100)),
    ]);
    let loaded = FrozenManifest::load(&fx.data, &fx.manifest, None).unwrap();
    assert_eq!(loaded.role(ManifestRole::Train).count(), 1);
    assert_eq!(loaded.role(ManifestRole::Dev).count(), 1);
    let pin = loaded.manifest_sha256.clone();
    assert!(FrozenManifest::load(&fx.data, &fx.manifest, Some(&pin)).is_ok());
    assert!(FrozenManifest::load(&fx.data, &fx.manifest, Some(&"0".repeat(64))).is_err());
    let original = fs::read_to_string(&fx.manifest).unwrap();
    for bad in [
        original.replace("DEV\t", "TEST\t"),
        original
            .replace("TRAIN\t", "XX\t")
            .replace("DEV\t", "TRAIN\t")
            .replace("XX\t", "DEV\t"),
        original.replace("t.txt", "../t.txt"),
        original.replace("OWNED_REVIEWED", "UNREVIEWED"),
    ] {
        fs::write(&fx.manifest, bad).unwrap();
        assert!(FrozenManifest::load(&fx.data, &fx.manifest, None).is_err());
    }
}

fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_sml-next-byte-eval"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn cli_synthetic_run_gate_pass_and_fail_closed() {
    let fx = Fixture::new(&[
        ("TRAIN", "t1.txt", text(1, 600)),
        ("TRAIN", "t2.txt", text(2, 600)),
        ("DEV", "d1.txt", text(3, 300)),
        ("DEV", "d2.txt", text(4, 150)),
    ]);
    let (q4, _) = trained_q4(&fx.dir, ByteFactorization::MsbTree);
    let (data, manifest, q4) = (
        fx.data.to_str().unwrap(),
        fx.manifest.to_str().unwrap(),
        q4.to_str().unwrap(),
    );
    let base = [
        "--mode",
        "synthetic",
        "--data-root",
        data,
        "--manifest",
        manifest,
        "--q4",
        q4,
    ];
    let first = cli(&base);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let stdout = String::from_utf8(first.stdout).unwrap();
    assert!(stdout.contains("mode=SYNTHETIC_UNPINNED"));
    assert!(stdout.contains("status=NOT_APPLICABLE"));
    assert!(stdout.contains("test=UNOPENED"));
    for arm in [
        "q4_atlas_s2a",
        "order2_dirichlet_backoff_train",
        "unigram_add1_train",
    ] {
        let row = stdout.lines().find(|l| l.starts_with(arm)).unwrap();
        assert_eq!(row.split('\t').nth(2), Some("446"));
    }
    let measured = stdout
        .split("measured_dev_q4_bpc=")
        .nth(1)
        .unwrap()
        .split(' ')
        .next()
        .unwrap()
        .to_string();
    let mut pass = base.to_vec();
    pass.extend(["--expected-q4-bpc", &measured]);
    let ok = cli(&pass);
    assert!(ok.status.success());
    assert!(String::from_utf8(ok.stdout)
        .unwrap()
        .contains("status=PASS"));
    let mut fail = base.to_vec();
    fail.extend(["--expected-q4-bpc", "0.000000001"]);
    let bad = cli(&fail);
    assert!(!bad.status.success());
    let out = String::from_utf8(bad.stdout).unwrap();
    assert!(out.contains("status=FAIL"));
    assert!(
        !out.contains("q4_atlas_s2a\t"),
        "no arm row after a failed gate"
    );
    assert!(String::from_utf8(bad.stderr)
        .unwrap()
        .contains("RUN_INVALID"));
    // Pinned mode refuses fixture inputs and overrides.
    let pinned = cli(&[
        "--mode",
        "pinned",
        "--data-root",
        data,
        "--manifest",
        manifest,
        "--q4",
        q4,
    ]);
    assert!(!pinned.status.success());
    let override_pages = cli(&[
        "--mode",
        "pinned",
        "--data-root",
        data,
        "--manifest",
        manifest,
        "--q4",
        q4,
        "--pages",
        "8",
    ]);
    assert!(!override_pages.status.success());
    // Baselines alone need no Q4 file; --q4 without the q4 arm is refused.
    let counts = cli(&[
        "--mode",
        "synthetic",
        "--data-root",
        data,
        "--manifest",
        manifest,
        "--arms",
        "order2,unigram",
    ]);
    assert!(counts.status.success());
    let mismatch = cli(&[
        "--mode",
        "synthetic",
        "--data-root",
        data,
        "--manifest",
        manifest,
        "--arms",
        "unigram",
        "--q4",
        q4,
    ]);
    assert!(!mismatch.status.success());
}
