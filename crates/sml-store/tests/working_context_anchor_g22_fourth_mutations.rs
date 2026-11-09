//! CONTEXT-2 G22: metamorphic fourth-mutation falsification over the
//! frozen G17–G21 detector matrices (G17 corpus, G18 permutation /
//! transcript guard, G19 singles, G20 pairs, G21 triples), on the
//! versioned anchor admission path
//! (`restore_into_anchored_with_versioned_provenance`).
//!
//! G21 (PR #902, squash `14b8ce6c`; report `106898cb`) found that every
//! independent third composes with its G20 pair as a plain union, and kept
//! 41 surviving masks (triples that are not the union of their singles),
//! 24 of them order-dependent. G22 tries to **falsify** that composition
//! law with a fourth mutation `d`, inserted at every position of every
//! G21 relative order of `a, b, c` (6 × 4 = all 24 orders).
//!
//! Pre-registered plan (pinned by [`G22_FROZEN_PLAN_SHA256`] before any
//! result is asserted):
//!
//! * **Bases:** every surviving G21 mask ([`surviving_masks`]; it contains
//!   all 24 G21 order-dependent triples).
//! * **Catalogue quadruples** (120): the 24 catalogue / net-null bases ×
//!   five fourths ([`fourths`]): two *independent* (expectation retarget
//!   on a free row, swap on the free order 4) and three *interacting*
//!   (swap on order 0, order 1 copied onto order 0, restore of the G19
//!   row's expectation).
//! * **Seeded quadruples** (16): SplitMix64 seed [`G22_SEED`] picks a base
//!   among all 41 masks and a fourth of any kind on any order / row.
//! * **Positive control:** the unmutated scenario fires nothing; a
//!   detection control of four mutually independent mutations is the
//!   exact union of its singles in all 24 orders.
//! * **Net-null negatives** (2), kept on purpose.
//! * **Metamorphic relations:** MR-1 — each order fires exactly
//!   `triple(relative order) ∪ d`; MR-2 — the set does not depend on the
//!   insertion position of `d`. Falsifications are enumerated, not hidden.
//! * **Independent typed oracle** [`quad_oracle`] (new for G22; `u16` row
//!   ids, occurrence counts, sorted/dedup distinctness, expectation
//!   sources; shares no code with G21's oracle, G20's `predict`, the
//!   harness or production) predicts detector set, rejection count and
//!   net-null status of every order.
//!
//! Frozen results (matrix v1, SHA-256 `24cb859a…`), produced by this suite:
//!
//! * 139 quadruples × 24 orders = 3,336 order evaluations (2,923 distinct
//!   sequences replayed through production, incl. the control); the typed
//!   oracle agrees with the harness on all of them.
//! * MR-1 and MR-2 hold for all 48 independent quadruples and the control.
//! * MR-1 is falsified by 32 quadruples (28 of 72 interacting, 3 of 16
//!   seeded, 1 net-null negative whose fourth repeats its third); MR-2 by
//!   31 (28 interacting, 3 seeded). No quadruple gains a detector outside
//!   the union of its four singles.
//! * 120 quadruples are not the exact union of their singles (negatives):
//!   `PermutationDistinct` 82, `OracleAgreement` 21, `PermutationDigest` +
//!   `TranscriptDigest` 11, `PermutationShape` + `PermutationDistinct` 6.
//! * The G21 mask survives the fourth in 116 quadruples and is re-exposed
//!   in 42.
//! * Undetected: only 4 quadruples (both G21 net-null bases + the
//!   expectation restore, and the 2 net-null negatives), in all 24
//!   orders, each asserted net-null. `StateInvariant` never fires.
//!
//! On every production rejection reached (273,700 over the distinct
//! replays) the shared target's canonical SWC2 encoding, SWE1 seal and all
//! `WorkingContextStats` are preserved byte-for-byte, and G17's adapter
//! asserts that the anchor state is never advanced. The G17–G21 sources
//! are pinned by SHA-256 and the G17–G21 digests are reproduced first.
//!
//! Limits: as in G19–G21, mutations act on the *test-side* scenario;
//! production always replays the frozen G17 rows. Authorities and anchors
//! are in-process mocks; SWE1 is unkeyed SHA-256. Harness-level guards,
//! not production admission gates. No migration, signature, attestation,
//! transport, remote execution or E2E claim. `NOT_MEASURED`. No
//! production change.
//!
//! Assigned by ChatGPT on RemoteOps #1208 (comment 6738302613).

// The harness below (generator, scenario, detectors, typed mutations, G20
// pairs, G21 triples, G21 typed oracle and renderings) is copied verbatim
// from G21 (PR #902) and re-proven by reproducing the G19, G20 and G21
// matrix digests and frozen lists. The G22 part starts further below.

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

/// SHA-256 of the frozen G17 / G18 / G19 / G20 / G21 test sources at main
/// `106898cb` (G21 code merged as `14b8ce6c`, report as `106898cb`).
const FROZEN_SOURCES: [(&str, &[u8], &str); 5] = [
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
    (
        "working_context_anchor_g20_paired_mutations.rs (G20)",
        include_bytes!("working_context_anchor_g20_paired_mutations.rs"),
        "49cdb8577d340a05ba0290ae48dec1b991f37fa56f780550f851cc239204938a",
    ),
    (
        "working_context_anchor_g21_triple_mutations.rs (G21)",
        include_bytes!("working_context_anchor_g21_triple_mutations.rs"),
        "72aca1dc5899aa06bfa9fd0b802447c7b4ccd1791e798e3f8df0dc147d63020b",
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

// ===========================================================================
// G21: triple-perturbation completeness.
// ===========================================================================

/// Version of this frozen G21 triple-mutation matrix.
const G21_MATRIX_VERSION: u16 = 1;
/// SplitMix64 seed of the parameterised (seeded) triples.
const G21_SEED: u64 = 0x4721_0000_0000_0021;
/// Number of seeded triples.
const G21_SEEDED_TRIPLES: usize = 24;
/// Frozen SHA-256 of the canonical G21 matrix rendering.
const G21_FROZEN_MATRIX_SHA256: &str =
    "e9e53022c4092e86a95399d06006d7eb4920af5e8a818b607e1fa675e9ac1835";

/// Row flipped by the independent corpus third (not the G19 row under
/// mutation, not the retarget source, not the positive control).
const THIRD_FLIP_ROW: &str = "composed_envelope_flip_g12_domain";
/// Row retargeted by the independent expectation third.
const THIRD_EXPECT_ROW: &str = "composed_version_unknown_envelope_flip";

/// The six application orders of a triple `[a, b, c]`.
const ORDERS: [[usize; 3]; 6] = [
    [0, 1, 2],
    [0, 2, 1],
    [1, 0, 2],
    [1, 2, 0],
    [2, 0, 1],
    [2, 1, 0],
];

fn order_label(o: [usize; 3]) -> String {
    o.iter().map(|&i| ["a", "b", "c"][i]).collect()
}

/// First row (cyclically from `start`) whose frozen expectation differs
/// from that of `row` (`differ = true`) or equals it (`differ = false`),
/// excluding `row` itself.
fn retarget_source(row: usize, start: usize, differ: bool) -> usize {
    let names = shared::row_names();
    let n = names.len();
    (0..n)
        .map(|d| (start + d) % n)
        .find(|&t| {
            t != row && (shared::expected(names[t]) != shared::expected(names[row])) == differ
        })
        .expect("a retarget source exists")
}

/// G20 masking pairs (which include the three order-dependent pairs and
/// the net-null swap self-pair), as typed catalogue mutations.
fn base_pairs() -> Vec<(&'static str, &'static str, Mutation, Mutation)> {
    let cat = catalogue();
    let by = |n: &str| cat.iter().find(|(c, _)| *c == n).expect("catalogue").1;
    FROZEN_MASKED
        .iter()
        .map(|(a, b, _, _, _)| (*a, *b, by(a), by(b)))
        .collect()
}

/// Independent third mutations: they touch neither order 0 / order 1 (the
/// only orders the catalogue edits), nor the G19 row under mutation, nor
/// the retarget source, nor the positive control.
fn thirds() -> Vec<(&'static str, Mutation)> {
    let flip = row_index(THIRD_FLIP_ROW);
    let er = row_index(THIRD_EXPECT_ROW);
    vec![
        ("third_corpus_flip", Mutation::CorpusBytesFlip { row: flip }),
        (
            "third_expectation_retarget",
            Mutation::ExpectationRetarget {
                row: er,
                to: retarget_source(er, 0, true),
            },
        ),
        (
            "third_index_swap",
            Mutation::PermIndexSwap { perm: 7, pos: 3 },
        ),
        (
            "third_index_overwrite",
            Mutation::PermIndexOverwrite { perm: 6, pos: 5 },
        ),
        (
            "third_duplicate",
            Mutation::PermDuplicate { src: 2, dst: 3 },
        ),
        ("third_repeat_control", Mutation::RepeatControl { perm: 5 }),
    ]
}

/// Value-preserving third used only by the net-null negatives: the
/// expectation of the third row is retargeted to an *equal* expectation.
fn silent_third() -> Mutation {
    let er = row_index(THIRD_EXPECT_ROW);
    Mutation::ExpectationRetarget {
        row: er,
        to: retarget_source(er, 0, false),
    }
}

fn is_independent_row(row: usize) -> bool {
    let name = shared::row_names()[row];
    name != ROW_UNDER_MUTATION && name != RETARGET_SOURCE && name != CONTROL
}

/// Seeded triples: a G20 masking pair (seeded index) plus a seeded
/// independent third of a seeded kind, on orders 2..8 and independent rows.
fn seeded_triples() -> Vec<(usize, Mutation)> {
    let n = shared::row_names().len();
    let k = G18_SEEDS.len();
    let pairs = FROZEN_MASKED.len();
    let mut rng = SplitMix64(G21_SEED);
    let indep_row = |rng: &mut SplitMix64| loop {
        let r = rng.below(n);
        if is_independent_row(r) {
            break r;
        }
    };
    let mut out = Vec::with_capacity(G21_SEEDED_TRIPLES);
    for _ in 0..G21_SEEDED_TRIPLES {
        let base = rng.below(pairs);
        let perm = 2 + rng.below(k - 2);
        let third = match rng.below(6) {
            0 => Mutation::CorpusBytesFlip {
                row: indep_row(&mut rng),
            },
            1 => {
                let row = indep_row(&mut rng);
                let start = rng.below(n);
                Mutation::ExpectationRetarget {
                    row,
                    to: retarget_source(row, start, true),
                }
            }
            2 => Mutation::PermIndexSwap {
                perm,
                pos: rng.below(n),
            },
            3 => Mutation::PermIndexOverwrite {
                perm,
                pos: rng.below(n),
            },
            4 => {
                let dst = 2 + (perm - 2 + 1 + rng.below(k - 3)) % (k - 2);
                Mutation::PermDuplicate { src: perm, dst }
            }
            _ => Mutation::RepeatControl { perm },
        };
        out.push((base, third));
    }
    out
}

// --- Independent typed triple oracle. -------------------------------------

/// Typed corpus fact of one row.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RowFact {
    Intact,
    BytesFlipped,
    Omitted,
}

/// What the G21 typed oracle predicts for a mutation sequence.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Verdict {
    fired: Vec<Detector>,
    rejections: usize,
    net_null: bool,
}

/// Independent typed oracle for any mutation sequence (written for G21;
/// it shares no code with G20's `predict`, with [`evaluate`], with
/// production or with any digest renderer):
///
/// * corpus: one [`RowFact`] per row name; a flip only changes an
///   `Intact` row (the digest it would replace is otherwise gone);
/// * orders: owned name strings; shape = sorted names equal to the sorted
///   corpus names; distinctness = no order equals the corpus order and no
///   two orders are equal (checked with a `HashSet`);
/// * permutation / transcript digests: fire iff some name order differs
///   from the frozen one (G18: outcome depends on the row only);
/// * expectations: owned name → outcome map, judged against G17's pure
///   oracle, only for names still replayed;
/// * state invariant: predicted never to fire (production replays the
///   frozen rows) — an explicit negative;
/// * `net_null`: every fact equals the frozen one.
fn triple_oracle(muts: &[Mutation]) -> Verdict {
    use std::collections::HashSet;

    let names: Vec<String> = shared::row_names()
        .into_iter()
        .map(str::to_string)
        .collect();
    let frozen: Vec<Vec<String>> = G18_SEEDS
        .iter()
        .map(|&s| {
            permutation(s, names.len())
                .into_iter()
                .map(|i| names[i].clone())
                .collect()
        })
        .collect();
    let mut orders = frozen.clone();
    let mut facts: BTreeMap<String, RowFact> =
        names.iter().map(|n| (n.clone(), RowFact::Intact)).collect();
    let frozen_expect: BTreeMap<String, String> = names
        .iter()
        .map(|n| (n.clone(), shared::expected(n)))
        .collect();
    let mut expect = frozen_expect.clone();
    let control = CONTROL.to_string();

    for &m in muts {
        match m {
            Mutation::CorpusBytesFlip { row } => {
                let f = facts.get_mut(&names[row]).expect("row fact");
                if *f == RowFact::Intact {
                    *f = RowFact::BytesFlipped;
                }
            }
            Mutation::CorpusRowOmitted { row } => {
                facts.insert(names[row].clone(), RowFact::Omitted);
                for o in &mut orders {
                    o.retain(|x| *x != names[row]);
                }
            }
            Mutation::PermIndexOverwrite { perm, pos } => {
                let o = &mut orders[perm];
                let at = pos % (o.len() - 1);
                o[at] = o[at + 1].clone();
            }
            Mutation::PermIndexSwap { perm, pos } => {
                let o = &mut orders[perm];
                let at = pos % (o.len() - 1);
                o.swap(at, at + 1);
            }
            Mutation::PermDuplicate { src, dst } => {
                orders[dst] = orders[src].clone();
            }
            Mutation::RepeatControl { perm } => {
                let o = &mut orders[perm];
                if let Some(at) = o.iter().position(|x| *x == control) {
                    let slot = if at + 1 < o.len() { at + 1 } else { at - 1 };
                    o[slot] = control.clone();
                }
            }
            Mutation::ExpectationRetarget { row, to } => {
                expect.insert(names[row].clone(), shared::expected(&names[to]));
            }
        }
    }

    let mut fired = Vec::new();
    if facts.values().any(|f| *f != RowFact::Intact) {
        fired.push(CorpusDigest);
    }
    let mut sorted_names = names.clone();
    sorted_names.sort();
    if orders.iter().any(|o| {
        let mut s = o.clone();
        s.sort();
        s != sorted_names
    }) {
        fired.push(PermutationShape);
    }
    let mut seen: HashSet<&Vec<String>> = HashSet::new();
    if !orders.iter().all(|o| *o != names && seen.insert(o)) {
        fired.push(PermutationDistinct);
    }
    let orders_changed = orders != frozen;
    if orders_changed {
        fired.push(PermutationDigest);
    }
    let replayed: HashSet<&String> = orders.iter().flatten().collect();
    if replayed
        .iter()
        .any(|n| expect[*n] != shared::oracle_outcome(n))
    {
        fired.push(OracleAgreement);
    }
    if orders_changed {
        fired.push(TranscriptDigest);
    }
    fired.sort_unstable();
    let rejections = orders
        .iter()
        .flatten()
        .filter(|n| shared::is_rejection(n))
        .count();
    let net_null =
        !orders_changed && facts.values().all(|f| *f == RowFact::Intact) && expect == frozen_expect;
    Verdict {
        fired,
        rejections,
        net_null,
    }
}

// --- Triple matrix. --------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Group {
    /// G20 masking pair + catalogue independent third.
    Catalogue,
    /// G20 masking pair + seeded independent third.
    Seeded,
    /// Positive control: three mutually independent thirds.
    PositiveControl,
    /// Net-null negatives (swap involution + value-preserving third).
    NetNull,
}

struct TripleRow {
    group: Group,
    label: String,
    muts: [Mutation; 3],
    singles: [Vec<Detector>; 3],
    /// Evaluation for each of [`ORDERS`].
    orders: Vec<Evaluation>,
    /// Union of the three single detector sets.
    union: BTreeSet<Detector>,
    /// Per order: detectors of the union absent from that order.
    lost: Vec<Vec<Detector>>,
    /// Detectors present in some order but in no single.
    gained: Vec<Detector>,
    /// Per order: the triple equals (pair `a,b` in that relative order) ∪
    /// single `c`, i.e. the third is genuinely independent of the pair.
    extends_pair: Vec<bool>,
}

impl TripleRow {
    fn commutes(&self) -> bool {
        self.orders.iter().all(|e| e.fired == self.orders[0].fired)
    }
    fn undetected_orders(&self) -> Vec<usize> {
        (0..ORDERS.len())
            .filter(|&k| self.orders[k].fired.is_empty())
            .collect()
    }
    fn is_union(&self) -> bool {
        self.lost.iter().all(Vec::is_empty) && self.gained.is_empty()
    }
    fn sequence(&self, k: usize) -> [Mutation; 3] {
        let o = ORDERS[k];
        [self.muts[o[0]], self.muts[o[1]], self.muts[o[2]]]
    }
}

fn triple_row(group: Group, label: String, muts: [Mutation; 3]) -> TripleRow {
    let singles = muts.map(|m| evaluate(&scenario_of(&[m])).fired);
    let union: BTreeSet<Detector> = singles.iter().flat_map(|s| s.iter().copied()).collect();
    let pair_ab = set(&evaluate(&scenario_of(&[muts[0], muts[1]])).fired);
    let pair_ba = set(&evaluate(&scenario_of(&[muts[1], muts[0]])).fired);
    let c_set = set(&singles[2]);
    let mut orders = Vec::new();
    let mut lost = Vec::new();
    let mut extends_pair = Vec::new();
    let mut any: BTreeSet<Detector> = BTreeSet::new();
    for o in ORDERS {
        let ev = evaluate(&scenario_of(&[muts[o[0]], muts[o[1]], muts[o[2]]]));
        let fired = set(&ev.fired);
        lost.push(union.difference(&fired).copied().collect());
        let a_first = o.iter().position(|&i| i == 0) < o.iter().position(|&i| i == 1);
        let pair = if a_first { &pair_ab } else { &pair_ba };
        let expect: BTreeSet<Detector> = pair.union(&c_set).copied().collect();
        extends_pair.push(fired == expect);
        any.extend(fired);
        orders.push(ev);
    }
    let gained = any.difference(&union).copied().collect();
    TripleRow {
        group,
        label,
        muts,
        singles,
        orders,
        union,
        lost,
        gained,
        extends_pair,
    }
}

struct TripleMatrix {
    control: Evaluation,
    triples: Vec<TripleRow>,
}

fn triple_matrix() -> &'static TripleMatrix {
    static M: OnceLock<TripleMatrix> = OnceLock::new();
    M.get_or_init(|| {
        let control = evaluate(&frozen_scenario());
        let pairs = base_pairs();
        let th = thirds();
        let mut triples = Vec::new();
        for (a, b, ma, mb) in &pairs {
            for (c, mc) in &th {
                triples.push(triple_row(
                    Group::Catalogue,
                    format!("{a}+{b}+{c}"),
                    [*ma, *mb, *mc],
                ));
            }
        }
        for (i, (base, mc)) in seeded_triples().into_iter().enumerate() {
            let (a, b, ma, mb) = pairs[base];
            triples.push(triple_row(
                Group::Seeded,
                format!("seeded{i}:{a}+{b}+{mc:?}"),
                [ma, mb, mc],
            ));
        }
        triples.push(triple_row(
            Group::PositiveControl,
            "third_corpus_flip+third_expectation_retarget+third_index_swap".to_string(),
            [th[0].1, th[1].1, th[2].1],
        ));
        let swap = catalogue()[3].1;
        triples.push(triple_row(
            Group::NetNull,
            "permutation_index_swap+permutation_index_swap+silent_expectation_retarget".to_string(),
            [swap, swap, silent_third()],
        ));
        let swap7 = th[2].1;
        triples.push(triple_row(
            Group::NetNull,
            "third_index_swap+third_index_swap+silent_expectation_retarget".to_string(),
            [swap7, swap7, silent_third()],
        ));
        TripleMatrix { control, triples }
    })
}

fn render_g21_matrix(m: &TripleMatrix) -> String {
    let mut out = String::new();
    for t in &m.triples {
        out.push_str(&format!(
            "{G21_MATRIX_VERSION}|triple|{:?}|{}|{:?}|a={:?}|b={:?}|c={:?}|union={:?}|gained={:?}|commutes={}|undetected={:?}\n",
            t.group,
            t.label,
            t.muts,
            t.singles[0],
            t.singles[1],
            t.singles[2],
            t.union,
            t.gained,
            t.commutes(),
            t.undetected_orders(),
        ));
        for (k, ev) in t.orders.iter().enumerate() {
            out.push_str(&format!(
                "{G21_MATRIX_VERSION}|order|{}|{}|{:?}|lost={:?}|extends_pair={}|{}|{}|{}|{}\n",
                t.label,
                order_label(ORDERS[k]),
                ev.fired,
                t.lost[k],
                t.extends_pair[k],
                ev.corpus_sha256,
                ev.permutations_sha256,
                ev.transcript_sha256,
                ev.rejections_checked,
            ));
        }
    }
    out
}

/// Bitmask over [`ORDERS`] (bit `k` = order `k`).
fn mask(f: impl Fn(usize) -> bool) -> u8 {
    (0..ORDERS.len())
        .filter(|&k| f(k))
        .fold(0u8, |acc, k| acc | (1 << k))
}

// --- Frozen expected triple results (negatives kept). ---------------------

#[derive(Debug, PartialEq, Eq)]
struct FrozenTriple {
    label: String,
    lost_any: Vec<Detector>,
    lost_orders: u8,
    gained: Vec<Detector>,
}

/// Union of the singles of the detection positive-control triple.
const FROZEN_POSITIVE_CONTROL_UNION: &[Detector] = &[
    CorpusDigest,
    PermutationDigest,
    OracleAgreement,
    TranscriptDigest,
];

/// Triples that are NOT the exact union of their singles:
/// `(label, detectors lost in some order, bitmask of lossy orders over
/// [`ORDERS`], gained)`. Negative results, kept on purpose.
#[allow(clippy::type_complexity)]
const FROZEN_TRIPLE_LOSSES: &[(&str, &[Detector], u8, &[Detector])] = &[
    ("corpus_row_omitted+typed_error_expectation+third_corpus_flip", &[OracleAgreement], 0b111111, &[]),
    ("corpus_row_omitted+typed_error_expectation+third_index_swap", &[OracleAgreement], 0b111111, &[]),
    ("corpus_row_omitted+typed_error_expectation+third_index_overwrite", &[OracleAgreement], 0b111111, &[]),
    ("corpus_row_omitted+typed_error_expectation+third_duplicate", &[OracleAgreement], 0b111111, &[]),
    ("corpus_row_omitted+typed_error_expectation+third_repeat_control", &[OracleAgreement], 0b111111, &[]),
    ("permutation_index_overwrite+duplicate_permutation+third_corpus_flip", &[PermutationDistinct], 0b101100, &[]),
    ("permutation_index_overwrite+duplicate_permutation+third_expectation_retarget", &[PermutationDistinct], 0b101100, &[]),
    ("permutation_index_overwrite+duplicate_permutation+third_index_swap", &[PermutationDistinct], 0b101100, &[]),
    ("permutation_index_overwrite+duplicate_permutation+third_index_overwrite", &[PermutationDistinct], 0b101100, &[]),
    ("permutation_index_overwrite+duplicate_permutation+third_repeat_control", &[PermutationDistinct], 0b101100, &[]),
    ("permutation_index_swap+permutation_index_swap+third_corpus_flip", &[PermutationDigest, TranscriptDigest], 0b111111, &[]),
    ("permutation_index_swap+permutation_index_swap+third_expectation_retarget", &[PermutationDigest, TranscriptDigest], 0b111111, &[]),
    ("permutation_index_swap+duplicate_permutation+third_corpus_flip", &[PermutationDistinct], 0b101100, &[]),
    ("permutation_index_swap+duplicate_permutation+third_expectation_retarget", &[PermutationDistinct], 0b101100, &[]),
    ("permutation_index_swap+duplicate_permutation+third_index_swap", &[PermutationDistinct], 0b101100, &[]),
    ("permutation_index_swap+duplicate_permutation+third_index_overwrite", &[PermutationDistinct], 0b101100, &[]),
    ("permutation_index_swap+duplicate_permutation+third_repeat_control", &[PermutationDistinct], 0b101100, &[]),
    ("duplicate_permutation+repeated_positive_control+third_corpus_flip", &[PermutationDistinct], 0b010011, &[]),
    ("duplicate_permutation+repeated_positive_control+third_expectation_retarget", &[PermutationDistinct], 0b010011, &[]),
    ("duplicate_permutation+repeated_positive_control+third_index_swap", &[PermutationDistinct], 0b010011, &[]),
    ("duplicate_permutation+repeated_positive_control+third_index_overwrite", &[PermutationDistinct], 0b010011, &[]),
    ("duplicate_permutation+repeated_positive_control+third_repeat_control", &[PermutationDistinct], 0b010011, &[]),
    ("seeded0:permutation_index_swap+duplicate_permutation+PermIndexOverwrite { perm: 7, pos: 2 }", &[PermutationDistinct], 0b101100, &[]),
    ("seeded1:duplicate_permutation+repeated_positive_control+CorpusBytesFlip { row: 8 }", &[PermutationDistinct], 0b010011, &[]),
    ("seeded2:duplicate_permutation+repeated_positive_control+PermIndexOverwrite { perm: 2, pos: 3 }", &[PermutationDistinct], 0b010011, &[]),
    ("seeded3:duplicate_permutation+repeated_positive_control+RepeatControl { perm: 5 }", &[PermutationDistinct], 0b010011, &[]),
    ("seeded4:corpus_row_omitted+typed_error_expectation+RepeatControl { perm: 6 }", &[OracleAgreement], 0b111111, &[]),
    ("seeded6:permutation_index_overwrite+duplicate_permutation+CorpusBytesFlip { row: 3 }", &[PermutationDistinct], 0b101100, &[]),
    ("seeded7:duplicate_permutation+repeated_positive_control+PermIndexSwap { perm: 6, pos: 0 }", &[PermutationDistinct], 0b010011, &[]),
    ("seeded8:duplicate_permutation+repeated_positive_control+CorpusBytesFlip { row: 9 }", &[PermutationDistinct], 0b010011, &[]),
    ("seeded9:permutation_index_swap+permutation_index_swap+ExpectationRetarget { row: 11, to: 12 }", &[PermutationDigest, TranscriptDigest], 0b111111, &[]),
    ("seeded11:corpus_row_omitted+typed_error_expectation+PermIndexOverwrite { perm: 5, pos: 4 }", &[OracleAgreement], 0b111111, &[]),
    ("seeded12:permutation_index_overwrite+duplicate_permutation+PermIndexOverwrite { perm: 6, pos: 3 }", &[PermutationDistinct], 0b101100, &[]),
    ("seeded14:corpus_row_omitted+typed_error_expectation+PermIndexSwap { perm: 6, pos: 8 }", &[OracleAgreement], 0b111111, &[]),
    ("seeded15:corpus_row_omitted+typed_error_expectation+CorpusBytesFlip { row: 3 }", &[OracleAgreement], 0b111111, &[]),
    ("seeded16:corpus_row_omitted+typed_error_expectation+RepeatControl { perm: 4 }", &[OracleAgreement], 0b111111, &[]),
    ("seeded17:permutation_index_swap+duplicate_permutation+PermIndexSwap { perm: 4, pos: 7 }", &[PermutationDistinct], 0b101100, &[]),
    ("seeded19:permutation_index_swap+permutation_index_swap+ExpectationRetarget { row: 1, to: 11 }", &[PermutationDigest, TranscriptDigest], 0b111111, &[]),
    ("seeded20:permutation_index_swap+permutation_index_swap+CorpusBytesFlip { row: 3 }", &[PermutationDigest, TranscriptDigest], 0b111111, &[]),
    ("permutation_index_swap+permutation_index_swap+silent_expectation_retarget", &[PermutationDigest, TranscriptDigest], 0b111111, &[]),
    ("third_index_swap+third_index_swap+silent_expectation_retarget", &[PermutationDigest, TranscriptDigest], 0b111111, &[]),
];

/// Triples whose detector set depends on application order: `(label,
/// bitmask of orders whose set differs from order abc)`.
const FROZEN_TRIPLE_NON_COMMUTING: &[(&str, u8)] = &[
    ("permutation_index_overwrite+duplicate_permutation+third_corpus_flip", 0b101100),
    ("permutation_index_overwrite+duplicate_permutation+third_expectation_retarget", 0b101100),
    ("permutation_index_overwrite+duplicate_permutation+third_index_swap", 0b101100),
    ("permutation_index_overwrite+duplicate_permutation+third_index_overwrite", 0b101100),
    ("permutation_index_overwrite+duplicate_permutation+third_repeat_control", 0b101100),
    ("permutation_index_swap+duplicate_permutation+third_corpus_flip", 0b101100),
    ("permutation_index_swap+duplicate_permutation+third_expectation_retarget", 0b101100),
    ("permutation_index_swap+duplicate_permutation+third_index_swap", 0b101100),
    ("permutation_index_swap+duplicate_permutation+third_index_overwrite", 0b101100),
    ("permutation_index_swap+duplicate_permutation+third_repeat_control", 0b101100),
    ("duplicate_permutation+repeated_positive_control+third_corpus_flip", 0b101100),
    ("duplicate_permutation+repeated_positive_control+third_expectation_retarget", 0b101100),
    ("duplicate_permutation+repeated_positive_control+third_index_swap", 0b101100),
    ("duplicate_permutation+repeated_positive_control+third_index_overwrite", 0b101100),
    ("duplicate_permutation+repeated_positive_control+third_repeat_control", 0b101100),
    ("seeded0:permutation_index_swap+duplicate_permutation+PermIndexOverwrite { perm: 7, pos: 2 }", 0b101100),
    ("seeded1:duplicate_permutation+repeated_positive_control+CorpusBytesFlip { row: 8 }", 0b101100),
    ("seeded2:duplicate_permutation+repeated_positive_control+PermIndexOverwrite { perm: 2, pos: 3 }", 0b101100),
    ("seeded3:duplicate_permutation+repeated_positive_control+RepeatControl { perm: 5 }", 0b101100),
    ("seeded6:permutation_index_overwrite+duplicate_permutation+CorpusBytesFlip { row: 3 }", 0b101100),
    ("seeded7:duplicate_permutation+repeated_positive_control+PermIndexSwap { perm: 6, pos: 0 }", 0b101100),
    ("seeded8:duplicate_permutation+repeated_positive_control+CorpusBytesFlip { row: 9 }", 0b101100),
    ("seeded12:permutation_index_overwrite+duplicate_permutation+PermIndexOverwrite { perm: 6, pos: 3 }", 0b101100),
    ("seeded17:permutation_index_swap+duplicate_permutation+PermIndexSwap { perm: 4, pos: 7 }", 0b101100),
];

/// Triples (outside the net-null group) whose set is NOT `pair ∪ third`
/// in some order: `(label, bitmask of such orders)`. Empty: every
/// independent third composes with its pair as a plain union.
const FROZEN_TRIPLE_NOT_EXTENDING: &[(&str, u8)] = &[];

/// Triples per group that are the exact union of their singles in all six
/// orders (the third re-fires what the pair had masked).
const FROZEN_UNION_CATALOGUE: usize = 8;
const FROZEN_UNION_SEEDED: usize = 7;

/// Triples undetected in some order: `(label, bitmask of such orders)`.
const FROZEN_TRIPLE_UNDETECTED: &[(&str, u8)] = &[
    (
        "permutation_index_swap+permutation_index_swap+silent_expectation_retarget",
        0b111111,
    ),
    (
        "third_index_swap+third_index_swap+silent_expectation_retarget",
        0b111111,
    ),
];

// ===========================================================================
// G21 re-proof (the G21 tests, folded into helpers so G22 trusts nothing it
// has not re-derived).
// ===========================================================================

fn reprove_g21_frozen_lists(tm: &TripleMatrix) {
    let non_union: Vec<FrozenTriple> = tm
        .triples
        .iter()
        .filter(|t| !t.is_union())
        .map(|t| FrozenTriple {
            label: t.label.clone(),
            lost_any: t
                .lost
                .iter()
                .flatten()
                .copied()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            lost_orders: mask(|k| !t.lost[k].is_empty()),
            gained: t.gained.clone(),
        })
        .collect();
    let want: Vec<FrozenTriple> = FROZEN_TRIPLE_LOSSES
        .iter()
        .map(|(l, la, lo, g)| FrozenTriple {
            label: (*l).to_string(),
            lost_any: la.to_vec(),
            lost_orders: *lo,
            gained: g.to_vec(),
        })
        .collect();
    assert_eq!(non_union, want, "G21 frozen lost/gained list");
    let unions = |g: Group| {
        tm.triples
            .iter()
            .filter(|t| t.group == g && t.is_union())
            .count()
    };
    assert_eq!(unions(Group::Catalogue), FROZEN_UNION_CATALOGUE);
    assert_eq!(unions(Group::Seeded), FROZEN_UNION_SEEDED);
    let non_commuting: Vec<(&str, u8)> = tm
        .triples
        .iter()
        .filter(|t| !t.commutes())
        .map(|t| {
            let first = &t.orders[0].fired;
            (t.label.as_str(), mask(|k| t.orders[k].fired != *first))
        })
        .collect();
    assert_eq!(non_commuting, FROZEN_TRIPLE_NON_COMMUTING.to_vec());
    let not_extending: Vec<(&str, u8)> = tm
        .triples
        .iter()
        .filter(|t| t.group != Group::NetNull && !t.extends_pair.iter().all(|&e| e))
        .map(|t| (t.label.as_str(), mask(|k| !t.extends_pair[k])))
        .collect();
    assert_eq!(not_extending, FROZEN_TRIPLE_NOT_EXTENDING.to_vec());
    let undetected: Vec<(&str, u8)> = tm
        .triples
        .iter()
        .filter(|t| !t.undetected_orders().is_empty())
        .map(|t| (t.label.as_str(), mask(|k| t.orders[k].fired.is_empty())))
        .collect();
    assert_eq!(undetected, FROZEN_TRIPLE_UNDETECTED.to_vec());
    let pc = tm
        .triples
        .iter()
        .find(|t| t.group == Group::PositiveControl)
        .expect("G21 positive control");
    assert_eq!(
        pc.union.iter().copied().collect::<Vec<_>>(),
        FROZEN_POSITIVE_CONTROL_UNION.to_vec()
    );
    // G21's own typed oracle still agrees with the harness on every order.
    for t in &tm.triples {
        for (k, ev) in t.orders.iter().enumerate() {
            let v = triple_oracle(&t.sequence(k));
            assert_eq!(v.fired, ev.fired, "{}: G21 oracle", t.label);
            assert_eq!(v.rejections, ev.rejections_checked);
        }
    }
}

// ===========================================================================
// G22: metamorphic fourth-mutation falsification.
// ===========================================================================

/// Version of this frozen G22 fourth-mutation matrix.
const G22_MATRIX_VERSION: u16 = 1;
/// SplitMix64 seed of the seeded quadruples (pre-registered).
const G22_SEED: u64 = 0x4722_0000_0000_0022;
/// Number of seeded quadruples (pre-registered bound).
const G22_SEEDED_QUADS: usize = 16;
/// Order left untouched by every G21 catalogue pair and third, used by the
/// independent fourth swap.
const FOURTH_FREE_PERM: usize = 4;
/// Frozen SHA-256 of the G21 matrix this suite builds on.
const G21_FROZEN_MATRIX_SHA256_PIN: &str =
    "e9e53022c4092e86a95399d06006d7eb4920af5e8a818b607e1fa675e9ac1835";
/// Frozen SHA-256 of the pre-registered G22 plan rendering (bases,
/// catalogue fourths, seeded quadruples, controls; no results).
const G22_FROZEN_PLAN_SHA256: &str =
    "4cd65d4976988c0960ab9e65bb3686b6b64e00bdb56368504b733c3f14d710ca";
/// Frozen SHA-256 of the canonical G22 matrix rendering (results).
const G22_FROZEN_MATRIX_SHA256: &str =
    "24cb859a593111ddb70f6644370d2392b0c187d6d7a3968d8440338247869160";

/// Pre-registered class of a catalogue fourth mutation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum FourthClass {
    /// Touches no order, row or expectation used by the base triple.
    Independent,
    /// Deliberately touches what the G20 pair touches (orders 0/1, the
    /// G19 row under mutation).
    Interacting,
}

/// First corpus row (in G17 order) used by no catalogue pair or third
/// and by neither the retarget source nor the positive control.
fn fourth_free_row() -> usize {
    let names = shared::row_names();
    (0..names.len())
        .find(|&r| {
            let nm = names[r];
            is_independent_row(r) && nm != THIRD_FLIP_ROW && nm != THIRD_EXPECT_ROW
        })
        .expect("a free row exists")
}

/// Pre-registered catalogue of fourth mutations.
fn fourths() -> Vec<(&'static str, FourthClass, Mutation)> {
    let free = fourth_free_row();
    let rum = row_index(ROW_UNDER_MUTATION);
    vec![
        (
            "fourth_free_retarget",
            FourthClass::Independent,
            Mutation::ExpectationRetarget {
                row: free,
                to: retarget_source(free, 0, true),
            },
        ),
        (
            "fourth_free_swap",
            FourthClass::Independent,
            Mutation::PermIndexSwap {
                perm: FOURTH_FREE_PERM,
                pos: 6,
            },
        ),
        (
            "fourth_order0_swap",
            FourthClass::Interacting,
            Mutation::PermIndexSwap { perm: 0, pos: 0 },
        ),
        (
            "fourth_order1_onto_order0",
            FourthClass::Interacting,
            Mutation::PermDuplicate { src: 1, dst: 0 },
        ),
        (
            "fourth_expectation_restore",
            FourthClass::Interacting,
            Mutation::ExpectationRetarget { row: rum, to: rum },
        ),
    ]
}

/// The 24 application orders of `[a, b, c, d]`: for each G21 relative
/// order `k` of `a, b, c` ([`ORDERS`]), the fourth `d` is inserted at
/// position `p` in `0..=3`. Index `4 * k + p`.
fn quad_orders() -> Vec<[usize; 4]> {
    let mut out = Vec::with_capacity(24);
    for o in ORDERS {
        for p in 0..=3 {
            let mut seq = o.to_vec();
            seq.insert(p, 3);
            out.push([seq[0], seq[1], seq[2], seq[3]]);
        }
    }
    out
}

fn quad_order_label(o: [usize; 4]) -> String {
    o.iter().map(|&i| ["a", "b", "c", "d"][i]).collect()
}

/// Seeded quadruples: index into the pool of surviving G21 masks plus a
/// seeded fourth of any kind on any order / row (interaction allowed).
fn seeded_quads(pool: usize) -> Vec<(usize, Mutation)> {
    let n = shared::row_names().len();
    let k = G18_SEEDS.len();
    let mut rng = SplitMix64(G22_SEED);
    let mut out = Vec::with_capacity(G22_SEEDED_QUADS);
    for _ in 0..G22_SEEDED_QUADS {
        let base = rng.below(pool);
        let perm = rng.below(k);
        let fourth = match rng.below(7) {
            0 => Mutation::CorpusBytesFlip { row: rng.below(n) },
            1 => Mutation::CorpusRowOmitted { row: rng.below(n) },
            2 => {
                let row = rng.below(n);
                let start = rng.below(n);
                Mutation::ExpectationRetarget {
                    row,
                    to: retarget_source(row, start, true),
                }
            }
            3 => Mutation::PermIndexSwap {
                perm,
                pos: rng.below(n),
            },
            4 => Mutation::PermIndexOverwrite {
                perm,
                pos: rng.below(n),
            },
            5 => {
                let dst = (perm + 1 + rng.below(k - 1)) % k;
                Mutation::PermDuplicate { src: perm, dst }
            }
            _ => Mutation::RepeatControl { perm },
        };
        out.push((base, fourth));
    }
    out
}

// --- Independent typed quadruple oracle. ----------------------------------

/// What the G22 typed oracle predicts for a mutation sequence.
#[derive(Clone, Debug, PartialEq, Eq)]
struct QuadVerdict {
    fired: Vec<Detector>,
    rejections: usize,
    net_null: bool,
}

/// Independent typed oracle (new for G22; shares no code with G21's
/// [`triple_oracle`], G20's `predict`, [`evaluate`], [`apply`],
/// [`permutation`] or production):
///
/// * frozen orders are re-derived from the G18 seeds with an inline
///   SplitMix64 / Fisher-Yates over `u16` row ids;
/// * corpus: sets of omitted and flipped row ids (a flip of an omitted or
///   already flipped row changes nothing);
/// * expectations: one *source* row id per row (`row` is judged against
///   the frozen expectation of its source);
/// * shape: per-order occurrence counts must all be exactly one;
/// * distinctness: sorted + deduplicated order list must keep its length
///   and contain no identity order;
/// * permutation / transcript digests: fire iff the id orders differ from
///   the frozen ones (G18: the outcome depends on the row only);
/// * oracle agreement: some *replayed* row whose source expectation
///   differs from G17's pure oracle outcome;
/// * state invariant: predicted never to fire (an explicit negative).
struct OracleTables {
    names: Vec<&'static str>,
    frozen: Vec<Vec<u16>>,
    frozen_expect: Vec<String>,
    pure: Vec<String>,
    rejection: Vec<bool>,
    control: u16,
}

fn oracle_id(i: usize) -> u16 {
    u16::try_from(i).expect("row id fits u16")
}

/// Frozen facts the G22 oracle reads, derived once from G17 / G18.
fn oracle_tables() -> &'static OracleTables {
    static T: OnceLock<OracleTables> = OnceLock::new();
    T.get_or_init(|| {
        let names = shared::row_names();
        let n = names.len();
        let id = oracle_id;
        let frozen: Vec<Vec<u16>> = G18_SEEDS
            .iter()
            .map(|&seed| {
                let mut st = seed;
                let mut v: Vec<u16> = (0..n).map(id).collect();
                for i in (1..n).rev() {
                    st = st.wrapping_add(0x9E37_79B9_7F4A_7C15);
                    let mut z = st;
                    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
                    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
                    z ^= z >> 31;
                    let bound = u64::try_from(i + 1).expect("fits u64");
                    let j = usize::try_from(z % bound).expect("fits usize");
                    v.swap(i, j);
                }
                v
            })
            .collect();
        let control = id(names
            .iter()
            .position(|nm| *nm == CONTROL)
            .expect("control row"));
        OracleTables {
            frozen_expect: names.iter().map(|nm| shared::expected(nm)).collect(),
            pure: names.iter().map(|nm| shared::oracle_outcome(nm)).collect(),
            rejection: names.iter().map(|nm| shared::is_rejection(nm)).collect(),
            names,
            frozen,
            control,
        }
    })
}

fn quad_oracle(muts: &[Mutation]) -> QuadVerdict {
    let t = oracle_tables();
    let n = t.names.len();
    let id = oracle_id;
    let ix = |r: u16| usize::from(r);
    let frozen = &t.frozen;
    let control = t.control;
    let mut orders = frozen.clone();
    let mut omitted: BTreeSet<u16> = BTreeSet::new();
    let mut flipped: BTreeSet<u16> = BTreeSet::new();
    let mut source: Vec<u16> = (0..n).map(id).collect();
    for &m in muts {
        match m {
            Mutation::CorpusBytesFlip { row } => {
                if !omitted.contains(&id(row)) {
                    flipped.insert(id(row));
                }
            }
            Mutation::CorpusRowOmitted { row } => {
                omitted.insert(id(row));
                for o in &mut orders {
                    o.retain(|&r| r != id(row));
                }
            }
            Mutation::PermIndexOverwrite { perm, pos } => {
                let o = &mut orders[perm];
                let at = pos % (o.len() - 1);
                o[at] = o[at + 1];
            }
            Mutation::PermIndexSwap { perm, pos } => {
                let o = &mut orders[perm];
                let at = pos % (o.len() - 1);
                o.swap(at, at + 1);
            }
            Mutation::PermDuplicate { src, dst } => {
                orders[dst] = orders[src].clone();
            }
            Mutation::RepeatControl { perm } => {
                let o = &mut orders[perm];
                if let Some(at) = o.iter().position(|&r| r == control) {
                    let slot = if at + 1 < o.len() { at + 1 } else { at - 1 };
                    o[slot] = control;
                }
            }
            Mutation::ExpectationRetarget { row, to } => {
                source[row] = id(to);
            }
        }
    }
    let frozen_expect = &t.frozen_expect;
    let pure = &t.pure;
    let identity: Vec<u16> = (0..n).map(id).collect();

    let mut fired = BTreeSet::new();
    if !omitted.is_empty() || !flipped.is_empty() {
        fired.insert(CorpusDigest);
    }
    let shape_ok = orders.iter().all(|o| {
        let mut count = vec![0u32; n];
        for &r in o {
            count[ix(r)] += 1;
        }
        o.len() == n && count.iter().all(|&c| c == 1)
    });
    if !shape_ok {
        fired.insert(PermutationShape);
    }
    let mut uniq = orders.clone();
    uniq.sort();
    uniq.dedup();
    if uniq.len() != orders.len() || orders.contains(&identity) {
        fired.insert(PermutationDistinct);
    }
    let changed = orders != *frozen;
    if changed {
        fired.insert(PermutationDigest);
        fired.insert(TranscriptDigest);
    }
    let mut replayed = vec![false; n];
    for &r in orders.iter().flatten() {
        replayed[ix(r)] = true;
    }
    if (0..n).any(|r| replayed[r] && frozen_expect[ix(source[r])] != pure[r]) {
        fired.insert(OracleAgreement);
    }
    let rejections = orders
        .iter()
        .flatten()
        .filter(|&&r| t.rejection[ix(r)])
        .count();
    let net_null = !changed
        && omitted.is_empty()
        && flipped.is_empty()
        && (0..n).all(|r| frozen_expect[ix(source[r])] == frozen_expect[r]);
    QuadVerdict {
        fired: fired.into_iter().collect(),
        rejections,
        net_null,
    }
}

// --- Quadruple matrix. ----------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum QuadGroup {
    /// Surviving G21 mask (catalogue / net-null base) + catalogue fourth.
    Catalogue(FourthClass),
    /// Surviving G21 mask (any group) + seeded fourth.
    Seeded,
    /// Positive control: four mutually independent mutations.
    PositiveControl,
    /// Net-null negatives.
    NetNull,
}

struct QuadRow {
    group: QuadGroup,
    label: String,
    muts: [Mutation; 4],
    singles: [Vec<Detector>; 4],
    union: BTreeSet<Detector>,
    /// Detector set of `a, b, c` in each of [`ORDERS`].
    triple_sets: Vec<BTreeSet<Detector>>,
    /// Per G21 relative order `k`: detectors the base triple `a, b, c`
    /// loses against the union of its three singles (its surviving mask).
    base_lost: Vec<BTreeSet<Detector>>,
    /// Evaluation for each of [`quad_orders`].
    orders: Vec<Evaluation>,
    /// Number of distinct mutation sequences among the 24 orders (each
    /// replayed through production exactly once).
    distinct: usize,
    /// Production rejections checked over the distinct sequences.
    replayed_rejections: usize,
    /// Per order: detectors of the four-singles union absent.
    lost: Vec<Vec<Detector>>,
    /// Detectors fired in some order but in no single.
    gained: Vec<Detector>,
    /// Per order: metamorphic prediction `triple(rel. order) ∪ d` minus
    /// the observed set (lost vs the prediction).
    mr_lost: Vec<Vec<Detector>>,
    /// Per order: observed set minus the metamorphic prediction.
    mr_gained: Vec<Vec<Detector>>,
}

impl QuadRow {
    fn sequence(&self, i: usize) -> [Mutation; 4] {
        let o = quad_orders()[i];
        o.map(|j| self.muts[j])
    }
    fn mask24(&self, f: impl Fn(usize) -> bool) -> u32 {
        (0..24)
            .filter(|&i| f(i))
            .fold(0u32, |acc, i| acc | (1 << i))
    }
    fn commutes(&self) -> bool {
        self.orders.iter().all(|e| e.fired == self.orders[0].fired)
    }
    /// Bit `k` set iff, for relative order `k` of `a, b, c`, the detector
    /// set depends on where `d` is inserted.
    fn position_dependent(&self) -> u8 {
        mask(|k| (1..4).any(|p| self.orders[4 * k + p].fired != self.orders[4 * k].fired))
    }
    fn falsified(&self) -> bool {
        self.mr_lost.iter().any(|v| !v.is_empty()) || self.mr_gained.iter().any(|v| !v.is_empty())
    }
    /// Base-mask detectors that fire in some quad order whose relative
    /// order `k` of `a, b, c` had them masked (re-exposed by `d`).
    fn reexposed(&self) -> Vec<Detector> {
        let mut out = BTreeSet::new();
        for (i, ev) in self.orders.iter().enumerate() {
            let fired = set(&ev.fired);
            out.extend(self.base_lost[i / 4].intersection(&fired).copied());
        }
        out.into_iter().collect()
    }
    /// Base-mask detectors still absent in some quad order whose relative
    /// order had them masked (the mask survives the fourth mutation).
    fn surviving(&self) -> Vec<Detector> {
        let mut out = BTreeSet::new();
        for (i, ev) in self.orders.iter().enumerate() {
            let fired = set(&ev.fired);
            out.extend(self.base_lost[i / 4].difference(&fired).copied());
        }
        out.into_iter().collect()
    }
    fn is_union(&self) -> bool {
        self.lost.iter().all(Vec::is_empty) && self.gained.is_empty()
    }
}

/// Runs `f` over `items` on scoped threads, preserving order.
fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let chunk = items.len().div_ceil(threads.max(1)).max(1);
    std::thread::scope(|s| {
        let handles: Vec<_> = items
            .chunks(chunk)
            .map(|c| s.spawn(|| c.iter().map(&f).collect::<Vec<R>>()))
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("worker"))
            .collect()
    })
}

/// A planned quadruple (pre-registered; no results).
struct Plan {
    group: QuadGroup,
    label: String,
    muts: [Mutation; 4],
    base_label: Option<String>,
}

/// Surviving G21 masks usable as bases: every G21 triple that is not the
/// exact union of its singles (its mask survived the third).
fn surviving_masks(tm: &TripleMatrix) -> Vec<&TripleRow> {
    tm.triples.iter().filter(|t| !t.is_union()).collect()
}

fn plan(tm: &TripleMatrix) -> Vec<Plan> {
    let pool = surviving_masks(tm);
    let mut out = Vec::new();
    for t in pool
        .iter()
        .filter(|t| matches!(t.group, Group::Catalogue | Group::NetNull))
    {
        for (name, class, m) in fourths() {
            out.push(Plan {
                group: QuadGroup::Catalogue(class),
                label: format!("{}+{name}", t.label),
                muts: [t.muts[0], t.muts[1], t.muts[2], m],
                base_label: Some(t.label.clone()),
            });
        }
    }
    for (i, (base, m)) in seeded_quads(pool.len()).into_iter().enumerate() {
        let t = pool[base];
        out.push(Plan {
            group: QuadGroup::Seeded,
            label: format!("q{i}:{}+{m:?}", t.label),
            muts: [t.muts[0], t.muts[1], t.muts[2], m],
            base_label: Some(t.label.clone()),
        });
    }
    let th = thirds();
    let f = fourths();
    out.push(Plan {
        group: QuadGroup::PositiveControl,
        label:
            "control:third_corpus_flip+third_expectation_retarget+third_index_swap+fourth_free_swap"
                .to_string(),
        muts: [th[0].1, th[1].1, th[2].1, f[1].2],
        base_label: None,
    });
    let swap0 = catalogue()[3].1;
    out.push(Plan {
        group: QuadGroup::NetNull,
        label: "netnull:permutation_index_swap+permutation_index_swap+fourth_expectation_restore+silent_expectation_retarget".to_string(),
        muts: [swap0, swap0, f[4].2, silent_third()],
        base_label: None,
    });
    out.push(Plan {
        group: QuadGroup::NetNull,
        label: "netnull:third_index_swap+third_index_swap+fourth_free_swap+fourth_free_swap"
            .to_string(),
        muts: [th[2].1, th[2].1, f[1].2, f[1].2],
        base_label: None,
    });
    out
}

fn render_plan(p: &[Plan]) -> String {
    let mut out = format!(
        "{G22_MATRIX_VERSION}|plan|seed={G22_SEED:#018x}|seeded={G22_SEEDED_QUADS}|g21={G21_FROZEN_MATRIX_SHA256_PIN}\n"
    );
    for (name, class, m) in fourths() {
        out.push_str(&format!(
            "{G22_MATRIX_VERSION}|fourth|{name}|{class:?}|{m:?}\n"
        ));
    }
    for q in p {
        out.push_str(&format!(
            "{G22_MATRIX_VERSION}|quad|{:?}|{}|{:?}|base={:?}\n",
            q.group, q.label, q.muts, q.base_label
        ));
    }
    out
}

fn sequence_key(seq: &[Mutation]) -> String {
    format!("{seq:?}")
}

fn quad_row(p: &Plan, tm: &TripleMatrix, singles_of: &BTreeMap<String, Vec<Detector>>) -> QuadRow {
    let singles = p.muts.map(|m| singles_of[&sequence_key(&[m])].clone());
    let union: BTreeSet<Detector> = singles.iter().flat_map(|s| s.iter().copied()).collect();
    let triple_sets: Vec<BTreeSet<Detector>> = match &p.base_label {
        Some(l) => {
            let t = tm
                .triples
                .iter()
                .find(|t| &t.label == l)
                .expect("G21 base triple");
            assert_eq!(t.muts, [p.muts[0], p.muts[1], p.muts[2]]);
            t.orders.iter().map(|e| set(&e.fired)).collect()
        }
        None => ORDERS
            .iter()
            .map(|o| set(&evaluate(&scenario_of(&o.map(|j| p.muts[j]))).fired))
            .collect(),
    };
    let base_union: BTreeSet<Detector> = singles[..3]
        .iter()
        .flat_map(|s| s.iter().copied())
        .collect();
    let base_lost: Vec<BTreeSet<Detector>> = triple_sets
        .iter()
        .map(|t| base_union.difference(t).copied().collect())
        .collect();
    let d_set = set(&singles[3]);
    let mut cache: BTreeMap<String, Evaluation> = BTreeMap::new();
    let mut orders = Vec::with_capacity(24);
    let mut lost = Vec::new();
    let mut mr_lost = Vec::new();
    let mut mr_gained = Vec::new();
    for (i, o) in quad_orders().into_iter().enumerate() {
        let seq = o.map(|j| p.muts[j]);
        let ev = cache
            .entry(sequence_key(&seq))
            .or_insert_with(|| evaluate(&scenario_of(&seq)))
            .clone();
        let fired = set(&ev.fired);
        lost.push(union.difference(&fired).copied().collect());
        let predicted: BTreeSet<Detector> = triple_sets[i / 4].union(&d_set).copied().collect();
        mr_lost.push(predicted.difference(&fired).copied().collect());
        mr_gained.push(fired.difference(&predicted).copied().collect());
        orders.push(ev);
    }
    let any: BTreeSet<Detector> = orders
        .iter()
        .flat_map(|e| e.fired.iter().copied())
        .collect();
    let replayed_rejections = cache.values().map(|e| e.rejections_checked).sum();
    QuadRow {
        group: p.group,
        label: p.label.clone(),
        muts: p.muts,
        singles,
        union: union.clone(),
        triple_sets,
        base_lost,
        orders,
        distinct: cache.len(),
        replayed_rejections,
        lost,
        gained: any.difference(&union).copied().collect(),
        mr_lost,
        mr_gained,
    }
}

struct QuadMatrix {
    plan_sha256: String,
    quads: Vec<QuadRow>,
}

fn quad_matrix() -> &'static QuadMatrix {
    static M: OnceLock<QuadMatrix> = OnceLock::new();
    M.get_or_init(|| {
        let tm = triple_matrix();
        let p = plan(tm);
        let mut ms: Vec<Mutation> = Vec::new();
        for q in &p {
            for m in q.muts {
                if !ms.contains(&m) {
                    ms.push(m);
                }
            }
        }
        let singles_of: BTreeMap<String, Vec<Detector>> = par_map(&ms, |m| {
            (sequence_key(&[*m]), evaluate(&scenario_of(&[*m])).fired)
        })
        .into_iter()
        .collect();
        let quads = par_map(&p, |q| quad_row(q, tm, &singles_of));
        QuadMatrix {
            plan_sha256: sha_hex(&render_plan(&p)),
            quads,
        }
    })
}

fn render_g22_matrix(m: &QuadMatrix) -> String {
    let mut out = format!("{G22_MATRIX_VERSION}|plan|{}\n", m.plan_sha256);
    for q in &m.quads {
        out.push_str(&format!(
            "{G22_MATRIX_VERSION}|quad|{:?}|{}|a={:?}|b={:?}|c={:?}|d={:?}|union={:?}|base_lost={:?}|gained={:?}|commutes={}|position_dependent={:#08b}|reexposed={:?}|surviving={:?}|distinct={}\n",
            q.group,
            q.label,
            q.singles[0],
            q.singles[1],
            q.singles[2],
            q.singles[3],
            q.union,
            q.base_lost,
            q.gained,
            q.commutes(),
            q.position_dependent(),
            q.reexposed(),
            q.surviving(),
            q.distinct,
        ));
        for (i, ev) in q.orders.iter().enumerate() {
            out.push_str(&format!(
                "{G22_MATRIX_VERSION}|order|{}|{}|{:?}|triple={:?}|lost={:?}|mr_lost={:?}|mr_gained={:?}|{}|{}|{}|{}\n",
                q.label,
                quad_order_label(quad_orders()[i]),
                ev.fired,
                q.triple_sets[i / 4],
                q.lost[i],
                q.mr_lost[i],
                q.mr_gained[i],
                ev.corpus_sha256,
                ev.permutations_sha256,
                ev.transcript_sha256,
                ev.rejections_checked,
            ));
        }
    }
    out
}

/// Distinct detectors over all orders of a per-order list.
fn flat(v: &[Vec<Detector>]) -> Vec<Detector> {
    v.iter()
        .flatten()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Machine-readable dump of the frozen lists (used once to freeze them;
/// printed with `G22_DUMP=1`).
fn dump_frozen(m: &QuadMatrix) -> String {
    let mut out = String::new();
    for q in &m.quads {
        out.push_str(&format!(
            "Q|{}|{:?}|lost={:?}|lostmask={:#026b}|gained={:?}|mr_lost={:?}|mr_gained={:?}|mrmask={:#026b}|posdep={:#08b}|ncmask={:#026b}|undet={:#026b}|reexposed={:?}|surviving={:?}|distinct={}|rej={}|union={:?}\n",
            q.label,
            q.group,
            flat(&q.lost),
            q.mask24(|i| !q.lost[i].is_empty()),
            q.gained,
            flat(&q.mr_lost),
            flat(&q.mr_gained),
            q.mask24(|i| !q.mr_lost[i].is_empty() || !q.mr_gained[i].is_empty()),
            q.position_dependent(),
            q.mask24(|i| q.orders[i].fired != q.orders[0].fired),
            q.mask24(|i| q.orders[i].fired.is_empty()),
            q.reexposed(),
            q.surviving(),
            q.distinct,
            q.replayed_rejections,
            q.union,
        ));
    }
    out.push_str(&format!("PLAN|{}\n", m.plan_sha256));
    out
}

// --- Frozen expected G22 results (negatives kept). ------------------------

/// Surviving G21 masks from the catalogue / net-null groups (bases of
/// the catalogue quadruples).
const FROZEN_G22_CATALOGUE_BASES: usize = 24;
/// Union of the singles of the detection positive-control quadruple.
const FROZEN_G22_POSITIVE_CONTROL_UNION: &[Detector] = &[
    CorpusDigest,
    PermutationDigest,
    OracleAgreement,
    TranscriptDigest,
];
/// Distinct production replays (incl. the unmutated control) and the
/// production rejections they check.
const FROZEN_G22_DISTINCT_REPLAYS: usize = 2_923;
const FROZEN_G22_REJECTIONS_CHECKED: usize = 273_700;
/// MR-1 falsifications: `(label, predicted-but-absent, present-but-not-
/// predicted, bitmask over the 24 insertion orders)`. Negatives, kept.
#[allow(clippy::type_complexity)]
#[rustfmt::skip]
const FROZEN_QUAD_FALSIFIED: &[(&str, &[Detector], &[Detector], u32)] = &[
    ("permutation_index_overwrite+duplicate_permutation+third_corpus_flip+fourth_order1_onto_order0", &[PermutationShape, PermutationDistinct], &[], 0b111101001111111101100010),
    ("permutation_index_overwrite+duplicate_permutation+third_expectation_retarget+fourth_order1_onto_order0", &[PermutationShape, PermutationDistinct], &[], 0b111101001111111101100010),
    ("permutation_index_overwrite+duplicate_permutation+third_index_swap+fourth_order1_onto_order0", &[PermutationShape, PermutationDistinct], &[], 0b111101001111111101100010),
    ("permutation_index_overwrite+duplicate_permutation+third_index_overwrite+fourth_order1_onto_order0", &[PermutationDistinct], &[], 0b011100000111001100000000),
    ("permutation_index_overwrite+duplicate_permutation+third_repeat_control+fourth_order1_onto_order0", &[PermutationDistinct], &[], 0b011100000111001100000000),
    ("permutation_index_swap+permutation_index_swap+third_corpus_flip+fourth_order1_onto_order0", &[PermutationDistinct], &[], 0b010001000110001001100010),
    ("permutation_index_swap+permutation_index_swap+third_expectation_retarget+fourth_order1_onto_order0", &[PermutationDistinct], &[], 0b010001000110001001100010),
    ("permutation_index_swap+duplicate_permutation+third_corpus_flip+fourth_order0_swap", &[PermutationDistinct], &[PermutationDistinct], 0b110010001110111010001100),
    ("permutation_index_swap+duplicate_permutation+third_corpus_flip+fourth_order1_onto_order0", &[PermutationDistinct], &[], 0b011100000111001100000000),
    ("permutation_index_swap+duplicate_permutation+third_expectation_retarget+fourth_order0_swap", &[PermutationDistinct], &[PermutationDistinct], 0b110010001110111010001100),
    ("permutation_index_swap+duplicate_permutation+third_expectation_retarget+fourth_order1_onto_order0", &[PermutationDistinct], &[], 0b011100000111001100000000),
    ("permutation_index_swap+duplicate_permutation+third_index_swap+fourth_order0_swap", &[PermutationDistinct], &[PermutationDistinct], 0b110010001110111010001100),
    ("permutation_index_swap+duplicate_permutation+third_index_swap+fourth_order1_onto_order0", &[PermutationDistinct], &[], 0b011100000111001100000000),
    ("permutation_index_swap+duplicate_permutation+third_index_overwrite+fourth_order0_swap", &[PermutationDistinct], &[PermutationDistinct], 0b110010001110111010001100),
    ("permutation_index_swap+duplicate_permutation+third_index_overwrite+fourth_order1_onto_order0", &[PermutationDistinct], &[], 0b011100000111001100000000),
    ("permutation_index_swap+duplicate_permutation+third_repeat_control+fourth_order0_swap", &[PermutationDistinct], &[PermutationDistinct], 0b110010001110111010001100),
    ("permutation_index_swap+duplicate_permutation+third_repeat_control+fourth_order1_onto_order0", &[PermutationDistinct], &[], 0b011100000111001100000000),
    ("duplicate_permutation+repeated_positive_control+third_corpus_flip+fourth_order0_swap", &[PermutationDistinct], &[], 0b100000001000110000000000),
    ("duplicate_permutation+repeated_positive_control+third_corpus_flip+fourth_order1_onto_order0", &[PermutationShape, PermutationDistinct], &[], 0b010011110110001011111111),
    ("duplicate_permutation+repeated_positive_control+third_expectation_retarget+fourth_order0_swap", &[PermutationDistinct], &[], 0b100000001000110000000000),
    ("duplicate_permutation+repeated_positive_control+third_expectation_retarget+fourth_order1_onto_order0", &[PermutationShape, PermutationDistinct], &[], 0b010011110110001011111111),
    ("duplicate_permutation+repeated_positive_control+third_index_swap+fourth_order0_swap", &[PermutationDistinct], &[], 0b100000001000110000000000),
    ("duplicate_permutation+repeated_positive_control+third_index_swap+fourth_order1_onto_order0", &[PermutationShape, PermutationDistinct], &[], 0b010011110110001011111111),
    ("duplicate_permutation+repeated_positive_control+third_index_overwrite+fourth_order0_swap", &[PermutationDistinct], &[], 0b100000001000110000000000),
    ("duplicate_permutation+repeated_positive_control+third_index_overwrite+fourth_order1_onto_order0", &[PermutationDistinct], &[], 0b000001110000000001110011),
    ("duplicate_permutation+repeated_positive_control+third_repeat_control+fourth_order0_swap", &[PermutationDistinct], &[], 0b100000001000110000000000),
    ("duplicate_permutation+repeated_positive_control+third_repeat_control+fourth_order1_onto_order0", &[PermutationDistinct], &[], 0b000001110000000001110011),
    ("permutation_index_swap+permutation_index_swap+silent_expectation_retarget+fourth_order1_onto_order0", &[PermutationDistinct], &[], 0b010001000110001001100010),
    ("q10:permutation_index_swap+duplicate_permutation+third_index_overwrite+RepeatControl { perm: 0 }", &[PermutationDistinct], &[], 0b000010000000000010001100),
    ("q13:permutation_index_swap+duplicate_permutation+third_index_swap+RepeatControl { perm: 0 }", &[PermutationDistinct], &[], 0b000010000000000010001100),
    ("q14:duplicate_permutation+repeated_positive_control+third_index_swap+RepeatControl { perm: 1 }", &[], &[PermutationDistinct], 0b000011000000000011101110),
    ("netnull:third_index_swap+third_index_swap+fourth_free_swap+fourth_free_swap", &[PermutationDigest, TranscriptDigest], &[], 0b111111111111111111111111),
];
/// Quadruples whose detector set depends on where `d` is inserted:
/// `(label, bitmask over the six relative orders of a, b, c)`.
#[rustfmt::skip]
const FROZEN_QUAD_POSITION_DEPENDENT: &[(&str, u8)] = &[
    ("permutation_index_overwrite+duplicate_permutation+third_corpus_flip+fourth_order1_onto_order0", 0b111111),
    ("permutation_index_overwrite+duplicate_permutation+third_expectation_retarget+fourth_order1_onto_order0", 0b111111),
    ("permutation_index_overwrite+duplicate_permutation+third_index_swap+fourth_order1_onto_order0", 0b111111),
    ("permutation_index_overwrite+duplicate_permutation+third_index_overwrite+fourth_order1_onto_order0", 0b101100),
    ("permutation_index_overwrite+duplicate_permutation+third_repeat_control+fourth_order1_onto_order0", 0b101100),
    ("permutation_index_swap+permutation_index_swap+third_corpus_flip+fourth_order1_onto_order0", 0b111111),
    ("permutation_index_swap+permutation_index_swap+third_expectation_retarget+fourth_order1_onto_order0", 0b111111),
    ("permutation_index_swap+duplicate_permutation+third_corpus_flip+fourth_order0_swap", 0b111111),
    ("permutation_index_swap+duplicate_permutation+third_corpus_flip+fourth_order1_onto_order0", 0b101100),
    ("permutation_index_swap+duplicate_permutation+third_expectation_retarget+fourth_order0_swap", 0b111111),
    ("permutation_index_swap+duplicate_permutation+third_expectation_retarget+fourth_order1_onto_order0", 0b101100),
    ("permutation_index_swap+duplicate_permutation+third_index_swap+fourth_order0_swap", 0b111111),
    ("permutation_index_swap+duplicate_permutation+third_index_swap+fourth_order1_onto_order0", 0b101100),
    ("permutation_index_swap+duplicate_permutation+third_index_overwrite+fourth_order0_swap", 0b111111),
    ("permutation_index_swap+duplicate_permutation+third_index_overwrite+fourth_order1_onto_order0", 0b101100),
    ("permutation_index_swap+duplicate_permutation+third_repeat_control+fourth_order0_swap", 0b111111),
    ("permutation_index_swap+duplicate_permutation+third_repeat_control+fourth_order1_onto_order0", 0b101100),
    ("duplicate_permutation+repeated_positive_control+third_corpus_flip+fourth_order0_swap", 0b101100),
    ("duplicate_permutation+repeated_positive_control+third_corpus_flip+fourth_order1_onto_order0", 0b111111),
    ("duplicate_permutation+repeated_positive_control+third_expectation_retarget+fourth_order0_swap", 0b101100),
    ("duplicate_permutation+repeated_positive_control+third_expectation_retarget+fourth_order1_onto_order0", 0b111111),
    ("duplicate_permutation+repeated_positive_control+third_index_swap+fourth_order0_swap", 0b101100),
    ("duplicate_permutation+repeated_positive_control+third_index_swap+fourth_order1_onto_order0", 0b111111),
    ("duplicate_permutation+repeated_positive_control+third_index_overwrite+fourth_order0_swap", 0b101100),
    ("duplicate_permutation+repeated_positive_control+third_index_overwrite+fourth_order1_onto_order0", 0b010011),
    ("duplicate_permutation+repeated_positive_control+third_repeat_control+fourth_order0_swap", 0b101100),
    ("duplicate_permutation+repeated_positive_control+third_repeat_control+fourth_order1_onto_order0", 0b010011),
    ("permutation_index_swap+permutation_index_swap+silent_expectation_retarget+fourth_order1_onto_order0", 0b111111),
    ("q10:permutation_index_swap+duplicate_permutation+third_index_overwrite+RepeatControl { perm: 0 }", 0b010011),
    ("q13:permutation_index_swap+duplicate_permutation+third_index_swap+RepeatControl { perm: 0 }", 0b010011),
    ("q14:duplicate_permutation+repeated_positive_control+third_index_swap+RepeatControl { perm: 1 }", 0b010011),
];
/// Quadruples undetected in some order: `(label, bitmask over the 24
/// orders)`. Every one is asserted net-null.
#[rustfmt::skip]
const FROZEN_QUAD_UNDETECTED: &[(&str, u32)] = &[
    ("permutation_index_swap+permutation_index_swap+silent_expectation_retarget+fourth_expectation_restore", 0b111111111111111111111111),
    ("third_index_swap+third_index_swap+silent_expectation_retarget+fourth_expectation_restore", 0b111111111111111111111111),
    ("netnull:permutation_index_swap+permutation_index_swap+fourth_expectation_restore+silent_expectation_retarget", 0b111111111111111111111111),
    ("netnull:third_index_swap+third_index_swap+fourth_free_swap+fourth_free_swap", 0b111111111111111111111111),
];
/// Per group: `[quads, non-union, non-commuting, re-exposed, surviving,
/// MR-1 falsified, position-dependent, undetected, distinct sequences]`.
#[rustfmt::skip]
const FROZEN_QUAD_GROUP_COUNTS: &[(&str, [usize; 9])] = &[
    ("Catalogue(Independent)", [48, 39, 30, 9, 39, 0, 0, 0, 1056]),
    ("Catalogue(Interacting)", [72, 67, 48, 28, 64, 28, 28, 2, 1500]),
    ("Seeded", [16, 12, 10, 5, 12, 3, 3, 0, 324]),
    ("PositiveControl", [1, 0, 0, 0, 0, 0, 0, 0, 24]),
    ("NetNull", [2, 2, 0, 0, 1, 1, 0, 2, 18]),
];
/// Histogram of detectors lost (vs the four singles' union) per quad.
#[rustfmt::skip]
const FROZEN_QUAD_LOST_HISTOGRAM: &[(&[Detector], usize)] = &[
    (&[PermutationShape, PermutationDistinct], 6),
    (&[PermutationDistinct], 82),
    (&[PermutationDigest, TranscriptDigest], 11),
    (&[OracleAgreement], 21),
];
/// Histogram of base-mask detectors that survive the fourth mutation.
#[rustfmt::skip]
const FROZEN_QUAD_SURVIVING_HISTOGRAM: &[(&[Detector], usize)] = &[
    (&[PermutationDistinct], 85),
    (&[PermutationDigest, TranscriptDigest], 10),
    (&[OracleAgreement], 21),
];
/// Histogram of base-mask detectors re-exposed by the fourth mutation.
#[rustfmt::skip]
const FROZEN_QUAD_REEXPOSED_HISTOGRAM: &[(&[Detector], usize)] = &[
    (&[PermutationDistinct], 21),
    (&[PermutationDigest, TranscriptDigest], 16),
    (&[OracleAgreement], 5),
];

// --- Tests. ----------------------------------------------------------------

fn group_counts(m: &QuadMatrix) -> Vec<(String, [usize; 9])> {
    let mut out: BTreeMap<QuadGroup, [usize; 9]> = BTreeMap::new();
    for q in &m.quads {
        let c = out.entry(q.group).or_insert([0; 9]);
        c[0] += 1;
        c[1] += usize::from(!q.is_union());
        c[2] += usize::from(!q.commutes());
        c[3] += usize::from(!q.reexposed().is_empty());
        c[4] += usize::from(!q.surviving().is_empty());
        c[5] += usize::from(q.falsified());
        c[6] += usize::from(q.position_dependent() != 0);
        c[7] += usize::from(q.orders.iter().any(|e| e.fired.is_empty()));
        c[8] += q.distinct;
    }
    out.into_iter()
        .map(|(g, c)| (format!("{g:?}"), c))
        .collect()
}

fn histogram(lists: impl Iterator<Item = Vec<Detector>>) -> Vec<(Vec<Detector>, usize)> {
    let mut h: BTreeMap<Vec<Detector>, usize> = BTreeMap::new();
    for l in lists.filter(|l| !l.is_empty()) {
        *h.entry(l).or_default() += 1;
    }
    h.into_iter().collect()
}

#[test]
fn frozen_sources_and_g17_to_g21_digests_are_reproduced_on_the_control() {
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
    assert!(m.control.fired.is_empty(), "positive control fired");
    assert_eq!(sha_hex(&render_g19_matrix(m)), G19_FROZEN_MATRIX_SHA256);
    assert_eq!(sha_hex(&render_g20_matrix(m)), G20_FROZEN_MATRIX_SHA256);
    let masked: Vec<(&str, &str)> = m
        .pairs
        .iter()
        .filter(|p| !p.delta.is_union())
        .map(|p| (p.a, p.b))
        .collect();
    let want: Vec<(&str, &str)> = FROZEN_MASKED.iter().map(|r| (r.0, r.1)).collect();
    assert_eq!(masked, want, "G20 masking list");
    let non_commuting: Vec<(&str, &str)> = m
        .pairs
        .iter()
        .filter(|p| !p.commutes())
        .map(|p| (p.a, p.b))
        .collect();
    assert_eq!(non_commuting, FROZEN_NON_COMMUTING.to_vec());
    let undetected: Vec<(&str, &str)> = m
        .pairs
        .iter()
        .filter(|p| p.undetected())
        .map(|p| (p.a, p.b))
        .collect();
    assert_eq!(undetected, FROZEN_UNDETECTED.to_vec());
    let tm = triple_matrix();
    assert!(tm.control.fired.is_empty());
    assert_eq!(G21_FROZEN_MATRIX_SHA256, G21_FROZEN_MATRIX_SHA256_PIN);
    assert_eq!(
        sha_hex(&render_g21_matrix(tm)),
        G21_FROZEN_MATRIX_SHA256_PIN,
        "re-implemented harness must reproduce the frozen G21 matrix"
    );
    reprove_g21_frozen_lists(tm);
}

#[test]
fn g22_plan_is_preregistered_bounded_and_deterministic() {
    let tm = triple_matrix();
    let pool = surviving_masks(tm);
    // Bases: every surviving G21 mask, which contains every G21
    // order-dependent triple.
    assert_eq!(pool.len(), FROZEN_TRIPLE_LOSSES.len());
    for (l, _) in FROZEN_TRIPLE_NON_COMMUTING {
        assert!(pool.iter().any(|t| t.label == *l), "{l} is a base");
    }
    let cat_bases = pool
        .iter()
        .filter(|t| matches!(t.group, Group::Catalogue | Group::NetNull))
        .count();
    assert_eq!(cat_bases, FROZEN_G22_CATALOGUE_BASES);
    let p = plan(tm);
    assert_eq!(
        p.len(),
        cat_bases * fourths().len() + G22_SEEDED_QUADS + 1 + 2,
        "bounded catalogue"
    );
    let labels: BTreeSet<&str> = p.iter().map(|q| q.label.as_str()).collect();
    assert_eq!(labels.len(), p.len(), "labels are unique");
    assert_eq!(
        sha_hex(&render_plan(&p)),
        G22_FROZEN_PLAN_SHA256,
        "pre-registered plan (bases, fourths, seed, controls) is frozen"
    );
    assert_eq!(quad_matrix().plan_sha256, G22_FROZEN_PLAN_SHA256);
    assert_eq!(seeded_quads(pool.len()), seeded_quads(pool.len()));

    // The 24 insertion orders are exactly the permutations of a, b, c, d.
    let orders = quad_orders();
    let distinct: BTreeSet<[usize; 4]> = orders.iter().copied().collect();
    assert_eq!(distinct.len(), 24);
    for o in &orders {
        let mut s = *o;
        s.sort_unstable();
        assert_eq!(s, [0, 1, 2, 3]);
    }

    // Independent fourths touch nothing the catalogue pairs or thirds use.
    let free = fourth_free_row();
    assert!(is_independent_row(free));
    let free_name = shared::row_names()[free];
    assert!(free_name != THIRD_FLIP_ROW && free_name != THIRD_EXPECT_ROW);
    let used_perms: BTreeSet<usize> = thirds()
        .iter()
        .flat_map(|(_, t)| match *t {
            Mutation::PermIndexOverwrite { perm, .. }
            | Mutation::PermIndexSwap { perm, .. }
            | Mutation::RepeatControl { perm } => vec![perm],
            Mutation::PermDuplicate { src, dst } => vec![src, dst],
            _ => vec![],
        })
        .chain([0, 1])
        .collect();
    assert!(!used_perms.contains(&FOURTH_FREE_PERM));
    for (name, class, f) in fourths() {
        if class == FourthClass::Independent {
            match f {
                Mutation::ExpectationRetarget { row, to } => {
                    assert_eq!(row, free, "{name}");
                    assert_ne!(
                        shared::expected(shared::row_names()[to]),
                        shared::expected(free_name),
                        "{name} is a real change"
                    );
                }
                Mutation::PermIndexSwap { perm, .. } => assert_eq!(perm, FOURTH_FREE_PERM),
                _ => unreachable!("independent fourths are a retarget and a swap"),
            }
        }
    }
}

#[test]
fn unmutated_positive_control_and_detection_positive_control() {
    let tm = triple_matrix();
    assert!(tm.control.fired.is_empty());
    assert_eq!(tm.control.corpus_sha256, G17_FROZEN_CORPUS_SHA256);
    assert_eq!(tm.control.transcript_sha256, G18_FROZEN_TRANSCRIPT_SHA256);
    let v = quad_oracle(&[]);
    assert!(v.fired.is_empty() && v.net_null);
    assert_eq!(v.rejections, tm.control.rejections_checked);
    let m = quad_matrix();
    let pc: Vec<&QuadRow> = m
        .quads
        .iter()
        .filter(|q| q.group == QuadGroup::PositiveControl)
        .collect();
    assert_eq!(pc.len(), 1);
    let pc = pc[0];
    assert!(
        pc.singles.iter().all(|s| !s.is_empty()),
        "every single fires"
    );
    assert!(pc.is_union(), "detection control is the exact union");
    assert!(pc.commutes(), "detection control commutes in all 24 orders");
    assert!(!pc.falsified());
    assert_eq!(pc.distinct, 24);
    assert_eq!(
        pc.union.iter().copied().collect::<Vec<_>>(),
        FROZEN_G22_POSITIVE_CONTROL_UNION.to_vec()
    );
}

#[test]
fn typed_quad_oracle_agrees_with_harness_on_every_order() {
    let tm = triple_matrix();
    let m = quad_matrix();
    let frozen = frozen_scenario();
    let checked: Vec<usize> = par_map(&m.quads, |q| {
        for (i, ev) in q.orders.iter().enumerate() {
            let seq = q.sequence(i);
            let v = quad_oracle(&seq);
            assert_eq!(v.fired, ev.fired, "{} order {i}: detector set", q.label);
            assert_eq!(
                v.rejections, ev.rejections_checked,
                "{}: rejections",
                q.label
            );
            assert_eq!(
                v.net_null,
                scenario_of(&seq) == frozen,
                "{} order {i}: net-null",
                q.label
            );
        }
        for (k, s) in q.singles.iter().enumerate() {
            assert_eq!(quad_oracle(&[q.muts[k]]).fired, *s, "{}: single", q.label);
        }
        q.orders.len()
    });
    assert_eq!(checked.iter().sum::<usize>(), m.quads.len() * 24);
    // The G22 oracle also reproduces every G21 triple order.
    for t in &tm.triples {
        for (k, ev) in t.orders.iter().enumerate() {
            assert_eq!(quad_oracle(&t.sequence(k)).fired, ev.fired, "{}", t.label);
        }
    }
}

#[test]
fn metamorphic_relations_lost_gained_and_negatives_are_frozen() {
    let m = quad_matrix();

    // MR-1 (pre-registered): with an independent fourth `d`, every order
    // fires exactly `triple(relative order of a, b, c) ∪ d`, whatever the
    // insertion position of `d`. Holds for every independent quadruple.
    for q in m.quads.iter().filter(|q| {
        matches!(
            q.group,
            QuadGroup::Catalogue(FourthClass::Independent) | QuadGroup::PositiveControl
        )
    }) {
        assert!(!q.falsified(), "{}: MR-1 falsified", q.label);
        assert_eq!(q.position_dependent(), 0, "{}: position-dependent", q.label);
    }

    // MR-1 falsifications (negatives kept): only interacting / seeded
    // fourths and one net-null negative.
    let falsified: Vec<(&str, Vec<Detector>, Vec<Detector>, u32)> = m
        .quads
        .iter()
        .filter(|q| q.falsified())
        .map(|q| {
            (
                q.label.as_str(),
                flat(&q.mr_lost),
                flat(&q.mr_gained),
                q.mask24(|i| !q.mr_lost[i].is_empty() || !q.mr_gained[i].is_empty()),
            )
        })
        .collect();
    let want: Vec<(&str, Vec<Detector>, Vec<Detector>, u32)> = FROZEN_QUAD_FALSIFIED
        .iter()
        .map(|(l, a, b, k)| (*l, a.to_vec(), b.to_vec(), *k))
        .collect();
    assert_eq!(falsified, want, "frozen MR-1 falsifications");

    // MR-2: the insertion position of `d` matters only for these quads.
    let posdep: Vec<(&str, u8)> = m
        .quads
        .iter()
        .filter(|q| q.position_dependent() != 0)
        .map(|q| (q.label.as_str(), q.position_dependent()))
        .collect();
    assert_eq!(
        posdep,
        FROZEN_QUAD_POSITION_DEPENDENT.to_vec(),
        "frozen position dependence"
    );

    // No quadruple ever gains a detector outside its four singles' union.
    assert!(m.quads.iter().all(|q| q.gained.is_empty()));

    // Undetected orders (negatives kept): every one is net-null.
    let undetected: Vec<(&str, u32)> = m
        .quads
        .iter()
        .filter(|q| q.orders.iter().any(|e| e.fired.is_empty()))
        .map(|q| (q.label.as_str(), q.mask24(|i| q.orders[i].fired.is_empty())))
        .collect();
    assert_eq!(
        undetected,
        FROZEN_QUAD_UNDETECTED.to_vec(),
        "frozen undetected"
    );
    for q in &m.quads {
        for (i, ev) in q.orders.iter().enumerate() {
            if ev.fired.is_empty() {
                assert!(
                    scenario_of(&q.sequence(i)) == frozen_scenario(),
                    "{}: undetected order {} is not net-null",
                    q.label,
                    quad_order_label(quad_orders()[i])
                );
            }
        }
    }
    for q in m.quads.iter().filter(|q| q.group == QuadGroup::NetNull) {
        assert!(q.orders.iter().all(|e| e.fired.is_empty()), "{}", q.label);
    }

    // Per-group counts and lost / surviving / re-exposed histograms.
    let counts = group_counts(m);
    let want: Vec<(String, [usize; 9])> = FROZEN_QUAD_GROUP_COUNTS
        .iter()
        .map(|(g, c)| ((*g).to_string(), *c))
        .collect();
    assert_eq!(counts, want, "frozen per-group counts");
    let h = |f: &dyn Fn(&QuadRow) -> Vec<Detector>| histogram(m.quads.iter().map(f));
    let lost = h(&|q| flat(&q.lost));
    let surviving = h(&|q| q.surviving());
    let reexposed = h(&|q| q.reexposed());
    let as_vec = |v: &[(&[Detector], usize)]| -> Vec<(Vec<Detector>, usize)> {
        v.iter().map(|(d, c)| (d.to_vec(), *c)).collect()
    };
    assert_eq!(lost, as_vec(FROZEN_QUAD_LOST_HISTOGRAM), "lost vs singles");
    assert_eq!(surviving, as_vec(FROZEN_QUAD_SURVIVING_HISTOGRAM));
    assert_eq!(reexposed, as_vec(FROZEN_QUAD_REEXPOSED_HISTOGRAM));

    // Negative kept: the state invariant never fires.
    for q in &m.quads {
        for ev in &q.orders {
            assert!(!ev.fired.contains(&Detector::StateInvariant));
        }
    }
}

#[test]
fn every_rejected_production_replay_preserves_swc2_swe1_stats_and_anchor() {
    let m = quad_matrix();
    let mut replays = 1;
    let mut rejections = triple_matrix().control.rejections_checked;
    for q in &m.quads {
        for ev in &q.orders {
            assert!(ev.rejections_checked > 0, "replay reached rejections");
            assert_eq!(
                ev.nonmutation_violations, 0,
                "{}: SWC2/SWE1/stats must be preserved on every rejection",
                q.label
            );
        }
        replays += q.distinct;
        rejections += q.replayed_rejections;
    }
    // The anchor is asserted unchanged inside G17's production adapter on
    // every replay (a violation panics before reaching this point).
    eprintln!(
        "G22: {} quadruples, {} orders, {replays} distinct production replays (incl. control), {rejections} production rejections checked",
        m.quads.len(),
        m.quads.len() * 24
    );
    assert_eq!(replays, FROZEN_G22_DISTINCT_REPLAYS);
    assert_eq!(rejections, FROZEN_G22_REJECTIONS_CHECKED);
    assert_eq!(sha_hex(&shared::rendering()), G17_FROZEN_CORPUS_SHA256);
    assert_eq!(
        sha_hex(&render_permutations(&frozen_permutations())),
        G18_FROZEN_PERMUTATIONS_SHA256
    );
}

#[test]
fn g22_matrix_is_deterministic_and_pinned() {
    let m = quad_matrix();
    let q0 = &m.quads[0];
    let again = evaluate(&scenario_of(&q0.sequence(23)));
    assert_eq!(again.fired, q0.orders[23].fired);
    let rendered = render_g22_matrix(m);
    if std::env::var_os("G22_DUMP").is_some() {
        eprintln!("{}", dump_frozen(m));
        eprintln!("G22 matrix sha256 {}", sha_hex(&rendered));
    }
    assert_eq!(
        sha_hex(&rendered),
        G22_FROZEN_MATRIX_SHA256,
        "frozen G22 matrix digest (bump G22_MATRIX_VERSION and re-freeze on change)"
    );
}
