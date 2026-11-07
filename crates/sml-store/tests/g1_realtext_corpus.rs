//! CONTEXT-3 G1 real-text corpus protocol: fail-closed loader tests on a
//! tiny hand-written fixture. No scores are computed.
//!
//! ## Coverage audit (G1 adversarial / Grok 2026-11-07)
//!
//! **Already covered (do not duplicate):**
//! - unpinned / uppercase SHA → `Unpinned`; missing `sha256` → `MissingKey`
//! - tampered corpus bytes → `ShaMismatch`; truncated → `LengthMismatch`
//! - exact duplicate text across train/eval → `Leakage`
//! - duplicate doc id, missing trailing newline, empty line, bad UTF-8 →
//!   `DuplicateDocId` / `MissingTrailingNewline` / `MalformedDoc` / `NotUtf8`
//! - empty eval (or train) split → `EmptySplit`
//! - `BadHeader`, `DuplicateKey`, `UnknownKey`, bad `split_mod` / `eval_residue`
//!
//! **Newly added here:**
//! - absolute / traversal / non-`Normal` path components → `UnsafePath`
//!   (via `assert_safe_relative_path`)
//! - symlink escape under an allowed root → `SymlinkEscape` (via
//!   `confine_regular_file`; lexical `Path::components` alone is insufficient;
//!   also wired into `g1_latency_rss` CLI for both manifest and corpus)
//! - safe relative fixture path accepted; confined regular file under root OK
//! - `byte_len 0` → `BadValue` (protocol: size must be > 0)

use sml_store::g1_realtext_corpus::{
    assert_safe_relative_path, confine_regular_file, load_pinned, parse_manifest, RealTextError,
};

const MANIFEST: &str = include_str!("fixtures/g1_realtext/tiny.manifest");
const CORPUS: &[u8] = include_bytes!("fixtures/g1_realtext/tiny.tsv");

fn ids(docs: &[sml_store::g1_realtext_corpus::RealTextDoc]) -> Vec<&str> {
    docs.iter().map(|d| d.doc_id.as_str()).collect()
}

#[test]
fn pinned_fixture_loads_with_deterministic_doc_split() {
    let c = load_pinned(MANIFEST, CORPUS).unwrap();
    assert_eq!(c.manifest().corpus_id, "g1-tiny-handwritten");
    assert_eq!(ids(c.eval()), ["d001", "d003", "d005", "d007"]);
    assert_eq!(ids(c.train()), ["d002", "d004", "d006", "d008"]);
}

#[test]
fn unpinned_sha_is_refused() {
    let m = MANIFEST.replace(
        "sha256 8bf10364c39828101d9716aa7c1097ba9a967b760546ec422e1e99c24c182031",
        "sha256 UNPINNED",
    );
    assert_eq!(load_pinned(&m, CORPUS), Err(RealTextError::Unpinned));
    let upper = MANIFEST.replace("8bf10364", "8BF10364");
    assert_eq!(parse_manifest(&upper), Err(RealTextError::Unpinned));
    let no_sha: String = MANIFEST
        .lines()
        .filter(|l| !l.starts_with("sha256"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        parse_manifest(&no_sha),
        Err(RealTextError::MissingKey { key: "sha256" })
    );
}

#[test]
fn tampered_bytes_are_refused() {
    let mut bytes = CORPUS.to_vec();
    bytes[10] ^= 1;
    assert!(matches!(
        load_pinned(MANIFEST, &bytes),
        Err(RealTextError::ShaMismatch { .. })
    ));
    assert!(matches!(
        load_pinned(MANIFEST, &CORPUS[..CORPUS.len() - 1]),
        Err(RealTextError::LengthMismatch {
            expected: 459,
            actual: 458
        })
    ));
}

fn repin(corpus: &[u8]) -> String {
    let hex = sml_store::raw_context::sha256_raw_bytes(corpus)
        .unwrap()
        .hex();
    MANIFEST
        .lines()
        .map(|l| {
            if l.starts_with("sha256 ") {
                format!("sha256 {hex}")
            } else if l.starts_with("byte_len ") {
                format!("byte_len {}", corpus.len())
            } else {
                l.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn exact_duplicate_across_split_is_leakage() {
    // d002 (train) text copied into d001 (eval).
    let s = std::str::from_utf8(CORPUS).unwrap().replace(
        "The tide rose over the old harbour wall before dawn.",
        "She wrote the ledger entry twice, then crossed one out.",
    );
    let err = load_pinned(&repin(s.as_bytes()), s.as_bytes()).unwrap_err();
    assert_eq!(
        err,
        RealTextError::Leakage {
            train_id: "d002".into(),
            eval_id: "d001".into()
        }
    );
}

#[test]
fn malformed_corpora_are_refused() {
    let dup = std::str::from_utf8(CORPUS)
        .unwrap()
        .replace("d008\t", "d007\t");
    assert_eq!(
        load_pinned(&repin(dup.as_bytes()), dup.as_bytes()),
        Err(RealTextError::DuplicateDocId {
            doc_id: "d007".into()
        })
    );
    let no_nl = &CORPUS[..CORPUS.len() - 1];
    assert_eq!(
        load_pinned(&repin(no_nl), no_nl),
        Err(RealTextError::MissingTrailingNewline)
    );
    let empty_line = b"d001\tx\n\nd002\ty\n";
    assert_eq!(
        load_pinned(&repin(empty_line), empty_line),
        Err(RealTextError::MalformedDoc { line_no: 2 })
    );
    let bad_utf8 = b"d001\t\xff\n";
    assert_eq!(
        load_pinned(&repin(bad_utf8), bad_utf8),
        Err(RealTextError::NotUtf8)
    );
    // only train-side ids (residue != 1) -> empty eval split
    let only_train = b"d002\ta\nd004\tb\n";
    assert_eq!(
        load_pinned(&repin(only_train), only_train),
        Err(RealTextError::EmptySplit { eval: true })
    );
}

#[test]
fn manifest_grammar_is_strict() {
    assert_eq!(parse_manifest("nope\n"), Err(RealTextError::BadHeader));
    let dup = format!("{MANIFEST}\nsplit_mod 4\n");
    assert_eq!(
        parse_manifest(&dup),
        Err(RealTextError::DuplicateKey {
            key: "split_mod".into()
        })
    );
    let unk = format!("{MANIFEST}\nscore 0.9\n");
    assert_eq!(
        parse_manifest(&unk),
        Err(RealTextError::UnknownKey {
            key: "score".into()
        })
    );
    let bad_res = MANIFEST.replace("eval_residue 1", "eval_residue 4");
    assert!(matches!(
        parse_manifest(&bad_res),
        Err(RealTextError::BadValue {
            key: "eval_residue",
            ..
        })
    ));
    let bad_mod = MANIFEST.replace("split_mod 4", "split_mod 1");
    assert!(matches!(
        parse_manifest(&bad_mod),
        Err(RealTextError::BadValue {
            key: "split_mod",
            ..
        })
    ));
}

#[test]
fn unsafe_paths_are_refused() {
    use std::path::Path;
    // Absolute, parent-dir traversal, empty, lone `.` / `..` must fail closed.
    // Note: `Path::components` elides mid-path `CurDir` (e.g. `foo/./bar` ≡
    // `foo/bar`), so that form is accepted as a normal relative path.
    let cases = [
        "/etc/passwd",
        "../secret",
        "foo/../../etc/passwd",
        "",
        ".",
        "..",
    ];
    for s in cases {
        let err = assert_safe_relative_path(Path::new(s)).unwrap_err();
        assert!(
            matches!(err, RealTextError::UnsafePath { .. }),
            "expected UnsafePath for {s:?}, got {err:?}"
        );
    }
    // Absolute Windows-style prefix: checked on Windows only (`is_absolute`).
    #[cfg(windows)]
    {
        let err = assert_safe_relative_path(Path::new(r"C:\Windows\System32")).unwrap_err();
        assert!(matches!(err, RealTextError::UnsafePath { .. }));
    }
    // Safe relative fixture paths (and mid-path `.` elided by components) pass.
    assert_safe_relative_path(Path::new("fixtures/g1_realtext/tiny.tsv")).unwrap();
    assert_safe_relative_path(Path::new("tiny.manifest")).unwrap();
    assert_safe_relative_path(Path::new("foo/./bar")).unwrap();
}

#[test]
fn zero_byte_len_is_refused() {
    let m = MANIFEST.replace("byte_len 459", "byte_len 0");
    assert!(matches!(
        parse_manifest(&m),
        Err(RealTextError::BadValue {
            key: "byte_len",
            ..
        })
    ));
}

#[cfg(unix)]
#[test]
fn symlink_escape_is_refused_for_manifest_and_corpus() {
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::Path;

    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let home = std::env::temp_dir().join(format!("g1-symlink-escape-{}-{id}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(&home).unwrap();
    let root = home.join("root");
    let outside = home.join("outside");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&outside).unwrap();

    // Outside secrets a pure lexical check cannot see.
    fs::write(outside.join("secret.manifest"), b"SECRET_MANIFEST\n").unwrap();
    fs::write(outside.join("secret.tsv"), b"SECRET_CORPUS\n").unwrap();

    // Relative names that Path::components treats as Normal-only.
    let man_link = "escape.manifest";
    let corp_link = "escape.tsv";
    symlink(outside.join("secret.manifest"), root.join(man_link)).unwrap();
    symlink(outside.join("secret.tsv"), root.join(corp_link)).unwrap();

    // Lexical check alone would ACCEPT these (no absolute / no `..`).
    assert_safe_relative_path(Path::new(man_link)).unwrap();
    assert_safe_relative_path(Path::new(corp_link)).unwrap();

    // Confinement must FAIL CLOSED for both manifest and corpus.
    let man_err = confine_regular_file(&root, Path::new(man_link)).unwrap_err();
    assert!(
        matches!(man_err, RealTextError::SymlinkEscape { .. }),
        "manifest symlink escape: {man_err:?}"
    );
    let corp_err = confine_regular_file(&root, Path::new(corp_link)).unwrap_err();
    assert!(
        matches!(corp_err, RealTextError::SymlinkEscape { .. }),
        "corpus symlink escape: {corp_err:?}"
    );

    // Mid-path directory symlink that would escape via `via/secret.tsv`.
    let via = "via";
    symlink(&outside, root.join(via)).unwrap();
    let mid = "via/secret.tsv";
    assert_safe_relative_path(Path::new(mid)).unwrap();
    let mid_err = confine_regular_file(&root, Path::new(mid)).unwrap_err();
    assert!(
        matches!(mid_err, RealTextError::SymlinkEscape { .. }),
        "mid-path symlink escape: {mid_err:?}"
    );

    // Positive control: regular files under root are admitted.
    fs::write(root.join("ok.manifest"), b"ok\n").unwrap();
    fs::write(root.join("ok.tsv"), b"ok\n").unwrap();
    let ok_m = confine_regular_file(&root, Path::new("ok.manifest")).unwrap();
    let ok_c = confine_regular_file(&root, Path::new("ok.tsv")).unwrap();
    assert!(ok_m.ends_with("ok.manifest"));
    assert!(ok_c.ends_with("ok.tsv"));
    assert!(ok_m.starts_with(fs::canonicalize(&root).unwrap()));
    assert!(ok_c.starts_with(fs::canonicalize(&root).unwrap()));

    let _ = fs::remove_dir_all(&home);
}
