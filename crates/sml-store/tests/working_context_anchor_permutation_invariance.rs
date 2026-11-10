//! CONTEXT-2 G18: deterministic permutation invariance and repeated
//! restore/reject sequences over the frozen G17 composed-mutation corpus
//! on the versioned anchor admission path
//! (`restore_into_anchored_with_versioned_provenance`).
//!
//! G17 (PR #891, exact head `f20776f0`, squash `f2b908e8`) checks each
//! corpus row against a **fresh** target. G18 replays the same 13 frozen
//! rows (1 positive control + 12 composed two-gate mutants) through a
//! **single shared live target**, in frozen seeded permutation orders and
//! in repeated runs, and asserts:
//!
//! * **no order dependence**: every row's production outcome equals both
//!   the frozen expected typed outcome and G17's independent pure oracle,
//!   whatever position it occupies and whatever ran before it;
//! * **no state drift**: every rejection leaves the shared target's
//!   canonical SWC2 encoding, SWE1 seal and all `WorkingContextStats`
//!   counters byte-for-byte unchanged, and the anchor state is never
//!   advanced (asserted inside G17's production adapter);
//! * **positive acceptance controls**: the positive control is accepted at
//!   any position (including repeated / after any rejection) and installs
//!   exactly the sealed Home3 snapshot;
//! * **reproducibility**: permutation orders come from a frozen SplitMix64
//!   seed list and are pinned by SHA-256, as is the full outcome
//!   transcript.
//!
//! G18 adds **no** new mutant definitions: the corpus is read through the
//! read-only `shared` module of the G17 file (the G17 tests are therefore
//! also compiled into this test binary and run again unchanged). The
//! G17 frozen corpus digest is re-checked here.
//!
//! Limits: authorities and anchors are in-process mocks; a matching
//! authority tag is an equality binding, not authenticated provenance.
//! SWE1 is unkeyed SHA-256. No migration, signature, attestation,
//! hardware counter, transport or E2E claim. `NOT_MEASURED`. No
//! production change.
//!
//! Assigned by ChatGPT on RemoteOps #1208 (comment 6726396005).

#[path = "working_context_anchor_composed_mutations.rs"]
mod g17;

use g17::shared;
use sml_core::elastic_ngram_paged_sha::sha256;
use sml_store::working_context::WorkingContext;
use sml_store::working_context_envelope::seal_context;

/// Version of this frozen G18 permutation corpus.
const G18_CORPUS_VERSION: u16 = 1;

/// Frozen SplitMix64 seeds, one permutation per seed.
const SEEDS: [u64; 8] = [
    0x4718_0000_0000_0001,
    0x4718_0000_0000_0002,
    0x4718_0000_0000_0003,
    0x4718_0000_0000_0005,
    0x4718_0000_0000_0008,
    0x4718_0000_0000_000D,
    0x4718_0000_0000_0015,
    0x4718_0000_0000_0022,
];

/// How many consecutive times each row is replayed in the repetition test.
const REPEATS: usize = 3;

/// Frozen SHA-256 of the canonical rendering of all seeded permutations.
const FROZEN_PERMUTATIONS_SHA256: &str =
    "b2b50f6c27f14fd4f3009ade800cf6fc7aa3633fb810b837a7564bce443edf4f";
/// Frozen SHA-256 of the full permuted-replay outcome transcript.
const FROZEN_TRANSCRIPT_SHA256: &str =
    "de2098e90bc23b72ecc839d60835ddb966f390014ef2770d1df66574658deaa1";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// SplitMix64 (Steele, Lea, Flood 2014): small, fully specified PRNG.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Unbiased enough for a frozen test order; reproducibility, not
    /// statistical quality, is the requirement.
    fn below(&mut self, n: usize) -> usize {
        let n64 = u64::try_from(n).expect("n fits u64");
        usize::try_from(self.next() % n64).expect("index fits usize")
    }
}

/// Fisher–Yates permutation of `0..n` from `seed`.
fn permutation(seed: u64, n: usize) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..n).collect();
    let mut rng = SplitMix64(seed);
    for i in (1..n).rev() {
        let j = rng.below(i + 1);
        idx.swap(i, j);
    }
    idx
}

fn all_permutations() -> Vec<Vec<usize>> {
    let n = shared::row_names().len();
    SEEDS.iter().map(|&s| permutation(s, n)).collect()
}

fn render_permutations(perms: &[Vec<usize>]) -> String {
    let mut out = String::new();
    for (seed, p) in SEEDS.iter().zip(perms) {
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

/// Apply `name` to the shared `target`, checking outcome and state
/// invariants against the state immediately before the call. Returns the
/// production outcome.
fn step(target: &mut WorkingContext, name: &str, context: &str) -> String {
    let before: Fingerprint = shared::fingerprint_of(target);
    let expected = shared::expected(name);
    let oracle = shared::oracle_outcome(name);
    assert_eq!(oracle, expected, "{context}: oracle vs frozen {name}");
    let prod = shared::production_outcome(target, name);
    assert_eq!(prod, expected, "{context}: production vs frozen {name}");
    assert_eq!(prod, oracle, "{context}: production vs oracle {name}");
    if shared::is_rejection(name) {
        assert_eq!(
            shared::fingerprint_of(target),
            before,
            "{context}: {name} rejection must leave SWC2/SWE1/stats unchanged"
        );
    } else {
        assert_eq!(
            shared::fingerprint_of(target),
            shared::fingerprint_of(&shared::accepted_snapshot()),
            "{context}: {name} acceptance must install exactly Home3"
        );
        assert_eq!(
            seal_context(target).unwrap(),
            shared::bytes(name),
            "{context}: {name} exact sealed snapshot, no replay"
        );
    }
    prod
}

#[test]
fn g17_corpus_is_unchanged_and_permutations_are_frozen() {
    assert_eq!(shared::CORPUS_VERSION, 1, "G18 is pinned to G17 corpus v1");
    assert_eq!(
        hex(&sha256(shared::rendering().as_bytes())),
        shared::CORPUS_SHA256,
        "G17 frozen corpus digest"
    );
    let names = shared::row_names();
    assert_eq!(names.len(), 13, "1 positive control + 12 composed mutants");
    assert_eq!(
        names.iter().filter(|n| !shared::is_rejection(n)).count(),
        1,
        "exactly one positive control"
    );

    let perms = all_permutations();
    assert_eq!(perms, all_permutations(), "seeded orders are deterministic");
    let identity: Vec<usize> = (0..names.len()).collect();
    for (seed, p) in SEEDS.iter().zip(&perms) {
        let mut sorted = p.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, identity, "{seed:#x}: must be a permutation");
    }
    // Distinct orders, none of them the frozen corpus order.
    for (i, a) in perms.iter().enumerate() {
        assert_ne!(a, &identity, "seed {:#x} is the identity order", SEEDS[i]);
        for b in &perms[i + 1..] {
            assert_ne!(a, b, "seeded orders must be pairwise distinct");
        }
    }
    // Positive control appears at different positions across orders.
    let control = names
        .iter()
        .position(|n| !shared::is_rejection(n))
        .expect("positive control");
    let mut positions: Vec<usize> = perms
        .iter()
        .map(|p| p.iter().position(|&i| i == control).unwrap())
        .collect();
    positions.sort_unstable();
    positions.dedup();
    assert!(
        positions.len() > 1,
        "control position must vary: {positions:?}"
    );

    assert_eq!(
        hex(&sha256(render_permutations(&perms).as_bytes())),
        FROZEN_PERMUTATIONS_SHA256,
        "frozen permutation digest (bump G18_CORPUS_VERSION and re-freeze on change)"
    );
}

#[test]
fn seeded_permutations_on_shared_target_are_order_independent() {
    let names = shared::row_names();
    let mut transcript = String::new();
    let mut per_row: Vec<Option<String>> = vec![None; names.len()];
    for (seed, p) in SEEDS.iter().zip(all_permutations()) {
        let mut target = shared::initial();
        let mut control_seen = false;
        for (pos, &i) in p.iter().enumerate() {
            let name = names[i];
            let ctx = format!("seed {seed:#x} pos {pos}");
            let out = step(&mut target, name, &ctx);
            if !shared::is_rejection(name) {
                control_seen = true;
            } else if !control_seen {
                // Before the control, rejections keep the initial target.
                assert_eq!(
                    shared::fingerprint_of(&target),
                    shared::fingerprint_of(&shared::initial()),
                    "{ctx}: no drift from initial target before control"
                );
            }
            match &per_row[i] {
                None => per_row[i] = Some(out.clone()),
                Some(prev) => assert_eq!(prev, &out, "{ctx}: {name} outcome is order dependent"),
            }
            transcript.push_str(&format!("{seed:#018x}|{pos}|{name}|{out}\n"));
        }
        assert!(control_seen);
        // Final state is the accepted snapshot regardless of order.
        assert_eq!(
            shared::fingerprint_of(&target),
            shared::fingerprint_of(&shared::accepted_snapshot()),
            "seed {seed:#x}: final state must be exactly Home3"
        );
    }
    assert_eq!(
        hex(&sha256(transcript.as_bytes())),
        FROZEN_TRANSCRIPT_SHA256,
        "frozen permuted-replay transcript digest"
    );
}

#[test]
fn repeated_restore_reject_sequences_do_not_drift() {
    let names = shared::row_names();
    let control = *names
        .iter()
        .find(|n| !shared::is_rejection(n))
        .expect("positive control");

    // 1. Every mutant replayed REPEATS times on a pristine shared target.
    let mut target = shared::initial();
    let pristine = shared::fingerprint_of(&target);
    for name in names.iter().filter(|n| shared::is_rejection(n)) {
        for k in 0..REPEATS {
            step(&mut target, name, &format!("pristine repeat {k}"));
        }
    }
    assert_eq!(
        shared::fingerprint_of(&target),
        pristine,
        "no cumulative drift"
    );

    // 2. Same after the control has been accepted (accepted state is
    //    preserved by every later rejection).
    step(&mut target, control, "accept before repeats");
    let accepted = shared::fingerprint_of(&target);
    for name in names.iter().filter(|n| shared::is_rejection(n)) {
        for k in 0..REPEATS {
            step(&mut target, name, &format!("accepted repeat {k}"));
        }
    }
    assert_eq!(
        shared::fingerprint_of(&target),
        accepted,
        "accepted state preserved"
    );

    // 3. Interleaved control / mutant / control: the control is accepted
    //    again after every rejection and the state is identical each time.
    for name in names.iter().filter(|n| shared::is_rejection(n)) {
        for k in 0..REPEATS {
            step(&mut target, name, &format!("interleave reject {k}"));
            step(&mut target, control, &format!("interleave accept {k}"));
            assert_eq!(
                shared::fingerprint_of(&target),
                accepted,
                "{name} interleave {k}"
            );
        }
    }

    // 4. Repeated positive control on a fresh target is idempotent.
    let mut fresh = shared::initial();
    for k in 0..REPEATS {
        step(&mut fresh, control, &format!("control repeat {k}"));
        assert_eq!(
            shared::fingerprint_of(&fresh),
            accepted,
            "control repeat {k}"
        );
    }
}

#[test]
fn reversed_and_forward_orders_agree_row_by_row() {
    let names = shared::row_names();
    let mut fwd_target = shared::initial();
    let fwd: Vec<String> = names
        .iter()
        .map(|n| step(&mut fwd_target, n, "forward"))
        .collect();
    let mut rev_target = shared::initial();
    let mut rev: Vec<String> = names
        .iter()
        .rev()
        .map(|n| step(&mut rev_target, n, "reverse"))
        .collect();
    rev.reverse();
    assert_eq!(fwd, rev, "row outcomes must not depend on direction");
    assert_eq!(
        shared::fingerprint_of(&fwd_target),
        shared::fingerprint_of(&rev_target),
        "forward and reverse replays converge to the same state"
    );
}
