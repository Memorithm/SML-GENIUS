//! G2 pinned admission hardening for CONTEXT-3 G1 real-text corpora:
//! placeholder `source` / `license` refusal and the reproducible split
//! fingerprint. No scores are computed; the fingerprint identifies a split,
//! it is not a quality measurement.

use sml_store::g1_realtext_corpus::{
    is_placeholder_metadata, load_pinned, parse_manifest, split_fingerprint_of, RealTextError,
    PLACEHOLDER_METADATA,
};

const MANIFEST: &str = include_str!("fixtures/g1_realtext/tiny.manifest");
const CORPUS: &[u8] = include_bytes!("fixtures/g1_realtext/tiny.tsv");
const SMOKE_MANIFEST: &str = include_str!("fixtures/g1_realtext/smoke.manifest");
const SMOKE_CORPUS: &[u8] = include_bytes!("fixtures/g1_realtext/smoke.tsv");
const PES2O7_MANIFEST: &str = include_str!("../../../docs/g1-realtext/g1-pes2o7-v1.manifest");

fn with_line(manifest: &str, key: &str, value: &str) -> String {
    manifest
        .lines()
        .map(|l| {
            if l.starts_with(&format!("{key} ")) {
                format!("{key} {value}")
            } else {
                l.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn repin(manifest: &str, corpus: &[u8]) -> String {
    let hex = sml_store::raw_context::sha256_raw_bytes(corpus)
        .unwrap()
        .hex();
    with_line(
        &with_line(manifest, "sha256", &hex),
        "byte_len",
        &corpus.len().to_string(),
    )
}

#[test]
fn committed_manifests_remain_admissible() {
    for m in [MANIFEST, SMOKE_MANIFEST, PES2O7_MANIFEST] {
        let parsed = parse_manifest(m).unwrap();
        assert!(!is_placeholder_metadata(&parsed.source));
        assert!(!is_placeholder_metadata(&parsed.license));
    }
}

#[test]
fn placeholder_source_and_license_are_refused() {
    let mut variants: Vec<String> = Vec::new();
    for p in PLACEHOLDER_METADATA {
        variants.push(p.to_owned());
        variants.push(p.to_ascii_lowercase());
        variants.push(format!("  {}\t", p.to_ascii_lowercase()));
    }
    // Mixed case and punctuation-only forms.
    variants.extend(
        [
            "Unknown", "ToDo", "n/A", "NuLl", "--", "???", "...", "-/-", "*",
        ]
        .map(str::to_owned),
    );
    let mut refused = 0_usize;
    for key in ["source", "license"] {
        for v in &variants {
            let m = with_line(MANIFEST, key, v);
            let err = parse_manifest(&m).unwrap_err();
            match err {
                RealTextError::BadValue { key: k, .. } => assert_eq!(k, key, "{v:?}"),
                other => panic!("expected BadValue for {key}={v:?}, got {other:?}"),
            }
            // load_pinned is gated by parse_manifest: also refused.
            assert!(matches!(
                load_pinned(&m, CORPUS),
                Err(RealTextError::BadValue { .. })
            ));
            refused += 1;
        }
    }
    assert_eq!(refused, 2 * (3 * PLACEHOLDER_METADATA.len() + 9));
}

#[test]
fn informative_values_containing_placeholder_words_are_accepted() {
    // Only an exact (trimmed, case-insensitive) placeholder is refused.
    for v in [
        "unknown-author blog 2019",
        "CC0-1.0",
        "NA-region archive",
        "MIT",
    ] {
        parse_manifest(&with_line(MANIFEST, "license", v)).unwrap();
        parse_manifest(&with_line(MANIFEST, "source", v)).unwrap();
    }
}

/// Snapshots computed once from `load_pinned` on the committed fixtures
/// (fixture identity only; not a measurement). A change here means the
/// fingerprint preimage or a fixture changed and must be reviewed.
const TINY_FP: &str = "a99e22bb897eb5158670ba6ad0cabedee6e0f62791c68cd8729db7ab4e969d67";
const SMOKE_FP: &str = "eaa58ef865832fec30651810f94fe5e509ee1ce433bf5358478e00cc24d9cc20";

#[test]
fn fixture_fingerprints_match_snapshot() {
    let t = load_pinned(MANIFEST, CORPUS).unwrap();
    let s = load_pinned(SMOKE_MANIFEST, SMOKE_CORPUS).unwrap();
    assert_eq!(t.split_fingerprint_hex().unwrap(), TINY_FP);
    assert_eq!(s.split_fingerprint_hex().unwrap(), SMOKE_FP);
}

#[test]
fn split_fingerprint_is_deterministic() {
    let a = load_pinned(MANIFEST, CORPUS).unwrap();
    let b = load_pinned(MANIFEST, CORPUS).unwrap();
    let fa = a.split_fingerprint().unwrap();
    assert_eq!(fa, b.split_fingerprint().unwrap());
    let hex = a.split_fingerprint_hex().unwrap();
    assert_eq!(hex.len(), 64);
    assert!(hex
        .bytes()
        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    let from_bytes: String = fa.iter().map(|x| format!("{x:02x}")).collect();
    assert_eq!(hex, from_bytes);
    let s = load_pinned(SMOKE_MANIFEST, SMOKE_CORPUS).unwrap();
    assert_ne!(s.split_fingerprint().unwrap(), fa);
    assert_eq!(
        s.split_fingerprint().unwrap(),
        load_pinned(SMOKE_MANIFEST, SMOKE_CORPUS)
            .unwrap()
            .split_fingerprint()
            .unwrap()
    );
}

#[test]
fn split_fingerprint_ignores_document_order_for_a_fixed_pin() {
    let c = load_pinned(MANIFEST, CORPUS).unwrap();
    let m = c.manifest();
    let train: Vec<&str> = c.train().iter().map(|d| d.doc_id.as_str()).collect();
    let eval: Vec<&str> = c.eval().iter().map(|d| d.doc_id.as_str()).collect();
    let mut train_rev = train.clone();
    train_rev.reverse();
    let mut eval_rev = eval.clone();
    eval_rev.reverse();
    let fwd = split_fingerprint_of(
        &m.corpus_id,
        &m.sha256_hex,
        m.split_mod,
        m.eval_residue,
        &train,
        &eval,
    )
    .unwrap();
    let rev = split_fingerprint_of(
        &m.corpus_id,
        &m.sha256_hex,
        m.split_mod,
        m.eval_residue,
        &train_rev,
        &eval_rev,
    )
    .unwrap();
    assert_eq!(fwd, rev);
    assert_eq!(fwd, c.split_fingerprint().unwrap());

    // Reversed corpus lines: same split membership, but new bytes => new pin.
    let text = std::str::from_utf8(CORPUS).unwrap();
    let mut lines: Vec<&str> = text.lines().collect();
    lines.reverse();
    let permuted = format!("{}\n", lines.join("\n"));
    let pm = repin(MANIFEST, permuted.as_bytes());
    let p = load_pinned(&pm, permuted.as_bytes()).unwrap();
    let mut p_eval: Vec<&str> = p.eval().iter().map(|d| d.doc_id.as_str()).collect();
    p_eval.sort_unstable();
    let mut c_eval = eval.clone();
    c_eval.sort_unstable();
    assert_eq!(p_eval, c_eval, "same split membership");
    let pmf = p.manifest();
    assert_eq!(
        p.split_fingerprint().unwrap(),
        split_fingerprint_of(
            &pmf.corpus_id,
            &pmf.sha256_hex,
            pmf.split_mod,
            pmf.eval_residue,
            &train,
            &eval
        )
        .unwrap(),
        "order-independent given the permuted corpus pin"
    );
    assert_ne!(
        p.split_fingerprint().unwrap(),
        fwd,
        "fingerprint binds the exact pinned bytes"
    );
}

#[test]
fn split_fingerprint_is_sensitive_to_split_and_identity() {
    let c = load_pinned(MANIFEST, CORPUS).unwrap();
    let m = c.manifest();
    let train: Vec<&str> = c.train().iter().map(|d| d.doc_id.as_str()).collect();
    let eval: Vec<&str> = c.eval().iter().map(|d| d.doc_id.as_str()).collect();
    let base = c.split_fingerprint().unwrap();
    let fp = |cid: &str, sha: &str, md: u64, r: u64, t: &[&str], e: &[&str]| {
        split_fingerprint_of(cid, sha, md, r, t, e).unwrap()
    };
    // One eval id changed.
    let mut e1 = eval.clone();
    e1[0] = "d009";
    assert_ne!(
        fp(
            &m.corpus_id,
            &m.sha256_hex,
            m.split_mod,
            m.eval_residue,
            &train,
            &e1
        ),
        base
    );
    // One doc moved from eval to train (membership change).
    let mut t2 = train.clone();
    t2.push(eval[0]);
    assert_ne!(
        fp(
            &m.corpus_id,
            &m.sha256_hex,
            m.split_mod,
            m.eval_residue,
            &t2,
            &eval[1..]
        ),
        base
    );
    // Train and eval swapped.
    assert_ne!(
        fp(
            &m.corpus_id,
            &m.sha256_hex,
            m.split_mod,
            m.eval_residue,
            &eval,
            &train
        ),
        base
    );
    // Residue / modulus / corpus id / pin changed.
    assert_ne!(
        fp(&m.corpus_id, &m.sha256_hex, m.split_mod, 2, &train, &eval),
        base
    );
    assert_ne!(
        fp(
            &m.corpus_id,
            &m.sha256_hex,
            5,
            m.eval_residue,
            &train,
            &eval
        ),
        base
    );
    assert_ne!(
        fp(
            "g1-other",
            &m.sha256_hex,
            m.split_mod,
            m.eval_residue,
            &train,
            &eval
        ),
        base
    );
    assert_ne!(
        fp(
            &m.corpus_id,
            &"0".repeat(64),
            m.split_mod,
            m.eval_residue,
            &train,
            &eval
        ),
        base
    );
    // Length-prefixing: concatenation ambiguity does not collide.
    assert_ne!(
        fp("c", "s", 2, 0, &["ab", "c"], &["d"]),
        fp("c", "s", 2, 0, &["a", "bc"], &["d"])
    );

    // End-to-end on the real loader: with split_mod 2, residues 0 and 1 both
    // admit (complementary splits) and yield distinct fingerprints.
    let m2 = with_line(MANIFEST, "split_mod", "2");
    let a = load_pinned(&with_line(&m2, "eval_residue", "0"), CORPUS).unwrap();
    let b = load_pinned(&with_line(&m2, "eval_residue", "1"), CORPUS).unwrap();
    assert_eq!(a.train().len() + a.eval().len(), 8);
    assert_eq!(a.eval().len(), b.train().len());
    assert_ne!(
        a.split_fingerprint().unwrap(),
        b.split_fingerprint().unwrap()
    );
    assert_ne!(a.split_fingerprint().unwrap(), base);
    assert_ne!(b.split_fingerprint().unwrap(), base);
}
