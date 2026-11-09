//! CONTEXT-2 G19: adversarial drift tests (negative controls) against the
//! frozen G18 permutation / transcript guard and the reused G17 corpus on
//! the versioned anchor admission path
//! (`restore_into_anchored_with_versioned_provenance`).
//!
//! G18 (PR #895, exact head `8f5282b7`, squash `3ace36f9`) pins three
//! things: the G17 corpus digest, a SHA-256 over 8 SplitMix64-seeded
//! permutation orders, and a SHA-256 over the permuted-replay outcome
//! transcript, plus per-step oracle agreement and state invariants. G19
//! shows that each of those guards actually bites. It applies one
//! independent mutation at a time to a scenario (corpus rendering,
//! permutation orders, frozen expectations) and records which guards
//! reject it. The results form a frozen mutation-to-detector matrix:
//!
//! | mutation                        | detectors that reject it                                           |
//! |---------------------------------|--------------------------------------------------------------------|
//! | `unmutated_positive_control`    | none (everything accepts)                                          |
//! | `corpus_row_bytes_flip`         | `CorpusDigest`                                                     |
//! | `corpus_row_omitted`            | `CorpusDigest`, `PermutationShape`, `PermutationDigest`, `TranscriptDigest` |
//! | `permutation_index_overwrite`   | `PermutationShape`, `PermutationDigest`, `TranscriptDigest`        |
//! | `permutation_index_swap`        | `PermutationDigest`, `TranscriptDigest`                            |
//! | `duplicate_permutation`         | `PermutationDistinct`, `PermutationDigest`, `TranscriptDigest`     |
//! | `typed_error_expectation`       | `OracleAgreement`                                                  |
//! | `repeated_positive_control`     | `PermutationShape`, `PermutationDigest`, `TranscriptDigest`        |
//!
//! Notable negative result, kept on purpose: the state invariant
//! (`StateInvariant`) rejects none of these mutations. A repeated positive
//! control is idempotent on the live target, so only the frozen
//! permutation and transcript pins catch it.
//!
//! On every production rejection reached while replaying any scenario,
//! the shared target's canonical SWC2 encoding, SWE1 seal and all
//! `WorkingContextStats` are preserved byte-for-byte, and G17's adapter
//! asserts that the anchor state is never advanced.
//!
//! G19 changes no G17 / G18 fixture. It reads G17 through its existing
//! read-only `shared` module and re-implements the G18 renderers here. A
//! re-implemented renderer is accepted as the frozen guard only if it
//! reproduces G18's frozen digests on the unmutated positive control
//! (asserted below).
//!
//! Limits: authorities and anchors are in-process mocks, and a matching
//! authority tag is an equality binding, not authenticated provenance.
//! SWE1 is unkeyed SHA-256. These are harness-level guards, not production
//! admission gates. No migration, signature, attestation, transport,
//! remote execution or E2E claim. `NOT_MEASURED`. No production change.
//!
//! Assigned by ChatGPT on RemoteOps #1208 (comment 6728339742).

#[path = "working_context_anchor_composed_mutations.rs"]
mod g17;

use g17::shared;
use sml_core::elastic_ngram_paged_sha::sha256;
use sml_store::working_context::WorkingContext;

// --- G18 frozen constants, copied verbatim and re-proven below. ---------

const G18_CORPUS_VERSION: u16 = 1;
const G18_SEEDS: [u64; 8] = [
    0x4718_0000_0000_0001,
    0x4718_0000_0000_0002,
    0x4718_0000_0000_0003,
    0x4718_0000_0000_0005,
    0x4718_0000_0000_0008,
    0x4718_0000_0000_000D,
    0x4718_0000_0000_0015,
    0x4718_0000_0000_0022,
];
const G18_FROZEN_PERMUTATIONS_SHA256: &str =
    "b2b50f6c27f14fd4f3009ade800cf6fc7aa3633fb810b837a7564bce443edf4f";
const G18_FROZEN_TRANSCRIPT_SHA256: &str =
    "de2098e90bc23b72ecc839d60835ddb966f390014ef2770d1df66574658deaa1";

/// Version of this frozen G19 drift matrix.
const G19_MATRIX_VERSION: u16 = 1;

/// Frozen SHA-256 of the canonical G19 matrix rendering (mutation name,
/// detectors, and the digests each scenario produced).
const G19_FROZEN_MATRIX_SHA256: &str =
    "fc9dacbbb9569e0b7cc43919554d14dfd63cb98f2c1824776e250dafce11c9ff";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha_hex(s: &str) -> String {
    hex(&sha256(s.as_bytes()))
}

// --- Re-implemented G18 permutation generator and renderers. ------------

struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        let n64 = u64::try_from(n).expect("n fits u64");
        usize::try_from(self.next() % n64).expect("index fits usize")
    }
}

fn permutation(seed: u64, n: usize) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..n).collect();
    let mut rng = SplitMix64(seed);
    for i in (1..n).rev() {
        let j = rng.below(i + 1);
        idx.swap(i, j);
    }
    idx
}

fn frozen_permutations() -> Vec<Vec<usize>> {
    let n = shared::row_names().len();
    G18_SEEDS.iter().map(|&s| permutation(s, n)).collect()
}

fn render_permutations(perms: &[Vec<usize>]) -> String {
    let mut out = String::new();
    for (seed, p) in G18_SEEDS.iter().zip(perms) {
        let order: Vec<String> = p.iter().map(ToString::to_string).collect();
        out.push_str(&format!(
            "{G18_CORPUS_VERSION}|{seed:#018x}|{}\n",
            order.join(",")
        ));
    }
    out
}

type Fingerprint = (
    Vec<u8>,
    Vec<u8>,
    sml_store::working_context::WorkingContextStats,
);

// --- Scenario and detectors. -------------------------------------------

/// Everything a G18-style replay depends on; mutations edit exactly one
/// aspect of it.
#[derive(Clone)]
struct Scenario {
    rendering: String,
    perms: Vec<Vec<usize>>,
    /// Frozen expected outcome per row name, in corpus order.
    expected: Vec<(&'static str, String)>,
}

fn frozen_scenario() -> Scenario {
    Scenario {
        rendering: shared::rendering(),
        perms: frozen_permutations(),
        expected: shared::row_names()
            .into_iter()
            .map(|n| (n, shared::expected(n)))
            .collect(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Detector {
    /// G17 frozen corpus digest over the canonical rendering.
    CorpusDigest,
    /// Every order is a permutation of all corpus rows.
    PermutationShape,
    /// Orders are pairwise distinct and none is the identity.
    PermutationDistinct,
    /// G18 frozen permutation digest.
    PermutationDigest,
    /// Frozen expectation == independent oracle == production, per step.
    OracleAgreement,
    /// Rejections leave SWC2/SWE1/stats unchanged; acceptance installs Home3.
    StateInvariant,
    /// G18 frozen permuted-replay transcript digest.
    TranscriptDigest,
}

struct Evaluation {
    fired: Vec<Detector>,
    permutations_sha256: String,
    transcript_sha256: String,
    corpus_sha256: String,
    /// Production rejections reached during replay; each one was checked
    /// for byte-for-byte nonmutation of the shared target.
    rejections_checked: usize,
    nonmutation_violations: usize,
}

fn evaluate(sc: &Scenario) -> Evaluation {
    let names = shared::row_names();
    let n = names.len();
    let mut fired = Vec::new();

    let corpus_sha256 = sha_hex(&sc.rendering);
    if corpus_sha256 != shared::CORPUS_SHA256 {
        fired.push(Detector::CorpusDigest);
    }

    let identity: Vec<usize> = (0..n).collect();
    if sc.perms.iter().any(|p| {
        let mut s = p.clone();
        s.sort_unstable();
        s != identity
    }) {
        fired.push(Detector::PermutationShape);
    }
    let distinct = sc
        .perms
        .iter()
        .enumerate()
        .all(|(i, a)| a != &identity && sc.perms[i + 1..].iter().all(|b| a != b));
    if !distinct {
        fired.push(Detector::PermutationDistinct);
    }
    let permutations_sha256 = sha_hex(&render_permutations(&sc.perms));
    if permutations_sha256 != G18_FROZEN_PERMUTATIONS_SHA256 {
        fired.push(Detector::PermutationDigest);
    }

    let expected_of = |name: &str| -> &str {
        &sc.expected
            .iter()
            .find(|(n, _)| *n == name)
            .expect("expected entry")
            .1
    };
    let mut oracle_ok = true;
    let mut state_ok = true;
    let mut rejections_checked = 0;
    let mut nonmutation_violations = 0;
    let accepted: Fingerprint = shared::fingerprint_of(&shared::accepted_snapshot());
    let mut transcript = String::new();
    for (seed, p) in G18_SEEDS.iter().zip(&sc.perms) {
        let mut target: WorkingContext = shared::initial();
        for (pos, &i) in p.iter().enumerate() {
            let name = names[i];
            let before = shared::fingerprint_of(&target);
            let out = shared::production_outcome(&mut target, name);
            let oracle = shared::oracle_outcome(name);
            let exp = expected_of(name);
            if out != exp || oracle != exp || out != oracle {
                oracle_ok = false;
            }
            if shared::is_rejection(name) {
                rejections_checked += 1;
                if shared::fingerprint_of(&target) != before {
                    nonmutation_violations += 1;
                    state_ok = false;
                }
            } else if shared::fingerprint_of(&target) != accepted {
                state_ok = false;
            }
            transcript.push_str(&format!("{seed:#018x}|{pos}|{name}|{out}\n"));
        }
    }
    if !oracle_ok {
        fired.push(Detector::OracleAgreement);
    }
    if !state_ok {
        fired.push(Detector::StateInvariant);
    }
    let transcript_sha256 = sha_hex(&transcript);
    if transcript_sha256 != G18_FROZEN_TRANSCRIPT_SHA256 {
        fired.push(Detector::TranscriptDigest);
    }
    fired.sort_unstable();
    Evaluation {
        fired,
        permutations_sha256,
        transcript_sha256,
        corpus_sha256,
        rejections_checked,
        nonmutation_violations,
    }
}

// --- Frozen independent mutations. -------------------------------------

const ROW_UNDER_MUTATION: &str = "composed_g10_stream_g10_rollback";
const CONTROL: &str = "positive_control_accept";

fn row_index(name: &str) -> usize {
    shared::row_names()
        .iter()
        .position(|n| *n == name)
        .expect("row present")
}

fn mutate(name: &str) -> Scenario {
    let mut sc = frozen_scenario();
    match name {
        "unmutated_positive_control" => {}
        "corpus_row_bytes_flip" => {
            // Replace the bytes digest field of one corpus row by the
            // digest of the same bytes with the last bit flipped.
            let mut b = shared::bytes(ROW_UNDER_MUTATION);
            let last = b.len() - 1;
            b[last] ^= 1;
            let old = hex(&sha256(&shared::bytes(ROW_UNDER_MUTATION)));
            let new = hex(&sha256(&b));
            let line_marker = format!("|{ROW_UNDER_MUTATION}|");
            sc.rendering = sc
                .rendering
                .lines()
                .map(|l| {
                    if l.contains(&line_marker) {
                        l.replacen(&old, &new, 1)
                    } else {
                        l.to_string()
                    }
                })
                .map(|l| l + "\n")
                .collect();
        }
        "corpus_row_omitted" => {
            // The row disappears from the corpus and from every order.
            let line_marker = format!("|{ROW_UNDER_MUTATION}|");
            sc.rendering = sc
                .rendering
                .lines()
                .filter(|l| !l.contains(&line_marker))
                .map(|l| l.to_string() + "\n")
                .collect();
            let gone = row_index(ROW_UNDER_MUTATION);
            for p in &mut sc.perms {
                p.retain(|&i| i != gone);
            }
        }
        "permutation_index_overwrite" => {
            let p = &mut sc.perms[0];
            p[0] = p[1];
        }
        "permutation_index_swap" => {
            sc.perms[0].swap(0, 1);
        }
        "duplicate_permutation" => {
            sc.perms[1] = sc.perms[0].clone();
        }
        "typed_error_expectation" => {
            let other = shared::expected("composed_g12_stream_g10_stream");
            let e = sc
                .expected
                .iter_mut()
                .find(|(n, _)| *n == ROW_UNDER_MUTATION)
                .expect("row");
            assert_ne!(e.1, other, "mutation must change the expectation");
            e.1 = other;
        }
        "repeated_positive_control" => {
            // Overwrite the mutant slot right after the control with the
            // control itself, so the control runs twice in a row.
            let c = row_index(CONTROL);
            let p = &mut sc.perms[0];
            let pos = p.iter().position(|&i| i == c).expect("control");
            let next = if pos + 1 < p.len() { pos + 1 } else { pos - 1 };
            p[next] = c;
        }
        other => panic!("unknown mutation {other}"),
    }
    sc
}

use Detector::{
    CorpusDigest, OracleAgreement, PermutationDigest, PermutationDistinct, PermutationShape,
    TranscriptDigest,
};

/// Frozen mutation-to-detector matrix.
const MATRIX: [(&str, &[Detector]); 8] = [
    ("unmutated_positive_control", &[]),
    ("corpus_row_bytes_flip", &[CorpusDigest]),
    (
        "corpus_row_omitted",
        &[
            CorpusDigest,
            PermutationShape,
            PermutationDigest,
            TranscriptDigest,
        ],
    ),
    (
        "permutation_index_overwrite",
        &[PermutationShape, PermutationDigest, TranscriptDigest],
    ),
    (
        "permutation_index_swap",
        &[PermutationDigest, TranscriptDigest],
    ),
    (
        "duplicate_permutation",
        &[PermutationDistinct, PermutationDigest, TranscriptDigest],
    ),
    ("typed_error_expectation", &[OracleAgreement]),
    (
        "repeated_positive_control",
        &[PermutationShape, PermutationDigest, TranscriptDigest],
    ),
];

fn render_matrix() -> String {
    let mut out = String::new();
    for (name, _) in MATRIX {
        let ev = evaluate(&mutate(name));
        out.push_str(&format!(
            "{G19_MATRIX_VERSION}|{name}|{:?}|{}|{}|{}|{}\n",
            ev.fired,
            ev.corpus_sha256,
            ev.permutations_sha256,
            ev.transcript_sha256,
            ev.rejections_checked,
        ));
    }
    out
}

// --- Tests. ------------------------------------------------------------

#[test]
fn reimplemented_guards_reproduce_frozen_g17_g18_digests() {
    assert_eq!(shared::CORPUS_VERSION, 1, "pinned to G17 corpus v1");
    assert_eq!(sha_hex(&shared::rendering()), shared::CORPUS_SHA256);
    let ev = evaluate(&frozen_scenario());
    assert_eq!(ev.permutations_sha256, G18_FROZEN_PERMUTATIONS_SHA256);
    assert_eq!(ev.transcript_sha256, G18_FROZEN_TRANSCRIPT_SHA256);
    assert!(ev.fired.is_empty(), "positive control fired {:?}", ev.fired);
    assert_eq!(ev.nonmutation_violations, 0);
}

#[test]
fn each_mutation_is_rejected_by_exactly_its_frozen_detectors() {
    for (name, want) in MATRIX {
        let ev = evaluate(&mutate(name));
        assert_eq!(ev.fired, want.to_vec(), "{name}: detector set");
        if name != "unmutated_positive_control" {
            assert!(!ev.fired.is_empty(), "{name}: mutation must be rejected");
        }
        assert!(
            !ev.fired.contains(&Detector::StateInvariant),
            "{name}: state invariant unexpectedly fired"
        );
    }
}

#[test]
fn every_rejection_preserves_target_bytes_and_fixtures_stay_frozen() {
    for (name, _) in MATRIX {
        let ev = evaluate(&mutate(name));
        assert!(
            ev.rejections_checked > 0,
            "{name}: replay reached rejections"
        );
        assert_eq!(
            ev.nonmutation_violations, 0,
            "{name}: SWC2/SWE1/stats must be preserved on every rejection"
        );
    }
    // Mutating scenarios never touches the frozen G17/G18 sources.
    assert_eq!(sha_hex(&shared::rendering()), shared::CORPUS_SHA256);
    assert_eq!(
        sha_hex(&render_permutations(&frozen_permutations())),
        G18_FROZEN_PERMUTATIONS_SHA256
    );
    // Detector evaluation itself is side-effect free on a live target.
    let t = shared::initial();
    let before = shared::fingerprint_of(&t);
    let _ = render_matrix();
    assert_eq!(shared::fingerprint_of(&t), before);
}

#[test]
fn mutations_are_independent_and_deterministic() {
    let base = frozen_scenario();
    for (name, _) in MATRIX {
        let sc = mutate(name);
        let aspects = [
            sc.rendering != base.rendering,
            sc.perms != base.perms,
            sc.expected != base.expected,
        ];
        let changed = aspects.iter().filter(|b| **b).count();
        match name {
            "unmutated_positive_control" => assert_eq!(changed, 0),
            // Omitting a row necessarily edits both corpus and orders.
            "corpus_row_omitted" => assert_eq!(changed, 2),
            _ => assert_eq!(changed, 1, "{name}: one aspect only"),
        }
    }
    let rendered = render_matrix();
    assert_eq!(rendered, render_matrix(), "matrix is deterministic");
    assert_eq!(
        sha_hex(&rendered),
        G19_FROZEN_MATRIX_SHA256,
        "frozen G19 matrix digest (bump G19_MATRIX_VERSION and re-freeze on change)"
    );
}
