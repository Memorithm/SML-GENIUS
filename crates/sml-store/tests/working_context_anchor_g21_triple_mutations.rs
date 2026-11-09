//! CONTEXT-2 G21: triple-perturbation completeness of the frozen
//! mutation-to-detector matrices (G19 singles, G20 pairs) over the frozen
//! G18 permutation / transcript guard and the frozen G17 corpus, on the
//! versioned anchor admission path
//! (`restore_into_anchored_with_versioned_provenance`).
//!
//! G19 (PR #898) froze the detector set of each single mutation; G20
//! (PR #900, squash `0257f530`) froze the paired matrix and found five
//! masking pairs, three of them order-dependent and one (the swap
//! self-pair) net-null and undetected. G21 asks what happens when an
//! **independent third mutation** is added to each of those pairs: does
//! the third re-expose the masked detector, does the masking survive, and
//! does the application order still matter? Every triple is applied in
//! all six orders.
//!
//! * **Catalogue triples** (30): each of the five G20 masking pairs
//!   ([`FROZEN_MASKED`], which contains the three order-dependent pairs and
//!   the net-null swap self-pair) plus each of six independent thirds
//!   ([`thirds`]: corpus flip, expectation retarget, index swap, index
//!   overwrite, duplicate, repeated control). Thirds touch neither order 0
//!   nor order 1 nor the G19 row under mutation, the retarget source or
//!   the positive control.
//! * **Seeded triples** (24): SplitMix64 seed [`G21_SEED`] picks the
//!   masking pair, the kind of third and its parameters (orders 2..8,
//!   independent rows).
//! * **Positive control**: the unmutated scenario fires nothing and
//!   reproduces every frozen digest; a detection control triple of three
//!   mutually independent thirds must be the exact union of its singles in
//!   all six orders.
//! * **Net-null negatives** (2): a swap involution plus a value-preserving
//!   expectation retarget (to an equal expectation). Kept on purpose.
//! * **Independent typed oracle**: [`triple_oracle`] (new for G21, shares
//!   no code with G20's oracle, the harness or production) predicts the
//!   detector set, rejection count and net-null status of every order.
//! * Per triple: `lost` detectors per order (in the union of the singles
//!   but absent), `gained` detectors, order dependence, undetected orders,
//!   and whether the triple equals `pair ∪ third` in each order. Frozen
//!   lists are asserted exactly and the full rendering is pinned by
//!   SHA-256 ([`G21_FROZEN_MATRIX_SHA256`]).
//!
//! Frozen results (matrix v1, SHA-256 `e9e53022…`), produced by this
//! suite:
//!
//! * 57 triples × 6 orders = 342 triple evaluations; the typed oracle
//!   agrees with the harness on all of them.
//! * In every order of every non-net-null triple, the detector set equals
//!   the pair's set (in that relative order) united with the third's set:
//!   no exception. No triple gains a detector outside its singles' union.
//! * 22 of 30 catalogue and 17 of 24 seeded triples still lose a single
//!   detector (negative results, inherited from the pair's mask):
//!   `PermutationDistinct` in 24 triples, `OracleAgreement` in 10,
//!   `PermutationDigest` + `TranscriptDigest` in 7 (incl. the 2 net-null
//!   negatives). The other 8 catalogue and 7 seeded triples are exact
//!   unions because the third itself fires the masked detector.
//! * 24 triples (15 catalogue, 9 seeded) are order-dependent, always in
//!   exactly the orders where `b` precedes `a` (bitmask `0b101100`).
//! * Only the 2 net-null negatives are undetected (all six orders), and
//!   each is asserted net-null. With an independent third, the G20 swap
//!   self-pair is never undetected, but it adds nothing: the triple fires
//!   exactly the third's detectors.
//! * `StateInvariant` fires for no triple (as in G19 / G20).
//!
//! Before any of that is trusted, the re-implemented harness must
//! reproduce the G17 corpus digest, the G18 permutation / transcript
//! digests, the G19 matrix digest and the G20 matrix digest (`020d1f0d…`)
//! and masking list. The G17, G18, G19 and G20 sources are pinned by
//! SHA-256.
//!
//! On every production rejection reached in every triple evaluation, the
//! shared target's canonical SWC2 encoding, SWE1 seal and all
//! `WorkingContextStats` are preserved byte-for-byte, and G17's adapter
//! asserts that the anchor state is never advanced.
//!
//! Limits: as in G19 / G20, mutations act on the *test-side* scenario;
//! production always replays the frozen G17 rows. Authorities and anchors
//! are in-process mocks; SWE1 is unkeyed SHA-256. Harness-level guards,
//! not production admission gates. No migration, signature, attestation,
//! transport, remote execution or E2E claim. `NOT_MEASURED`. No
//! production change.
//!
//! Assigned by ChatGPT on RemoteOps #1208 (comment 6736254436).

// The harness below (generator, scenario, detectors, typed mutations,
// G20 pairs and renderings) is copied verbatim from G20 and re-proven by
// reproducing the G19 and G20 matrix digests.

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

/// SHA-256 of the frozen G17 / G18 / G19 / G20 test sources at main
/// `9eedaa6d` (G20 merged as `0257f530`).
const FROZEN_SOURCES: [(&str, &[u8], &str); 4] = [
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

// --- Tests. ----------------------------------------------------------------

#[test]
fn frozen_sources_and_g17_to_g20_digests_are_reproduced_on_the_control() {
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
    assert_eq!(m.control.nonmutation_violations, 0);
    assert_eq!(
        sha_hex(&render_g19_matrix(m)),
        G19_FROZEN_MATRIX_SHA256,
        "re-implemented singles must reproduce the frozen G19 matrix"
    );
    assert_eq!(
        sha_hex(&render_g20_matrix(m)),
        G20_FROZEN_MATRIX_SHA256,
        "re-implemented harness must reproduce the frozen G20 matrix"
    );
    // The G20 masking / order-dependence lists the triples build on.
    let masked: Vec<(&str, &str)> = m
        .pairs
        .iter()
        .filter(|p| !p.delta.is_union())
        .map(|p| (p.a, p.b))
        .collect();
    let want: Vec<(&str, &str)> = FROZEN_MASKED.iter().map(|r| (r.0, r.1)).collect();
    assert_eq!(masked, want);
    for nc in FROZEN_NON_COMMUTING {
        assert!(want.contains(nc), "order-dependent pair is a base pair");
    }
    assert!(want.contains(&FROZEN_UNDETECTED[0]));
}

#[test]
fn unmutated_positive_control_and_detection_positive_control() {
    let tm = triple_matrix();
    assert!(tm.control.fired.is_empty());
    assert_eq!(tm.control.corpus_sha256, G17_FROZEN_CORPUS_SHA256);
    assert_eq!(tm.control.transcript_sha256, G18_FROZEN_TRANSCRIPT_SHA256);
    assert_eq!(triple_oracle(&[]).fired, Vec::<Detector>::new());
    assert!(triple_oracle(&[]).net_null);
    let pc: Vec<&TripleRow> = tm
        .triples
        .iter()
        .filter(|t| t.group == Group::PositiveControl)
        .collect();
    assert_eq!(pc.len(), 1);
    let pc = pc[0];
    assert!(pc.is_union(), "positive control triple is the exact union");
    assert!(pc.commutes());
    assert_eq!(
        pc.union.iter().copied().collect::<Vec<_>>(),
        FROZEN_POSITIVE_CONTROL_UNION.to_vec()
    );
    assert!(pc.singles.iter().all(|s| !s.is_empty()));
}

#[test]
fn typed_triple_oracle_agrees_with_harness_on_every_order() {
    let m = matrix();
    let tm = triple_matrix();
    let check = |muts: &[Mutation], ev: &Evaluation| {
        let v = triple_oracle(muts);
        assert_eq!(
            ev.fired, v.fired,
            "{muts:?}: detector set (oracle vs harness)"
        );
        assert_eq!(ev.rejections_checked, v.rejections, "{muts:?}: rejections");
        assert_eq!(
            v.net_null,
            scenario_of(muts) == frozen_scenario(),
            "{muts:?}: net-null"
        );
    };
    check(&[], &m.control);
    for (_, mu, ev) in &m.singles {
        check(&[*mu], ev);
    }
    let mut n = 0;
    for t in &tm.triples {
        for (k, ev) in t.orders.iter().enumerate() {
            check(&t.sequence(k), ev);
            n += 1;
        }
    }
    assert_eq!(n, tm.triples.len() * ORDERS.len());
}

#[test]
fn triple_matrix_lost_gained_order_dependence_and_negatives_are_frozen() {
    let tm = triple_matrix();
    let count = |g: Group| tm.triples.iter().filter(|t| t.group == g).count();
    assert_eq!(
        count(Group::Catalogue),
        FROZEN_MASKED.len() * thirds().len()
    );
    assert_eq!(count(Group::Seeded), G21_SEEDED_TRIPLES);
    assert_eq!(count(Group::PositiveControl), 1);
    assert_eq!(count(Group::NetNull), 2);

    // Third mutations are independent of the catalogue mutations.
    for (_, t) in thirds() {
        match t {
            Mutation::CorpusBytesFlip { row } | Mutation::ExpectationRetarget { row, .. } => {
                assert!(is_independent_row(row));
            }
            Mutation::PermIndexOverwrite { perm, .. }
            | Mutation::PermIndexSwap { perm, .. }
            | Mutation::RepeatControl { perm } => assert!(perm >= 2),
            Mutation::PermDuplicate { src, dst } => assert!(src >= 2 && dst >= 2 && src != dst),
            Mutation::CorpusRowOmitted { .. } => unreachable!("no omission third"),
        }
    }

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
    assert_eq!(non_union, want, "frozen lost/gained list");
    assert!(
        tm.triples.iter().all(|t| t.gained.is_empty()),
        "no triple adds a detector outside the union of its singles"
    );
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
    assert_eq!(
        non_commuting,
        FROZEN_TRIPLE_NON_COMMUTING.to_vec(),
        "frozen order dependence"
    );

    let not_extending: Vec<(&str, u8)> = tm
        .triples
        .iter()
        .filter(|t| t.group != Group::NetNull && !t.extends_pair.iter().all(|&e| e))
        .map(|t| (t.label.as_str(), mask(|k| !t.extends_pair[k])))
        .collect();
    assert_eq!(
        not_extending,
        FROZEN_TRIPLE_NOT_EXTENDING.to_vec(),
        "frozen pair-plus-third independence failures"
    );

    // Undetected orders: only the net-null negatives, and only because the
    // scenario is the frozen one again (otherwise it is a guard escape).
    let undetected: Vec<(&str, u8)> = tm
        .triples
        .iter()
        .filter(|t| !t.undetected_orders().is_empty())
        .map(|t| (t.label.as_str(), mask(|k| t.orders[k].fired.is_empty())))
        .collect();
    assert_eq!(
        undetected,
        FROZEN_TRIPLE_UNDETECTED.to_vec(),
        "frozen undetected list"
    );
    for t in &tm.triples {
        for k in t.undetected_orders() {
            assert!(
                scenario_of(&t.sequence(k)) == frozen_scenario(),
                "{}: undetected order {} is not net-null",
                t.label,
                order_label(ORDERS[k])
            );
        }
    }
    // Negative kept: with an independent third, the net-null swap pair is
    // never undetected, but it contributes nothing: the triple fires
    // exactly the third's own detectors.
    for t in tm.triples.iter().filter(|t| {
        t.group == Group::Catalogue
            && t.label
                .starts_with("permutation_index_swap+permutation_index_swap+")
    }) {
        for ev in &t.orders {
            assert_eq!(ev.fired, t.singles[2], "{}: only the third fires", t.label);
        }
    }

    // Negative kept: the state invariant never fires for any triple.
    for t in &tm.triples {
        for ev in &t.orders {
            assert!(!ev.fired.contains(&Detector::StateInvariant));
        }
    }
}

#[test]
fn every_rejected_production_replay_preserves_swc2_swe1_stats_and_anchor() {
    let tm = triple_matrix();
    let mut evaluations = 1;
    let mut total = tm.control.rejections_checked;
    assert_eq!(tm.control.nonmutation_violations, 0);
    for t in &tm.triples {
        for ev in &t.orders {
            assert!(ev.rejections_checked > 0, "replay reached rejections");
            assert_eq!(
                ev.nonmutation_violations, 0,
                "{}: SWC2/SWE1/stats must be preserved on every rejection",
                t.label
            );
            total += ev.rejections_checked;
            evaluations += 1;
        }
    }
    // The anchor is asserted unchanged inside G17's production adapter on
    // every replay (a violation panics before reaching this point).
    eprintln!(
        "G21: {} triples, {evaluations} triple evaluations (incl. control), {total} production rejections checked",
        tm.triples.len()
    );
    assert_eq!(sha_hex(&shared::rendering()), G17_FROZEN_CORPUS_SHA256);
    assert_eq!(
        sha_hex(&render_permutations(&frozen_permutations())),
        G18_FROZEN_PERMUTATIONS_SHA256
    );
}

#[test]
fn g21_matrix_is_deterministic_and_pinned() {
    let tm = triple_matrix();
    assert_eq!(seeded_triples(), seeded_triples());
    let t0 = &tm.triples[0];
    let again = evaluate(&scenario_of(&t0.sequence(5)));
    assert_eq!(again.fired, t0.orders[5].fired);
    let rendered = render_g21_matrix(tm);
    if std::env::var_os("G21_DUMP").is_some() {
        eprintln!("{rendered}");
    }
    assert_eq!(
        sha_hex(&rendered),
        G21_FROZEN_MATRIX_SHA256,
        "frozen G21 matrix digest (bump G21_MATRIX_VERSION and re-freeze on change)"
    );
}
