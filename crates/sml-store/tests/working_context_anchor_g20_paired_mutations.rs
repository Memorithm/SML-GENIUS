//! CONTEXT-2 G20: paired-perturbation completeness of the frozen
//! mutation-to-detector matrix (G19) over the frozen G18 permutation /
//! transcript guard and the frozen G17 corpus, on the versioned anchor
//! admission path (`restore_into_anchored_with_versioned_provenance`).
//!
//! G19 (PR #898, exact head `84edfa38`, squash `485a51a3`) applied one
//! mutation at a time and froze the detector set of each. G20 asks
//! whether that matrix stays *complete* when two mutations are applied
//! **together**: does the detector set of a pair equal the union of the
//! two single detector sets, or does one mutation mask (or add to) the
//! other? Every pair is applied in both orders.
//!
//! * **Exhaustive catalogue pairs**: all 28 unordered pairs (including the
//!   7 self-pairs) of the 7 G19 single mutations, grouped by aspect:
//!   corpus+permutation (8), expectation+permutation (4),
//!   corpus+expectation (2), permutation+permutation (10),
//!   corpus+corpus (3) and expectation+expectation (1).
//! * **Seeded pairs**: 48 deterministic SplitMix64-parameterised pairs
//!   (seed [`G20_SEED`]) alternating corpus+permutation and
//!   expectation+permutation, with seeded rows, permutations and
//!   positions.
//! * **Independent typed oracle**: [`predict`] derives the expected
//!   detector set and rejection count of any mutation sequence from typed
//!   facts (omitted / flipped row sets, name-level orders, an expectation
//!   map checked against G17's independent pure oracle). It never calls
//!   [`evaluate`] nor production, and must agree with the harness on every
//!   single, pair (both orders) and seeded pair.
//! * **Union / masking matrix**: every catalogue pair records `lost`
//!   (single detectors absent from the pair) and `gained` (pair detectors
//!   absent from both singles), whether the two orders commute, and
//!   whether the pair escapes all detectors. The frozen expected masking
//!   list is asserted exactly and the whole rendering is pinned by
//!   SHA-256 ([`G20_FROZEN_MATRIX_SHA256`]).
//! * **Undetected cases are negative results, kept on purpose** (see
//!   [`FROZEN_MASKED`] and [`FROZEN_UNDETECTED`]).
//!
//! Frozen results (matrix v1, SHA-256 `020d1f0d…`):
//!
//! * 23 of 28 catalogue pairs and 48 of 48 seeded pairs are exactly the
//!   union of their singles, in both orders; no pair ever *gains* a
//!   detector outside that union.
//! * 5 catalogue pairs mask a detector (negative results):
//!
//! | pair (a + b)                                            | lost (a then b)                        | lost (b then a)                        |
//! |---------------------------------------------------------|----------------------------------------|----------------------------------------|
//! | `corpus_row_omitted` + `typed_error_expectation`        | `OracleAgreement`                      | `OracleAgreement`                      |
//! | `permutation_index_overwrite` + `duplicate_permutation` | none                                   | `PermutationDistinct`                  |
//! | `permutation_index_swap` + `permutation_index_swap`     | `PermutationDigest`, `TranscriptDigest` | `PermutationDigest`, `TranscriptDigest` |
//! | `permutation_index_swap` + `duplicate_permutation`      | none                                   | `PermutationDistinct`                  |
//! | `duplicate_permutation` + `repeated_positive_control`   | `PermutationDistinct`                  | none                                   |
//!
//! * 1 catalogue pair is fully undetected: the swap self-pair, which is
//!   an involution and therefore net-null (asserted: the scenario equals
//!   the frozen one). 3 pairs are order-dependent (the duplicate rows).
//! * `StateInvariant` fires for no pair (as for every G19 single), and
//!   `PermutationDigest` / `TranscriptDigest` are never separated: with
//!   production fixed, the transcript changes iff an order changes.
//!
//! Before any of that is trusted, the re-implemented guards must
//! reproduce, on the unmutated positive control, the G17 corpus digest
//! (`15ceba9c…`), the G18 permutation and transcript digests
//! (`b2b50f6c…` / `de2098e9…`), and, with the re-implemented single
//! mutations, the G19 matrix digest (`fc9dacbb…`). The G17, G18 and G19
//! source files themselves are pinned by SHA-256, so any edit of a
//! frozen fixture fails this suite.
//!
//! On every production rejection reached in every evaluation, the shared
//! target's canonical SWC2 encoding, SWE1 seal and all
//! `WorkingContextStats` are preserved byte-for-byte, and G17's adapter
//! asserts that the anchor state is never advanced.
//!
//! Limits: mutations act on the *test-side* scenario (corpus rendering,
//! orders, expectations), exactly as in G19; production always replays
//! the frozen G17 rows. Authorities and anchors are in-process mocks; a
//! matching authority tag is an equality binding, not authenticated
//! provenance. SWE1 is unkeyed SHA-256. These are harness-level guards,
//! not production admission gates. No migration, signature, attestation,
//! transport, remote execution or E2E claim. `NOT_MEASURED`. No
//! production change.
//!
//! Assigned by ChatGPT on RemoteOps #1208 (comments 6729688978,
//! 6730382450).

#[path = "working_context_anchor_composed_mutations.rs"]
mod g17;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use g17::shared;
use sml_core::elastic_ngram_paged_sha::sha256;
use sml_store::working_context::WorkingContext;

// --- Frozen upstream pins (re-proven below). ------------------------------

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
const G17_FROZEN_CORPUS_SHA256: &str =
    "15ceba9cdb3ed42f92acd73bdf2208dcd7050298bd3c489a7f5a64ee0b298bb8";
const G18_FROZEN_PERMUTATIONS_SHA256: &str =
    "b2b50f6c27f14fd4f3009ade800cf6fc7aa3633fb810b837a7564bce443edf4f";
const G18_FROZEN_TRANSCRIPT_SHA256: &str =
    "de2098e90bc23b72ecc839d60835ddb966f390014ef2770d1df66574658deaa1";
const G19_MATRIX_VERSION: u16 = 1;
const G19_FROZEN_MATRIX_SHA256: &str =
    "fc9dacbbb9569e0b7cc43919554d14dfd63cb98f2c1824776e250dafce11c9ff";

/// SHA-256 of the frozen G17 / G18 / G19 test sources at main `071217ca`.
const FROZEN_SOURCES: [(&str, &[u8], &str); 3] = [
    (
        "working_context_anchor_composed_mutations.rs (G17)",
        include_bytes!("working_context_anchor_composed_mutations.rs"),
        "7d2d33725f9ba21b653976ee8328ad18cc810582a5bcb683a7ef9bccfb3c9cf7",
    ),
    (
        "working_context_anchor_permutation_invariance.rs (G18)",
        include_bytes!("working_context_anchor_permutation_invariance.rs"),
        "c09c812aefee49c111d1b8c2f2b9f8e590f0084b9243492a1976494d4a83b43c",
    ),
    (
        "working_context_anchor_g18_drift_negatives.rs (G19)",
        include_bytes!("working_context_anchor_g18_drift_negatives.rs"),
        "23f09842c8bcfe580c48479ab87e3650dcd190d7f7a1b39df97d01691ea69a7f",
    ),
];

// --- G20 frozen constants. ------------------------------------------------

/// Version of this frozen G20 paired-mutation matrix.
const G20_MATRIX_VERSION: u16 = 1;
/// SplitMix64 seed of the parameterised (seeded) pairs.
const G20_SEED: u64 = 0x4720_0000_0000_0020;
/// Number of seeded pairs (alternating corpus+perm / expectation+perm).
const G20_SEEDED_PAIRS: usize = 48;
/// Frozen SHA-256 of the canonical G20 matrix rendering.
const G20_FROZEN_MATRIX_SHA256: &str =
    "020d1f0de9fa35a358f44d2553234d2d5cb912d425d69732e41df609d7d7911f";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha_hex(s: &str) -> String {
    hex(&sha256(s.as_bytes()))
}

// --- Re-implemented G18 generator and renderer (re-proven). -------------

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

// --- Scenario and harness detectors (G19 semantics). -------------------

#[derive(Clone, PartialEq, Eq)]
struct Scenario {
    rendering: String,
    perms: Vec<Vec<usize>>,
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

/// Same variants, order and `Debug` rendering as G19's detector enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Detector {
    CorpusDigest,
    PermutationShape,
    PermutationDistinct,
    PermutationDigest,
    OracleAgreement,
    StateInvariant,
    TranscriptDigest,
}

use Detector::{
    CorpusDigest, OracleAgreement, PermutationDigest, PermutationDistinct, PermutationShape,
    TranscriptDigest,
};

#[derive(Clone)]
struct Evaluation {
    fired: Vec<Detector>,
    corpus_sha256: String,
    permutations_sha256: String,
    transcript_sha256: String,
    rejections_checked: usize,
    nonmutation_violations: usize,
}

fn evaluate(sc: &Scenario) -> Evaluation {
    let names = shared::row_names();
    let n = names.len();
    let mut fired = Vec::new();

    let corpus_sha256 = sha_hex(&sc.rendering);
    if corpus_sha256 != shared::CORPUS_SHA256 {
        fired.push(CorpusDigest);
    }
    let identity: Vec<usize> = (0..n).collect();
    if sc.perms.iter().any(|p| {
        let mut s = p.clone();
        s.sort_unstable();
        s != identity
    }) {
        fired.push(PermutationShape);
    }
    let distinct = sc
        .perms
        .iter()
        .enumerate()
        .all(|(i, a)| a != &identity && sc.perms[i + 1..].iter().all(|b| a != b));
    if !distinct {
        fired.push(PermutationDistinct);
    }
    let permutations_sha256 = sha_hex(&render_permutations(&sc.perms));
    if permutations_sha256 != G18_FROZEN_PERMUTATIONS_SHA256 {
        fired.push(PermutationDigest);
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
        fired.push(OracleAgreement);
    }
    if !state_ok {
        fired.push(Detector::StateInvariant);
    }
    let transcript_sha256 = sha_hex(&transcript);
    if transcript_sha256 != G18_FROZEN_TRANSCRIPT_SHA256 {
        fired.push(TranscriptDigest);
    }
    fired.sort_unstable();
    Evaluation {
        fired,
        corpus_sha256,
        permutations_sha256,
        transcript_sha256,
        rejections_checked,
        nonmutation_violations,
    }
}

// --- Typed mutations. ----------------------------------------------------

const ROW_UNDER_MUTATION: &str = "composed_g10_stream_g10_rollback";
const RETARGET_SOURCE: &str = "composed_g12_stream_g10_stream";
const CONTROL: &str = "positive_control_accept";

fn row_index(name: &str) -> usize {
    shared::row_names()
        .iter()
        .position(|n| *n == name)
        .expect("row present")
}

/// One typed, parameterised mutation. Rows and permutations are indices
/// into the frozen G17 corpus order and the frozen G18 seed list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mutation {
    /// Corpus: the bytes-digest field of `row` is replaced by the digest
    /// of the same bytes with the last bit flipped (no-op if absent).
    CorpusBytesFlip { row: usize },
    /// Corpus: `row` disappears from the rendering and from every order.
    CorpusRowOmitted { row: usize },
    /// Permutation: `perm[pos] = perm[pos + 1]` (positions wrap).
    PermIndexOverwrite { perm: usize, pos: usize },
    /// Permutation: swap `perm[pos]` and `perm[pos + 1]` (positions wrap).
    PermIndexSwap { perm: usize, pos: usize },
    /// Permutation: order `dst` becomes a copy of order `src`.
    PermDuplicate { src: usize, dst: usize },
    /// Permutation: the slot after the control (or before, if last)
    /// becomes the control (no-op if the control is absent).
    RepeatControl { perm: usize },
    /// Expectation: frozen expectation of `row` := that of `to`.
    ExpectationRetarget { row: usize, to: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Aspect {
    Corpus,
    Expectation,
    Permutation,
}

impl Mutation {
    fn aspect(self) -> Aspect {
        match self {
            Mutation::CorpusBytesFlip { .. } | Mutation::CorpusRowOmitted { .. } => Aspect::Corpus,
            Mutation::ExpectationRetarget { .. } => Aspect::Expectation,
            _ => Aspect::Permutation,
        }
    }
}

fn wrap_pos(pos: usize, len: usize) -> usize {
    pos % (len - 1)
}

fn corpus_line_marker(row: usize) -> String {
    format!("|{}|", shared::row_names()[row])
}

fn apply(sc: &mut Scenario, m: Mutation) {
    match m {
        Mutation::CorpusBytesFlip { row } => {
            let name = shared::row_names()[row];
            let mut b = shared::bytes(name);
            let last = b.len() - 1;
            b[last] ^= 1;
            let old = hex(&sha256(&shared::bytes(name)));
            let new = hex(&sha256(&b));
            let marker = corpus_line_marker(row);
            sc.rendering = sc
                .rendering
                .lines()
                .map(|l| {
                    if l.contains(&marker) {
                        l.replacen(&old, &new, 1)
                    } else {
                        l.to_string()
                    }
                })
                .map(|l| l + "\n")
                .collect();
        }
        Mutation::CorpusRowOmitted { row } => {
            let marker = corpus_line_marker(row);
            sc.rendering = sc
                .rendering
                .lines()
                .filter(|l| !l.contains(&marker))
                .map(|l| l.to_string() + "\n")
                .collect();
            for p in &mut sc.perms {
                p.retain(|&i| i != row);
            }
        }
        Mutation::PermIndexOverwrite { perm, pos } => {
            let p = &mut sc.perms[perm];
            let pos = wrap_pos(pos, p.len());
            p[pos] = p[pos + 1];
        }
        Mutation::PermIndexSwap { perm, pos } => {
            let p = &mut sc.perms[perm];
            let pos = wrap_pos(pos, p.len());
            p.swap(pos, pos + 1);
        }
        Mutation::PermDuplicate { src, dst } => {
            sc.perms[dst] = sc.perms[src].clone();
        }
        Mutation::RepeatControl { perm } => {
            let c = row_index(CONTROL);
            let p = &mut sc.perms[perm];
            if let Some(pos) = p.iter().position(|&i| i == c) {
                let next = if pos + 1 < p.len() { pos + 1 } else { pos - 1 };
                p[next] = c;
            }
        }
        Mutation::ExpectationRetarget { row, to } => {
            let name = shared::row_names()[row];
            let other = shared::expected(shared::row_names()[to]);
            let e = sc
                .expected
                .iter_mut()
                .find(|(n, _)| *n == name)
                .expect("row");
            e.1 = other;
        }
    }
}

fn scenario_of(muts: &[Mutation]) -> Scenario {
    let mut sc = frozen_scenario();
    for &m in muts {
        apply(&mut sc, m);
    }
    sc
}

/// The seven G19 single mutations, as typed mutations, in G19 order.
fn catalogue() -> Vec<(&'static str, Mutation)> {
    let row = row_index(ROW_UNDER_MUTATION);
    vec![
        ("corpus_row_bytes_flip", Mutation::CorpusBytesFlip { row }),
        ("corpus_row_omitted", Mutation::CorpusRowOmitted { row }),
        (
            "permutation_index_overwrite",
            Mutation::PermIndexOverwrite { perm: 0, pos: 0 },
        ),
        (
            "permutation_index_swap",
            Mutation::PermIndexSwap { perm: 0, pos: 0 },
        ),
        (
            "duplicate_permutation",
            Mutation::PermDuplicate { src: 0, dst: 1 },
        ),
        (
            "typed_error_expectation",
            Mutation::ExpectationRetarget {
                row,
                to: row_index(RETARGET_SOURCE),
            },
        ),
        (
            "repeated_positive_control",
            Mutation::RepeatControl { perm: 0 },
        ),
    ]
}

// --- Independent typed oracle. -------------------------------------------

/// What the typed oracle predicts for a mutation sequence.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Prediction {
    fired: Vec<Detector>,
    rejections: usize,
}

/// Predicts detectors and rejection count from typed facts only. It does
/// not call [`evaluate`], production, or any digest renderer:
///
/// * corpus facts are the sets of omitted and flipped rows (a flip of an
///   already-omitted or already-flipped row is a no-op, because the
///   frozen digest it replaces is gone);
/// * orders are tracked as **name** sequences and judged by multiset
///   counts against the frozen corpus name set;
/// * expectations are a name → outcome map judged against G17's
///   independent pure oracle (not production), and only for rows that
///   are still replayed somewhere;
/// * permutation and transcript digests change iff some name sequence
///   differs from the frozen one (G18: outcomes depend on the row only);
/// * the state invariant is predicted never to fire, because production
///   always replays the frozen rows (kept as an explicit negative).
fn predict(muts: &[Mutation]) -> Prediction {
    let names = shared::row_names();
    let frozen_orders: Vec<Vec<&str>> = frozen_permutations()
        .iter()
        .map(|p| p.iter().map(|&i| names[i]).collect())
        .collect();
    let mut orders = frozen_orders.clone();
    let mut omitted: BTreeSet<&str> = BTreeSet::new();
    let mut flipped: BTreeSet<&str> = BTreeSet::new();
    let mut expect: BTreeMap<&str, String> =
        names.iter().map(|&n| (n, shared::expected(n))).collect();

    for &m in muts {
        match m {
            Mutation::CorpusBytesFlip { row } => {
                let n = names[row];
                if !omitted.contains(n) {
                    flipped.insert(n);
                }
            }
            Mutation::CorpusRowOmitted { row } => {
                let n = names[row];
                omitted.insert(n);
                for o in &mut orders {
                    o.retain(|x| *x != n);
                }
            }
            Mutation::PermIndexOverwrite { perm, pos } => {
                let o = &mut orders[perm];
                let k = pos % (o.len() - 1);
                o[k] = o[k + 1];
            }
            Mutation::PermIndexSwap { perm, pos } => {
                let o = &mut orders[perm];
                let k = pos % (o.len() - 1);
                o.swap(k, k + 1);
            }
            Mutation::PermDuplicate { src, dst } => {
                orders[dst] = orders[src].clone();
            }
            Mutation::RepeatControl { perm } => {
                let o = &mut orders[perm];
                if let Some(k) = o.iter().position(|x| *x == CONTROL) {
                    let t = if k + 1 < o.len() { k + 1 } else { k - 1 };
                    o[t] = CONTROL;
                }
            }
            Mutation::ExpectationRetarget { row, to } => {
                expect.insert(names[row], shared::expected(names[to]));
            }
        }
    }

    let mut fired = BTreeSet::new();
    if !omitted.is_empty() || !flipped.is_empty() {
        fired.insert(CorpusDigest);
    }
    let universe: BTreeMap<&str, usize> = names.iter().map(|&n| (n, 1)).collect();
    if orders.iter().any(|o| {
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for &x in o {
            *counts.entry(x).or_default() += 1;
        }
        counts != universe
    }) {
        fired.insert(PermutationShape);
    }
    let corpus_order: Vec<&str> = names.clone();
    let mut seen: BTreeSet<&Vec<&str>> = BTreeSet::new();
    let distinct = orders.iter().all(|o| *o != corpus_order && seen.insert(o));
    if !distinct {
        fired.insert(PermutationDistinct);
    }
    if orders != frozen_orders {
        fired.insert(PermutationDigest);
        fired.insert(TranscriptDigest);
    }
    let replayed: BTreeSet<&str> = orders.iter().flatten().copied().collect();
    if replayed
        .iter()
        .any(|n| expect[n] != shared::oracle_outcome(n))
    {
        fired.insert(OracleAgreement);
    }
    let rejections = orders
        .iter()
        .flatten()
        .filter(|n| shared::is_rejection(n))
        .count();
    Prediction {
        fired: fired.into_iter().collect(),
        rejections,
    }
}

// --- Seeded pairs. ---------------------------------------------------------

fn seeded_pairs() -> Vec<(Mutation, Mutation)> {
    let names = shared::row_names();
    let n = names.len();
    let k = G18_SEEDS.len();
    let mut rng = SplitMix64(G20_SEED);
    let mut out = Vec::with_capacity(G20_SEEDED_PAIRS);
    for i in 0..G20_SEEDED_PAIRS {
        let first = if i % 2 == 0 {
            let row = rng.below(n);
            if rng.below(2) == 0 {
                Mutation::CorpusBytesFlip { row }
            } else {
                Mutation::CorpusRowOmitted { row }
            }
        } else {
            let row = rng.below(n);
            // First row (cyclically after a seeded offset) whose frozen
            // expectation differs, so the retarget is a real change.
            let start = rng.below(n);
            let to = (0..n)
                .map(|d| (start + d) % n)
                .find(|&t| shared::expected(names[t]) != shared::expected(names[row]))
                .expect("a differing expectation exists");
            Mutation::ExpectationRetarget { row, to }
        };
        let perm = rng.below(k);
        let second = match rng.below(4) {
            0 => Mutation::PermIndexOverwrite {
                perm,
                pos: rng.below(n),
            },
            1 => Mutation::PermIndexSwap {
                perm,
                pos: rng.below(n),
            },
            2 => {
                let dst = (perm + 1 + rng.below(k - 1)) % k;
                Mutation::PermDuplicate { src: perm, dst }
            }
            _ => Mutation::RepeatControl { perm },
        };
        out.push((first, second));
    }
    out
}

// --- Matrix. ---------------------------------------------------------------

fn set(v: &[Detector]) -> BTreeSet<Detector> {
    v.iter().copied().collect()
}

struct PairRow {
    category: String,
    a: &'static str,
    b: &'static str,
    single_a: Vec<Detector>,
    single_b: Vec<Detector>,
    ab: Evaluation,
    ba: Evaluation,
    delta: Delta,
}

impl PairRow {
    fn commutes(&self) -> bool {
        self.ab.fired == self.ba.fired
    }
    fn undetected(&self) -> bool {
        self.ab.fired.is_empty() || self.ba.fired.is_empty()
    }
}

fn category(a: Mutation, b: Mutation) -> String {
    let (x, y) = if a.aspect() <= b.aspect() {
        (a.aspect(), b.aspect())
    } else {
        (b.aspect(), a.aspect())
    };
    format!("{x:?}+{y:?}").to_lowercase()
}

/// Per-order lost detectors (in the union of the singles but absent from
/// that order's pair set) and gained detectors (in either order's pair
/// set but in neither single).
fn union_delta(sa: &[Detector], sb: &[Detector], ab: &[Detector], ba: &[Detector]) -> Delta {
    let union: BTreeSet<Detector> = set(sa).union(&set(sb)).copied().collect();
    let pair_any: BTreeSet<Detector> = set(ab).union(&set(ba)).copied().collect();
    Delta {
        lost_ab: union.difference(&set(ab)).copied().collect(),
        lost_ba: union.difference(&set(ba)).copied().collect(),
        gained: pair_any.difference(&union).copied().collect(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Delta {
    lost_ab: Vec<Detector>,
    lost_ba: Vec<Detector>,
    gained: Vec<Detector>,
}

impl Delta {
    fn is_union(&self) -> bool {
        self.lost_ab.is_empty() && self.lost_ba.is_empty() && self.gained.is_empty()
    }
}

struct Matrix {
    control: Evaluation,
    singles: Vec<(&'static str, Mutation, Evaluation)>,
    pairs: Vec<PairRow>,
    seeded: Vec<SeededRow>,
}

struct SeededRow {
    a: Mutation,
    b: Mutation,
    single_a: Evaluation,
    single_b: Evaluation,
    ab: Evaluation,
    ba: Evaluation,
    delta: Delta,
}

fn matrix() -> &'static Matrix {
    static M: OnceLock<Matrix> = OnceLock::new();
    M.get_or_init(|| {
        let control = evaluate(&frozen_scenario());
        let cat = catalogue();
        let singles: Vec<_> = cat
            .iter()
            .map(|&(name, m)| (name, m, evaluate(&scenario_of(&[m]))))
            .collect();
        let mut pairs = Vec::new();
        for i in 0..singles.len() {
            for j in i..singles.len() {
                let (na, ma, ea) = &singles[i];
                let (nb, mb, eb) = &singles[j];
                let ab = evaluate(&scenario_of(&[*ma, *mb]));
                let ba = evaluate(&scenario_of(&[*mb, *ma]));
                let delta = union_delta(&ea.fired, &eb.fired, &ab.fired, &ba.fired);
                pairs.push(PairRow {
                    category: category(*ma, *mb),
                    a: na,
                    b: nb,
                    single_a: ea.fired.clone(),
                    single_b: eb.fired.clone(),
                    ab,
                    ba,
                    delta,
                });
            }
        }
        let seeded = seeded_pairs()
            .into_iter()
            .map(|(a, b)| {
                let single_a = evaluate(&scenario_of(&[a]));
                let single_b = evaluate(&scenario_of(&[b]));
                let ab = evaluate(&scenario_of(&[a, b]));
                let ba = evaluate(&scenario_of(&[b, a]));
                let delta = union_delta(&single_a.fired, &single_b.fired, &ab.fired, &ba.fired);
                SeededRow {
                    a,
                    b,
                    single_a,
                    single_b,
                    ab,
                    ba,
                    delta,
                }
            })
            .collect();
        Matrix {
            control,
            singles,
            pairs,
            seeded,
        }
    })
}

/// G19's exact rendering, recomputed from the re-implemented singles.
fn render_g19_matrix(m: &Matrix) -> String {
    let mut out = String::new();
    let mut rows: Vec<(&str, &Evaluation)> = vec![("unmutated_positive_control", &m.control)];
    rows.extend(m.singles.iter().map(|(n, _, e)| (*n, e)));
    // G19 lists repeated_positive_control last, after typed_error_expectation:
    // same order as `catalogue()`.
    for (name, ev) in rows {
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

fn render_g20_matrix(m: &Matrix) -> String {
    let mut out = String::new();
    for p in &m.pairs {
        out.push_str(&format!(
            "{G20_MATRIX_VERSION}|pair|{}|{}+{}|a={:?}|b={:?}|ab={:?}|ba={:?}|lost_ab={:?}|lost_ba={:?}|gained={:?}|commutes={}|undetected={}|{}|{}|{}|{}|{}\n",
            p.category,
            p.a,
            p.b,
            p.single_a,
            p.single_b,
            p.ab.fired,
            p.ba.fired,
            p.delta.lost_ab,
            p.delta.lost_ba,
            p.delta.gained,
            p.commutes(),
            p.undetected(),
            p.ab.corpus_sha256,
            p.ab.permutations_sha256,
            p.ab.transcript_sha256,
            p.ab.rejections_checked,
            p.ba.rejections_checked,
        ));
    }
    for (i, r) in m.seeded.iter().enumerate() {
        out.push_str(&format!(
            "{G20_MATRIX_VERSION}|seeded|{G20_SEED:#018x}|{i}|{}|{:?}|{:?}|a={:?}|b={:?}|ab={:?}|ba={:?}|lost_ab={:?}|lost_ba={:?}|gained={:?}|{}|{}\n",
            category(r.a, r.b),
            r.a,
            r.b,
            r.single_a.fired,
            r.single_b.fired,
            r.ab.fired,
            r.ba.fired,
            r.delta.lost_ab,
            r.delta.lost_ba,
            r.delta.gained,
            r.ab.rejections_checked,
            r.ba.rejections_checked,
        ));
    }
    out
}

// --- Frozen expected masking results (negative results, kept). -----------

/// Catalogue pairs `(a, b)` whose detector set is NOT the union of the
/// singles: `(a, b, lost when a then b, lost when b then a, gained)`.
/// Negative results, kept on purpose.
#[allow(clippy::type_complexity)]
const FROZEN_MASKED: &[(&str, &str, &[Detector], &[Detector], &[Detector])] = &[
    // The retargeted expectation belongs to the omitted row, which is
    // never replayed: OracleAgreement is masked in both orders (the
    // corpus / permutation detectors still reject the pair).
    (
        "corpus_row_omitted",
        "typed_error_expectation",
        &[OracleAgreement],
        &[OracleAgreement],
        &[],
    ),
    // Duplicating order 0 into order 1 *before* order 0 is edited leaves
    // two distinct orders: PermutationDistinct is masked in that order only.
    (
        "permutation_index_overwrite",
        "duplicate_permutation",
        &[],
        &[PermutationDistinct],
        &[],
    ),
    // The swap is an involution: applied twice it restores the frozen
    // orders, so the pair is net-null and nothing fires (undetected).
    (
        "permutation_index_swap",
        "permutation_index_swap",
        &[PermutationDigest, TranscriptDigest],
        &[PermutationDigest, TranscriptDigest],
        &[],
    ),
    (
        "permutation_index_swap",
        "duplicate_permutation",
        &[],
        &[PermutationDistinct],
        &[],
    ),
    (
        "duplicate_permutation",
        "repeated_positive_control",
        &[PermutationDistinct],
        &[],
        &[],
    ),
];

/// Catalogue pairs that escape every detector in at least one order.
const FROZEN_UNDETECTED: &[(&str, &str)] = &[("permutation_index_swap", "permutation_index_swap")];

/// Catalogue pairs whose detector set depends on application order.
const FROZEN_NON_COMMUTING: &[(&str, &str)] = &[
    ("permutation_index_overwrite", "duplicate_permutation"),
    ("permutation_index_swap", "duplicate_permutation"),
    ("duplicate_permutation", "repeated_positive_control"),
];

/// Seeded pairs that are not the exact union of their singles.
const FROZEN_SEEDED_MASKED: usize = 0;

// --- Tests. ----------------------------------------------------------------

#[test]
fn frozen_g17_g18_g19_sources_and_digests_are_reproduced_on_the_control() {
    for (what, bytes, want) in FROZEN_SOURCES {
        assert_eq!(hex(&sha256(bytes)), want, "{what} must stay frozen");
    }
    assert_eq!(shared::CORPUS_VERSION, 1, "pinned to G17 corpus v1");
    assert_eq!(shared::CORPUS_SHA256, G17_FROZEN_CORPUS_SHA256);
    let m = matrix();
    assert_eq!(m.control.corpus_sha256, G17_FROZEN_CORPUS_SHA256);
    assert_eq!(
        m.control.permutations_sha256,
        G18_FROZEN_PERMUTATIONS_SHA256
    );
    assert_eq!(m.control.transcript_sha256, G18_FROZEN_TRANSCRIPT_SHA256);
    assert!(
        m.control.fired.is_empty(),
        "positive control fired {:?}",
        m.control.fired
    );
    assert_eq!(m.control.nonmutation_violations, 0);
    assert_eq!(
        sha_hex(&render_g19_matrix(m)),
        G19_FROZEN_MATRIX_SHA256,
        "re-implemented singles must reproduce the frozen G19 matrix"
    );
}

#[test]
fn typed_oracle_agrees_with_harness_on_control_singles_pairs_and_seeded_pairs() {
    let m = matrix();
    let check = |muts: &[Mutation], ev: &Evaluation| {
        let p = predict(muts);
        assert_eq!(
            ev.fired, p.fired,
            "{muts:?}: detector set (oracle vs harness)"
        );
        assert_eq!(ev.rejections_checked, p.rejections, "{muts:?}: rejections");
    };
    check(&[], &m.control);
    for (_, mu, ev) in &m.singles {
        check(&[*mu], ev);
    }
    let cat = catalogue();
    let mut k = 0;
    for i in 0..cat.len() {
        for j in i..cat.len() {
            let p = &m.pairs[k];
            check(&[cat[i].1, cat[j].1], &p.ab);
            check(&[cat[j].1, cat[i].1], &p.ba);
            k += 1;
        }
    }
    assert_eq!(k, 28);
    assert_eq!(m.seeded.len(), G20_SEEDED_PAIRS);
    for r in &m.seeded {
        check(&[r.a], &r.single_a);
        check(&[r.b], &r.single_b);
        check(&[r.a, r.b], &r.ab);
        check(&[r.b, r.a], &r.ba);
    }
}

#[test]
fn paired_matrix_union_masking_and_undetected_cases_are_frozen() {
    let m = matrix();
    let masked: Vec<(&str, &str, Delta)> = m
        .pairs
        .iter()
        .filter(|p| !p.delta.is_union())
        .map(|p| (p.a, p.b, p.delta.clone()))
        .collect();
    let want: Vec<(&str, &str, Delta)> = FROZEN_MASKED
        .iter()
        .map(|(a, b, lab, lba, g)| {
            (
                *a,
                *b,
                Delta {
                    lost_ab: lab.to_vec(),
                    lost_ba: lba.to_vec(),
                    gained: g.to_vec(),
                },
            )
        })
        .collect();
    assert_eq!(masked, want, "frozen masking list");
    assert_eq!(
        m.pairs.iter().filter(|p| p.delta.is_union()).count(),
        28 - FROZEN_MASKED.len(),
        "every other catalogue pair is exactly the union of its singles"
    );
    assert!(
        m.pairs.iter().all(|p| p.delta.gained.is_empty()),
        "no pair adds a detector outside the union of its singles"
    );

    let undetected: Vec<(&str, &str)> = m
        .pairs
        .iter()
        .filter(|p| p.undetected())
        .map(|p| (p.a, p.b))
        .collect();
    assert_eq!(
        undetected,
        FROZEN_UNDETECTED.to_vec(),
        "frozen undetected list"
    );
    // An undetected pair must be a net-null mutation (the scenario is the
    // frozen one again), otherwise it would be a guard escape.
    let cat = catalogue();
    let by = |n: &str| cat.iter().find(|(c, _)| *c == n).expect("catalogue").1;
    for (a, b) in FROZEN_UNDETECTED {
        let sc = scenario_of(&[by(a), by(b)]);
        let rev = scenario_of(&[by(b), by(a)]);
        assert!(
            sc == frozen_scenario() || rev == frozen_scenario(),
            "{a}+{b}: undetected pair is not net-null"
        );
    }

    let non_commuting: Vec<(&str, &str)> = m
        .pairs
        .iter()
        .filter(|p| !p.commutes())
        .map(|p| (p.a, p.b))
        .collect();
    assert_eq!(
        non_commuting,
        FROZEN_NON_COMMUTING.to_vec(),
        "frozen order dependence"
    );

    // Seeded corpus+permutation and expectation+permutation pairs: all are
    // exact unions of their singles, in both orders.
    let seeded_masked = m.seeded.iter().filter(|r| !r.delta.is_union()).count();
    assert_eq!(seeded_masked, FROZEN_SEEDED_MASKED);
    assert!(m
        .seeded
        .iter()
        .all(|r| !r.ab.fired.is_empty() && !r.ba.fired.is_empty()));

    // Required categories are present with the expected sizes.
    let count = |c: &str| m.pairs.iter().filter(|p| p.category == c).count();
    assert_eq!(count("corpus+permutation"), 8);
    assert_eq!(count("expectation+permutation"), 4);
    assert_eq!(count("corpus+expectation"), 2);
    assert_eq!(count("permutation+permutation"), 10);
    assert_eq!(count("corpus+corpus"), 3);
    assert_eq!(count("expectation+expectation"), 1);
    let seeded_count = |c: &str| m.seeded.iter().filter(|r| category(r.a, r.b) == c).count();
    assert_eq!(seeded_count("corpus+permutation"), G20_SEEDED_PAIRS / 2);
    assert_eq!(
        seeded_count("expectation+permutation"),
        G20_SEEDED_PAIRS / 2
    );

    // Negative result kept: the state invariant never fires, paired or not.
    for p in &m.pairs {
        assert!(!p.ab.fired.contains(&Detector::StateInvariant));
        assert!(!p.ba.fired.contains(&Detector::StateInvariant));
    }
    for r in &m.seeded {
        assert!(!r.ab.fired.contains(&Detector::StateInvariant));
        assert!(!r.ba.fired.contains(&Detector::StateInvariant));
    }
}

#[test]
fn every_production_rejection_preserves_target_bytes_in_every_evaluation() {
    let m = matrix();
    let mut evs: Vec<&Evaluation> = vec![&m.control];
    evs.extend(m.singles.iter().map(|(_, _, e)| e));
    for p in &m.pairs {
        evs.push(&p.ab);
        evs.push(&p.ba);
    }
    for r in &m.seeded {
        evs.extend([&r.single_a, &r.single_b, &r.ab, &r.ba]);
    }
    let mut total = 0;
    for ev in &evs {
        assert!(ev.rejections_checked > 0, "replay reached rejections");
        assert_eq!(
            ev.nonmutation_violations, 0,
            "SWC2/SWE1/stats must be preserved on every rejection"
        );
        total += ev.rejections_checked;
    }
    eprintln!(
        "G20: {} evaluations, {total} production rejections checked",
        evs.len()
    );
    // Mutating scenarios never touches the frozen G17/G18 sources.
    assert_eq!(sha_hex(&shared::rendering()), G17_FROZEN_CORPUS_SHA256);
    assert_eq!(
        sha_hex(&render_permutations(&frozen_permutations())),
        G18_FROZEN_PERMUTATIONS_SHA256
    );
}

#[test]
fn g20_matrix_is_deterministic_and_pinned() {
    let m = matrix();
    let rendered = render_g20_matrix(m);
    assert_eq!(
        seeded_pairs(),
        seeded_pairs(),
        "seeded pairs are deterministic"
    );
    let again = evaluate(&scenario_of(&[seeded_pairs()[0].0, seeded_pairs()[0].1]));
    assert_eq!(again.fired, m.seeded[0].ab.fired);
    assert_eq!(
        sha_hex(&rendered),
        G20_FROZEN_MATRIX_SHA256,
        "frozen G20 matrix digest (bump G20_MATRIX_VERSION and re-freeze on change)"
    );
}
