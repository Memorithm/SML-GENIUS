//! CONTEXT-3 G1: real-text corpus fixture protocol (scaffolding only).
//!
//! This module measures nothing and produces no score. It defines how a
//! real-text corpus is *admitted* to a G1 run so that a later measurement can
//! be pre-registered against a pinned, leakage-checked input. See
//! `docs/G1-CONTEXT-3-REALTEXT-CORPUS-PROTOCOL.md`.
//!
//! Manifest (line-oriented, `key value`, `#` comments, every key exactly once):
//!
//! ```text
//! G1_REALTEXT_MANIFEST v1
//! corpus_id <ascii id>
//! source <free text: URL / citation>
//! license <SPDX id or free text>
//! sha256 <64 lowercase hex of the corpus file bytes>
//! byte_len <decimal>
//! split_mod <M >= 2>
//! eval_residue <0 <= r < M>
//! ```
//!
//! Corpus file: UTF-8, one document per `\n`-terminated line,
//! `<doc_id>\t<text>`. Doc ids are unique; text is non-empty and contains no
//! tab. Split is by document: `sha256(doc_id)[0..8]` (big-endian u64) mod `M`
//! `== r` -> eval, else train. Fail-closed: an unpinned or mismatching SHA, a
//! wrong length, a duplicate id, or identical text appearing in both train and
//! eval (exact-duplicate leakage) is an error, never a warning.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

use crate::raw_context::sha256_raw_bytes;

pub const MANIFEST_HEADER: &str = "G1_REALTEXT_MANIFEST v1";
const KEYS: [&str; 7] = [
    "corpus_id",
    "source",
    "license",
    "sha256",
    "byte_len",
    "split_mod",
    "eval_residue",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealTextError {
    BadHeader,
    MalformedLine {
        line: String,
    },
    UnknownKey {
        key: String,
    },
    DuplicateKey {
        key: String,
    },
    MissingKey {
        key: &'static str,
    },
    BadValue {
        key: &'static str,
        value: String,
    },
    /// `sha256` is absent-as-placeholder (e.g. `UNPINNED`) or not 64 lowercase hex.
    Unpinned,
    LengthMismatch {
        expected: u64,
        actual: u64,
    },
    ShaMismatch {
        expected: String,
        actual: String,
    },
    NotUtf8,
    MissingTrailingNewline,
    MalformedDoc {
        line_no: usize,
    },
    DuplicateDocId {
        doc_id: String,
    },
    EmptySplit {
        eval: bool,
    },
    Leakage {
        train_id: String,
        eval_id: String,
    },
    /// Absolute path, empty path, or non-`Normal` component (`..`, `.`, prefix, root).
    UnsafePath {
        path: String,
    },
    Hash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealTextManifest {
    pub corpus_id: String,
    pub source: String,
    pub license: String,
    pub sha256_hex: String,
    pub byte_len: u64,
    pub split_mod: u64,
    pub eval_residue: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealTextDoc {
    pub doc_id: String,
    pub text: String,
}

/// A corpus whose bytes matched the pinned manifest and passed split/leakage
/// rules. Construction is the only gate; there is no unchecked constructor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedRealTextCorpus {
    manifest: RealTextManifest,
    train: Vec<RealTextDoc>,
    eval: Vec<RealTextDoc>,
}

impl PinnedRealTextCorpus {
    #[must_use]
    pub fn manifest(&self) -> &RealTextManifest {
        &self.manifest
    }
    #[must_use]
    pub fn train(&self) -> &[RealTextDoc] {
        &self.train
    }
    #[must_use]
    pub fn eval(&self) -> &[RealTextDoc] {
        &self.eval
    }
}

fn is_pinned_hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn parse_manifest(text: &str) -> Result<RealTextManifest, RealTextError> {
    let mut lines = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'));
    if lines.next() != Some(MANIFEST_HEADER) {
        return Err(RealTextError::BadHeader);
    }
    let mut map: BTreeMap<&str, &str> = BTreeMap::new();
    for line in lines {
        let (key, value) = line
            .split_once(' ')
            .map(|(k, v)| (k, v.trim()))
            .filter(|(_, v)| !v.is_empty())
            .ok_or_else(|| RealTextError::MalformedLine {
                line: line.to_owned(),
            })?;
        if !KEYS.contains(&key) {
            return Err(RealTextError::UnknownKey {
                key: key.to_owned(),
            });
        }
        if map.insert(key, value).is_some() {
            return Err(RealTextError::DuplicateKey {
                key: key.to_owned(),
            });
        }
    }
    let get = |key: &'static str| {
        map.get(key)
            .copied()
            .ok_or(RealTextError::MissingKey { key })
    };
    let num = |key: &'static str| -> Result<u64, RealTextError> {
        let v = get(key)?;
        v.parse::<u64>().map_err(|_| RealTextError::BadValue {
            key,
            value: v.to_owned(),
        })
    };
    let corpus_id = get("corpus_id")?;
    if !corpus_id
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    {
        return Err(RealTextError::BadValue {
            key: "corpus_id",
            value: corpus_id.to_owned(),
        });
    }
    let sha = get("sha256")?;
    if !is_pinned_hex(sha) {
        return Err(RealTextError::Unpinned);
    }
    let split_mod = num("split_mod")?;
    let eval_residue = num("eval_residue")?;
    if split_mod < 2 {
        return Err(RealTextError::BadValue {
            key: "split_mod",
            value: split_mod.to_string(),
        });
    }
    if eval_residue >= split_mod {
        return Err(RealTextError::BadValue {
            key: "eval_residue",
            value: eval_residue.to_string(),
        });
    }
    let byte_len = num("byte_len")?;
    // Protocol: corpus size must be > 0 (empty bytes + empty-hash is nonsense).
    if byte_len == 0 {
        return Err(RealTextError::BadValue {
            key: "byte_len",
            value: "0".to_owned(),
        });
    }
    Ok(RealTextManifest {
        corpus_id: corpus_id.to_owned(),
        source: get("source")?.to_owned(),
        license: get("license")?.to_owned(),
        sha256_hex: sha.to_owned(),
        byte_len,
        split_mod,
        eval_residue,
    })
}

/// Refuse absolute paths and any non-`Normal` component (`..` / root / prefix /
/// lone `.`). Empty paths are also refused. Mid-path `.` is elided by
/// `Path::components` and is therefore treated as a normal relative path.
/// Intended for CLI/opt-in file args before `fs::read`.
pub fn assert_safe_relative_path(path: &Path) -> Result<(), RealTextError> {
    let display = path.to_string_lossy().into_owned();
    if display.is_empty() {
        return Err(RealTextError::UnsafePath { path: display });
    }
    if path.is_absolute()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(RealTextError::UnsafePath { path: display });
    }
    Ok(())
}

/// Deterministic split bucket of a document id.
pub fn split_bucket(doc_id: &str, split_mod: u64) -> Result<u64, RealTextError> {
    let h = sha256_raw_bytes(doc_id.as_bytes()).map_err(|_| RealTextError::Hash)?;
    let mut first = [0_u8; 8];
    first.copy_from_slice(&h.bytes()[..8]);
    Ok(u64::from_be_bytes(first) % split_mod)
}

/// Fail-closed loader: verifies length + SHA-256 against the manifest before
/// parsing, then applies the split and exact-duplicate leakage rule.
pub fn load_pinned(
    manifest_text: &str,
    corpus_bytes: &[u8],
) -> Result<PinnedRealTextCorpus, RealTextError> {
    let manifest = parse_manifest(manifest_text)?;
    let actual_len = u64::try_from(corpus_bytes.len()).map_err(|_| RealTextError::Hash)?;
    if actual_len != manifest.byte_len {
        return Err(RealTextError::LengthMismatch {
            expected: manifest.byte_len,
            actual: actual_len,
        });
    }
    let actual = sha256_raw_bytes(corpus_bytes)
        .map_err(|_| RealTextError::Hash)?
        .hex();
    if actual != manifest.sha256_hex {
        return Err(RealTextError::ShaMismatch {
            expected: manifest.sha256_hex,
            actual,
        });
    }
    let text = std::str::from_utf8(corpus_bytes).map_err(|_| RealTextError::NotUtf8)?;
    let body = text
        .strip_suffix('\n')
        .ok_or(RealTextError::MissingTrailingNewline)?;
    let mut ids = BTreeSet::new();
    let (mut train, mut eval) = (Vec::new(), Vec::new());
    for (i, line) in body.split('\n').enumerate() {
        let (id, doc) = line
            .split_once('\t')
            .filter(|(id, doc)| {
                !id.is_empty() && !doc.is_empty() && !doc.contains('\t') && !doc.contains('\r')
            })
            .ok_or(RealTextError::MalformedDoc { line_no: i + 1 })?;
        if !ids.insert(id) {
            return Err(RealTextError::DuplicateDocId {
                doc_id: id.to_owned(),
            });
        }
        let d = RealTextDoc {
            doc_id: id.to_owned(),
            text: doc.to_owned(),
        };
        if split_bucket(id, manifest.split_mod)? == manifest.eval_residue {
            eval.push(d);
        } else {
            train.push(d);
        }
    }
    if train.is_empty() {
        return Err(RealTextError::EmptySplit { eval: false });
    }
    if eval.is_empty() {
        return Err(RealTextError::EmptySplit { eval: true });
    }
    let train_text: BTreeMap<&str, &str> = train
        .iter()
        .map(|d| (d.text.as_str(), d.doc_id.as_str()))
        .collect();
    for d in &eval {
        if let Some(tid) = train_text.get(d.text.as_str()) {
            return Err(RealTextError::Leakage {
                train_id: (*tid).to_owned(),
                eval_id: d.doc_id.clone(),
            });
        }
    }
    Ok(PinnedRealTextCorpus {
        manifest,
        train,
        eval,
    })
}
