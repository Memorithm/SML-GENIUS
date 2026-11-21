//! Synthetic-data tests for the NBE-2 mixture arms
//! (`docs/SML-NEXT-BYTE-NBE2-MIXTURE-PREREGISTRATION.md`). No number here is a
//! model measurement: every input is a generated fixture.
use sml_core::{
    byte_factorization::ByteFactorization,
    elastic_ngram_paged_sha::{hex, sha256},
    next_byte_eval::{
        check_normalized, evaluate_documents, CausalBayesMixture, LinearMixture, NextBytePredictor,
        Order2Counts, Q4AtlasPredictor, Result, UnigramCounts, FIRST_SCORED_POSITION,
        LINEAR_MIXTURE_WEIGHT,
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
        "sml-nbe2-{label}-{}-{nanos}-{}",
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

fn fitted_counts() -> (Order2Counts, UnigramCounts) {
    let mut o2 = Order2Counts::default();
    let mut u1 = UnigramCounts::default();
    for seed in 10..14 {
        let doc = text(seed, 500);
        o2.fit_document(&doc).unwrap();
        u1.fit_document(&doc).unwrap();
    }
    (o2, u1)
}

/// Counts probability calls and checks the evaluator-style contract: within
/// a document, every probability call is for history length `consumed` and
/// the length grows by exactly one per call.
struct Counting<P> {
    inner: P,
    probability_calls: u64,
    expected_len: usize,
    strict: bool,
}

impl<P> Counting<P> {
    fn new(inner: P, strict: bool) -> Self {
        Self {
            inner,
            probability_calls: 0,
            expected_len: FIRST_SCORED_POSITION,
            strict,
        }
    }
}

impl<P: NextBytePredictor> NextBytePredictor for Counting<P> {
    fn arm(&self) -> &str {
        self.inner.arm()
    }
    fn begin_document(&mut self) -> Result<()> {
        self.expected_len = FIRST_SCORED_POSITION;
        self.inner.begin_document()
    }
    fn log2_probability(&mut self, history: &[u8], next: u8) -> Result<f64> {
        if self.strict {
            assert_eq!(history.len(), self.expected_len, "one call per position");
            self.expected_len += 1;
        }
        self.probability_calls += 1;
        self.inner.log2_probability(history, next)
    }
    fn top1(&mut self, history: &[u8]) -> Result<u8> {
        self.inner.top1(history)
    }
}

fn docs() -> Vec<Vec<u8>> {
    vec![text(1, 220), text(2, 2), text(3, 90), text(4, 41)]
}

#[test]
fn linear_mixture_is_normalized_matches_formula_and_obeys_jensen() {
    let (mut o2, mut u1) = fitted_counts();
    let docs = docs();
    let refs = docs.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let l_o2 = evaluate_documents(&mut o2, &refs, false).unwrap();
    let l_u1 = evaluate_documents(&mut u1, &refs, false).unwrap();
    let mut mix = LinearMixture::new("mix", &mut o2, &mut u1, LINEAR_MIXTURE_WEIGHT).unwrap();
    let l_mix = evaluate_documents(&mut mix, &refs, true).unwrap();
    assert_eq!(l_mix.scored_bytes, l_o2.scored_bytes);
    assert!(l_mix.total_loss_bits <= 0.5 * (l_o2.total_loss_bits + l_u1.total_loss_bits) + 1e-9);
    let histories = [&docs[0][..2], &docs[0][..57], &docs[2][..9]];
    let mut got = Vec::new();
    for history in histories {
        check_normalized(&mut mix, history).unwrap();
        for c in [b' ', b'e', 0u8, 255] {
            got.push(mix.log2_probability(history, c).unwrap().exp2());
        }
    }
    drop(mix);
    let mut k = 0;
    for history in histories {
        let (a, b) = (history[history.len() - 2], history[history.len() - 1]);
        for c in [b' ', b'e', 0u8, 255] {
            let expected = 0.5 * o2.probability(a, b, c) + 0.5 * u1.probability(c);
            assert!(
                (got[k] - expected).abs() < 1e-12,
                "{} vs {expected}",
                got[k]
            );
            k += 1;
        }
    }
}

#[test]
fn linear_mixture_rejects_degenerate_weights() {
    let (mut o2, mut u1) = fitted_counts();
    for w in [0.0, 1.0, -0.1, 1.5, f64::NAN] {
        assert!(LinearMixture::new("mix", &mut o2, &mut u1, w).is_err());
    }
}

#[test]
fn bayes_mixture_obeys_the_one_bit_per_document_bound_and_is_normalized() {
    let (mut o2, mut u1) = fitted_counts();
    let docs = docs();
    let refs = docs.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let l_o2 = evaluate_documents(&mut o2, &refs, false).unwrap();
    let l_u1 = evaluate_documents(&mut u1, &refs, false).unwrap();
    let mut mix = CausalBayesMixture::new("bayes", &mut o2, &mut u1);
    let report = evaluate_documents(&mut mix, &refs, true).unwrap();
    let best = l_o2.total_loss_bits.min(l_u1.total_loss_bits);
    assert!(report.total_loss_bits <= best + report.documents as f64 + 1e-9);
    // Fresh document: prior 1/2, distribution sums to one at several depths.
    mix.begin_document().unwrap();
    assert!((mix.weight_a() - 0.5).abs() < 1e-15);
    let doc = &docs[0];
    for end in [2usize, 3, 30, 120] {
        check_normalized(&mut mix, &doc[..end]).unwrap();
    }
    // Posterior = prior x causal likelihood of the revealed bytes doc[2..120].
    let (o2, u1) = fitted_counts();
    let (mut la, mut lb) = (0f64, 0f64);
    for i in FIRST_SCORED_POSITION..120 {
        la += o2.probability(doc[i - 2], doc[i - 1], doc[i]).log2();
        lb += u1.probability(doc[i]).log2();
    }
    let expected = 1.0 / (1.0 + (lb - la).exp2());
    let got = mix.weight_a();
    assert!(
        (got - expected).abs() <= 1e-9 * expected.max(1e-300) + 1e-300,
        "{got} vs {expected}"
    );
    // A shrinking history without begin_document is a contract violation.
    assert!(mix.log2_probability(&doc[..10], doc[10]).is_err());
}

#[test]
fn bayes_mixture_of_identical_experts_equals_the_expert() {
    let (o2, _) = fitted_counts();
    let mut solo = Order2Counts::default();
    for seed in 10..14 {
        solo.fit_document(&text(seed, 500)).unwrap();
    }
    let mut twin = Order2Counts::default();
    for seed in 10..14 {
        twin.fit_document(&text(seed, 500)).unwrap();
    }
    let docs = docs();
    let refs = docs.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let mut o2 = o2;
    let alone = evaluate_documents(&mut o2, &refs, true).unwrap();
    let mut mix = CausalBayesMixture::new("bayes", &mut solo, &mut twin);
    let mixed = evaluate_documents(&mut mix, &refs, true).unwrap();
    assert!((alone.total_loss_bits - mixed.total_loss_bits).abs() < 1e-9);
    assert_eq!(alone.top1_stream_sha256, mixed.top1_stream_sha256);
}

#[test]
fn bayes_mixture_calls_each_expert_once_per_position_and_hides_the_target() {
    let (o2, u1) = fitted_counts();
    let docs = docs();
    let refs = docs.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let mut a = Counting::new(o2, true);
    let mut b = Counting::new(u1, true);
    let mut mix = CausalBayesMixture::new("bayes", &mut a, &mut b);
    let report = evaluate_documents(&mut mix, &refs, false).unwrap();
    drop(mix);
    assert_eq!(a.probability_calls, report.scored_bytes);
    assert_eq!(b.probability_calls, report.scored_bytes);
    // top1 at a position does not depend on the byte that follows it.
    let (o2, u1) = fitted_counts();
    let mut mix = CausalBayesMixture::new("bayes", o2, u1);
    let doc = &docs[0];
    let mut guesses = Vec::new();
    for variant in [b'a', b'z'] {
        let mut altered = doc[..60].to_vec();
        altered.push(variant);
        mix.begin_document().unwrap();
        for i in FIRST_SCORED_POSITION..60 {
            mix.log2_probability(&altered[..i], altered[i]).unwrap();
        }
        guesses.push(mix.top1(&altered[..60]).unwrap());
    }
    assert_eq!(guesses[0], guesses[1]);
}

fn trained_q4(dir: &std::path::Path) -> (PathBuf, SparseConfig) {
    let config = SparseConfig {
        pages: 8,
        max_cached_pages: 2,
        learning_rate: 0.15,
    };
    let root = dir.join("atlas");
    let mut atlas =
        SparseGradientAtlas::create_with_factorization(&root, config, ByteFactorization::MsbTree)
            .unwrap();
    for seed in 0..3 {
        atlas.train_document(&text(seed, 300)).unwrap();
    }
    let q4 = root.join("weights.q4");
    atlas.export_q4(&q4).unwrap();
    (q4, config)
}

#[test]
fn q4_and_order2_mixtures_obey_both_theorem_gates() {
    let dir = temp_dir("q4mix");
    let (q4, config) = trained_q4(&dir);
    let open = || {
        Q4AtlasPredictor::new(
            "q4",
            QuantizedSparseAtlas::open_with_factorization(&q4, config, ByteFactorization::MsbTree)
                .unwrap(),
            ByteFactorization::MsbTree,
        )
    };
    let (mut o2, _) = fitted_counts();
    let docs = docs();
    let refs = docs.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let l_q4 = evaluate_documents(&mut open(), &refs, false).unwrap();
    let l_o2 = evaluate_documents(&mut o2, &refs, false).unwrap();
    let mut half = LinearMixture::new("h", open(), &mut o2, LINEAR_MIXTURE_WEIGHT).unwrap();
    let l_half = evaluate_documents(&mut half, &refs, false).unwrap();
    check_normalized(&mut half, &docs[0][..2]).unwrap();
    drop(half);
    let mut bayes = CausalBayesMixture::new("b", open(), &mut o2);
    let l_bayes = evaluate_documents(&mut bayes, &refs, false).unwrap();
    assert!(l_half.total_loss_bits <= 0.5 * (l_q4.total_loss_bits + l_o2.total_loss_bits) + 1e-9);
    assert!(
        l_bayes.total_loss_bits
            <= l_q4.total_loss_bits.min(l_o2.total_loss_bits) + l_bayes.documents as f64 + 1e-9
    );
    fs::remove_dir_all(&dir).ok();
}

fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_sml-next-byte-eval"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn cli_mixture_arms_are_opt_in_gated_and_leave_default_rows_unchanged() {
    let dir = temp_dir("cli");
    let data = dir.join("data");
    fs::create_dir_all(&data).unwrap();
    let mut body = String::from("role\tpath\tbytes\tsha256\trights_review\n");
    for (role, name, bytes) in [
        ("TRAIN", "t1.txt", text(1, 600)),
        ("TRAIN", "t2.txt", text(2, 600)),
        ("DEV", "d1.txt", text(3, 300)),
        ("DEV", "d2.txt", text(4, 150)),
    ] {
        fs::write(data.join(name), &bytes).unwrap();
        body.push_str(&format!(
            "{role}\t{name}\t{}\t{}\tOWNED_REVIEWED\n",
            bytes.len(),
            hex(&sha256(&bytes))
        ));
    }
    let manifest = dir.join("manifest.tsv");
    fs::write(&manifest, body).unwrap();
    let (q4, _) = trained_q4(&dir);
    let (data, manifest, q4) = (
        data.to_str().unwrap(),
        manifest.to_str().unwrap(),
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
    let run = |extra: &[&str]| {
        let mut args = base.to_vec();
        args.extend_from_slice(extra);
        cli(&args)
    };
    let default = run(&[]);
    assert!(default.status.success());
    let default_out = String::from_utf8(default.stdout).unwrap();
    assert!(!default_out.contains("mix_"));
    assert!(!default_out.contains("MIXTURE_GATES"));

    let mixed = run(&["--arms", "q4,order2,unigram,mix_half,mix_bayes"]);
    assert!(
        mixed.status.success(),
        "{}",
        String::from_utf8_lossy(&mixed.stderr)
    );
    let mixed_out = String::from_utf8(mixed.stdout).unwrap();
    // Existing rows are byte-identical except for wall-clock latency columns.
    let stable = |out: &str, arm: &str| {
        let row = out
            .lines()
            .find(|l| l.starts_with(arm))
            .unwrap()
            .to_string();
        let cols = row.split('\t').collect::<Vec<_>>();
        [0usize, 1, 2, 3, 4, 5, 6, 7]
            .iter()
            .filter_map(|&i| cols.get(i).map(|s| s.to_string()))
            .collect::<Vec<_>>()
    };
    let header = default_out
        .lines()
        .find(|l| l.starts_with("arm\t"))
        .unwrap()
        .split('\t')
        .map(str::to_string)
        .collect::<Vec<_>>();
    let latency = header
        .iter()
        .enumerate()
        .filter(|(_, h)| h.contains("ns"))
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    for arm in [
        "q4_atlas_s2a\t",
        "order2_dirichlet_backoff_train\t",
        "unigram_add1_train\t",
    ] {
        let a = stable(&default_out, arm);
        let b = stable(&mixed_out, arm);
        for (i, (x, y)) in a.iter().zip(&b).enumerate() {
            if !latency.contains(&i) {
                assert_eq!(x, y, "{arm} column {i}");
            }
        }
    }
    for arm in ["mix_q4_order2_linear_half\t", "mix_q4_order2_bayes_doc\t"] {
        let row = mixed_out.lines().find(|l| l.starts_with(arm)).unwrap();
        assert_eq!(row.split('\t').nth(2), Some("446"));
    }
    let gates = mixed_out
        .lines()
        .find(|l| l.starts_with("MIXTURE_GATES"))
        .unwrap();
    assert!(gates.contains("order2_repro=NOT_APPLICABLE"));
    assert!(gates.contains("m1_jensen=PASS"));
    assert!(gates.contains("m2_bayes_bound=PASS"));

    // Mixtures need both experts scored before them, in the same run.
    for arms in [
        "mix_half",
        "q4,mix_half",
        "order2,mix_bayes,q4",
        "mix_bayes,q4,order2",
    ] {
        let out = run(&["--arms", arms]);
        assert!(!out.status.success(), "{arms} must be refused");
    }
    // Pinned mixture runs are pre-registered without top-1.
    let pinned_top1 = cli(&[
        "--mode",
        "pinned",
        "--data-root",
        data,
        "--manifest",
        manifest,
        "--checkpoint",
        data,
        "--arms",
        "q4,order2,mix_half",
    ]);
    assert!(!pinned_top1.status.success());
    assert!(String::from_utf8(pinned_top1.stderr)
        .unwrap()
        .contains("--no-top1"));
    // A mixture arm alone needs a Q4 input.
    let no_q4 = cli(&[
        "--mode",
        "synthetic",
        "--data-root",
        data,
        "--manifest",
        manifest,
        "--arms",
        "order2,mix_half",
    ]);
    assert!(!no_q4.status.success());
    fs::remove_dir_all(&dir).ok();
}
