# LONGCTX-B1 Harness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the substrate for prerequisite 5 in `docs/SML-LONGCTX-B1-PREREGISTRATION.md` §12 (“B1 harness (generator + scorer + accounting) not written; it must be merged and unit-tested on hand-written fixtures before any campaign run”). That means a deterministic task 1–3 generator, exact-match/F1/FPR scorer stubs, and a §8 per-cell accounting record. All of it lives in `crates/sml-store/src/longctx_b1.rs` and is tested only on hand-written fixtures in `crates/sml-store/tests/longctx_b1.rs`.

**Architecture:** One dependency-free module in `sml-store` with five layers. (1) Frozen constants: `sml-byte-v0`, ladder, seeds `0xB1000001`..`0xB1000005`, depths, counts. (2) `B1Rng` (SplitMix64) plus split-namespaced hex keys. (3) The `B1FillerSource` hook with `FixtureFiller`, the filler-pin gate and the needle-in-filler abort inside an exact-span splice. (4) Generators `generate_task1/2/3`. (5) Scorer tallies (`H1Tally`, `H2Tally`/`Confusion`, `H3Tally`), `CellRecord` with frozen caps, and the `B1Arm` hook with `ExactScanSmokeArm`. Later PRs attach the pinned filler, B-lex, B-attn, the CONTEXT-3 arm adapter and RemoteOps dispatch through `B1FillerSource`, `B1Arm` and `CellRecord`, without changing this module's contracts.

**Tech Stack:** Rust / sml-store (or justify separate crate). This plan uses **`sml-store`**: it already owns `raw_context::sha256_raw_bytes` (needed for the filler pin) and the CONTEXT-1/3 substrate the primary arm adapter will call. A separate crate would duplicate that SHA-256 path for no isolation benefit while the harness has no new dependencies. Rust 1.89, edition 2021, workspace lint `unsafe_code = "forbid"`. No new crates, no `Cargo.toml`/`Cargo.lock` change. No NVIDIA/CUDA stack.

## Global Constraints
- No campaign runs; handwritten fixtures only; no invented metrics
- Do not claim H1–H4 results
- Merge only with mandatory CI green on exact head; no --no-verify
- Primary arm CONTEXT-3 already on main; this plan builds harness substrate only
- HYB Claude = competitor baseline only (not Grok architecture)
- Tokenizer sml-byte-v0: 1 byte = 1 token (frozen in prereg)
- Filler SHA / B-attn model choice / B-lex CCOS vs native are OUT OF SCOPE placeholders as unchecked follow-ups — harness must expose hooks so they plug in later
- Dual-track: CCOS Voie A ≠ 1M attention

Additional execution constraints (AGENTS.md):

- Run `bash scripts/install-hooks.sh` once per clone. The `.githooks/pre-push` hook runs `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings` and `cargo test --locked --workspace`. Never push with `--no-verify`.
- Required CI must be green on the exact PR head SHA before merge. Read the ecosystem roadmap before any PR/merge decision. Local worktree merges do not count as GitHub merges.
- Spans in reports and docs are written “N byte-tokens”, never “N-token context”. A mapped store is not a context window.
- Prereg anchors: #765 (preregistration), CONTEXT-3 on `main` at `327a4e57` (#768).

## Explicit non-goals (next PRs)

- No RemoteOps/Thor/Dell dispatch, no host probes (RSS/VRAM/latency fields stay `None` in this PR).
- No filler corpus pin: `FixtureFiller` only. The pinned source implements `B1FillerSource` in the fixture-freeze PR.
- No B-attn model, weights or runtime selection, and no weights download.
- No BM25 / B-lex implementation, and no choice yet between the CCOS #217 harness and a native one. The next PR implements `B1Arm` for B-lex.
- No CONTEXT-3 `ContextRetriever` adapter as a `B1Arm` (it needs segmenting the haystack via CONTEXT-1c; separate PR).
- No DEV or TEST campaign, no result table, no edits to prereg §8's result table.

## File Map

| File | Action | Responsibility |
|---|---|---|
| `crates/sml-store/src/longctx_b1.rs` | Create | Constants (`SML_BYTE_V0`, `SPAN_LADDER`, `B1_SEEDS`, depths, counts), `B1Split`, `B1Error`, `B1Rng`, key helpers, `B1FillerSource`/`FixtureFiller`/`verify_filler_pin`, `ensure_absent_from_filler`, `splice_haystack`, `generate_task1/2/3`, scorer tallies, caps, `CellRecord`, `B1ArmId`, `B1Arm`, `ExactScanSmokeArm`. |
| `crates/sml-store/src/lib.rs` | Modify (1 line) | Add `pub mod longctx_b1;` right after `pub mod delta_kv_context;`. |
| `crates/sml-store/tests/longctx_b1.rs` | Create | Hand-written fixture filler + 13 tests. |
| `docs/SML-LONGCTX-B1-PREREGISTRATION.md` | Not modified | Frozen. Do not edit thresholds, seeds or tables. |
| `docs/ROADMAP.md` | Modify (after merge only) | Check the “B1 harness” prereq sub-bullet in a follow-up docs PR once CI is green on `main`. |

Not touched: `Cargo.toml`, `Cargo.lock`, `context_retrieve.rs`, `raw_context.rs`, `context_ingest.rs`, `context_semantic_ingest.rs`, MOR, Core.

## Prereg → test map

| Prereg clause | Test | Task |
|---|---|---|
| §2 `sml-byte-v0` | `b1_tokenizer_is_one_byte_one_token` | 1 |
| §3 ladder, §9 seeds/splits | `b1_seeds_splits_and_ladder_are_frozen` | 1 |
| §9 disjoint DEV/TEST key namespaces | `b1_rng_is_deterministic_and_namespaces_are_disjoint` | 2 |
| §5 Hamming-1/2 near-miss | `b1_near_miss_has_exact_hamming_distance_and_keeps_namespace` | 2 |
| §9 needle never in filler, abort on hit | `b1_abort_when_needle_appears_in_filler` | 3 |
| §3 exact span in byte-tokens | `b1_splice_hits_exact_span_and_reports_offsets` | 3 |
| §5 filler pinned before run | `b1_fixture_filler_is_never_campaign_pinned` | 3 |
| §5 task 1 | `b1_task1_generator_is_deterministic_and_well_formed` | 4 |
| §5 task 2 | `b1_task2_generator_has_half_absent_membership_keys` | 5 |
| §5 task 3 | `b1_task3_hops_are_at_least_half_span_apart` | 6 |
| §4 H1/H2/H3 scoring | `b1_scorers_on_handwritten_answers` | 7 |
| §8 accounting, caps, fail-closed | `b1_cell_record_invariants_and_campaign_gate` | 8 |
| §6 raw-scan = smoke only | `b1_exact_scan_smoke_arm_answers_generated_fixtures` | 8 |

H4 (sublinear work) has no scorer in this plan. It needs `segments_decoded` across admitted rungs from a real arm, and `CellRecord` already carries that field. The exponent fit lands with the campaign-runner PR, and `H4_EXPONENT_MAX` is frozen here so the threshold cannot drift.

All commands run from the repository root of a clean worktree branched from `origin/main` (e.g. `feat/longctx-b1-harness`). Tasks 1–2 may print `unused import` **warnings** (`sha256_raw_bytes`, `ContextContentHash` are first used in Task 3). Those are gone by Task 3, and Task 9 enforces `clippy -D warnings`.

---
## Task 1: Tokenizer constant, frozen seeds/splits/ladder, error type

Freezes prereg §2 (`sml-byte-v0`), §3 ladder, §5 counts and §9 seeds/splits as code constants. Unknown seeds and off-ladder spans fail closed.

**Files:**
- Create: `crates/sml-store/tests/longctx_b1.rs`
- Create: `crates/sml-store/src/longctx_b1.rs`
- Modify: `crates/sml-store/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

Create `crates/sml-store/tests/longctx_b1.rs` with exactly:

```rust
//! LONGCTX-B1 harness tests. Hand-written fixtures only; no campaign, no metric.

use sml_store::longctx_b1::*;

/// Hand-written filler: lowercase words and spaces, so no 8-hex run can occur.
const FILLER: &[u8] = b"the slow river runs past quiet hills and old stone walls ";

fn filler() -> FixtureFiller {
    FixtureFiller { text: FILLER }
}

#[test]
fn b1_tokenizer_is_one_byte_one_token() {
    assert_eq!(SML_BYTE_V0, "sml-byte-v0");
    assert_eq!(sml_byte_v0_token_count(b""), 0);
    assert_eq!(sml_byte_v0_token_count("é".as_bytes()), 2);
    assert_eq!(sml_byte_v0_token_count(b"KEY=0"), 5);
}

#[test]
fn b1_seeds_splits_and_ladder_are_frozen() {
    assert_eq!(
        B1_SEEDS,
        [
            0xB100_0001,
            0xB100_0002,
            0xB100_0003,
            0xB100_0004,
            0xB100_0005
        ]
    );
    assert_eq!(B1Split::for_seed(0xB100_0001), Ok(B1Split::Dev));
    assert_eq!(B1Split::for_seed(0xB100_0002), Ok(B1Split::Dev));
    for s in [0xB100_0003, 0xB100_0004, 0xB100_0005] {
        assert_eq!(B1Split::for_seed(s), Ok(B1Split::Test));
    }
    assert_eq!(B1Split::for_seed(42), Err(B1Error::UnknownSeed(42)));
    assert_eq!(validate_span(1_048_576), Ok(1_048_576));
    assert_eq!(
        validate_span(1_000_000),
        Err(B1Error::SpanNotOnLadder(1_000_000))
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test --locked -p sml-store --test longctx_b1
```

Expected: FAIL to compile — `unresolved import sml_store::longctx_b1`.

- [ ] **Step 3: Write minimal implementation**

In `crates/sml-store/src/lib.rs`, after `pub mod delta_kv_context;` add:

```rust
pub mod longctx_b1;
```

Create `crates/sml-store/src/longctx_b1.rs` with exactly:

```rust
//! LONGCTX-B1 harness substrate (docs/SML-LONGCTX-B1-PREREGISTRATION.md).
//!
//! Generator, scorer and per-cell accounting only. This module runs no
//! campaign, reports no metric and supports no H1–H4 claim. Filler corpus pin,
//! B-lex, B-attn and RemoteOps dispatch plug in through the hooks below.

use std::fmt;

use sml_core::context_ledger::ContextContentHash;

use crate::raw_context::sha256_raw_bytes;

/// Frozen B1 tokenizer identifier (prereg §2).
pub const SML_BYTE_V0: &str = "sml-byte-v0";

/// `sml-byte-v0`: one token = one UTF-8 byte of the raw payload.
#[must_use]
pub fn sml_byte_v0_token_count(raw: &[u8]) -> u64 {
    raw.len() as u64
}

/// Frozen span ladder in byte-tokens (prereg §3).
pub const SPAN_LADDER: [u64; 5] = [4096, 16384, 65536, 262144, 1048576];

/// Frozen generation seeds (prereg §9).
pub const B1_SEEDS: [u64; 5] = [
    0xB100_0001,
    0xB100_0002,
    0xB100_0003,
    0xB100_0004,
    0xB100_0005,
];

/// Frozen task-1 depths in permille of the filler (prereg §5).
pub const TASK1_DEPTHS_PERMILLE: [u16; 7] = [0, 100, 250, 500, 750, 900, 1000];
/// Frozen task-1 near-miss distractor count (prereg §5).
pub const TASK1_DISTRACTORS: usize = 16;
/// Frozen task-2 needle counts (prereg §5).
pub const TASK2_K_VALUES: [usize; 3] = [2, 4, 8];
/// Frozen per-cell instance count (prereg §5).
pub const INSTANCES_PER_CELL: usize = 20;

/// DEV/TEST split (prereg §9). DEV = seeds 1–2, TEST = seeds 3–5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum B1Split {
    Dev,
    Test,
}

impl B1Split {
    /// Fails closed on any seed outside the frozen list.
    pub fn for_seed(seed: u64) -> Result<Self, B1Error> {
        match B1_SEEDS.iter().position(|s| *s == seed) {
            Some(0 | 1) => Ok(Self::Dev),
            Some(_) => Ok(Self::Test),
            None => Err(B1Error::UnknownSeed(seed)),
        }
    }

    /// Disjoint key namespaces: first hex nibble 0–7 = DEV, 8–f = TEST.
    #[must_use]
    pub const fn first_nibble_range(self) -> (u8, u8) {
        match self {
            Self::Dev => (0x0, 0x7),
            Self::Test => (0x8, 0xf),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum B1Error {
    UnknownSeed(u64),
    SpanNotOnLadder(u64),
    SpanTooSmall { span: u64, inserted: u64 },
    NeedleInFiller { offset: usize },
    FillerNotPinned,
    FillerShaMismatch,
    InvalidDepth(u16),
    InvalidK(usize),
    AccountingInvariant(&'static str),
    Hash,
}

impl fmt::Display for B1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for B1Error {}

/// Fails closed on any span outside the frozen ladder.
pub fn validate_span(span: u64) -> Result<u64, B1Error> {
    if SPAN_LADDER.contains(&span) {
        Ok(span)
    } else {
        Err(B1Error::SpanNotOnLadder(span))
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --locked -p sml-store --test longctx_b1
```

Expected: PASS — `test result: ok. 2 passed; 0 failed`.

- [ ] **Step 5: Commit**

```bash
git add crates/sml-store/src/longctx_b1.rs crates/sml-store/tests/longctx_b1.rs crates/sml-store/src/lib.rs
git commit -m "feat(longctx-b1): tokenizer constant, frozen seeds, splits and ladder"
```

---

## Task 2: Deterministic SplitMix64, split key namespaces, near-miss keys

All randomness comes from one SplitMix64 stream seeded per (seed, task, cell, instance). DEV keys start with nibble 0–7 and TEST keys with 8–f, so the §9 DEV/TEST key namespaces are disjoint by construction. Near-miss keys keep nibble 0, so they stay in the same namespace.

**Files:**
- Modify: `crates/sml-store/tests/longctx_b1.rs`
- Modify: `crates/sml-store/src/longctx_b1.rs`

- [ ] **Step 1: Write the failing tests**

Append to `crates/sml-store/tests/longctx_b1.rs`:

```rust
#[test]
fn b1_rng_is_deterministic_and_namespaces_are_disjoint() {
    let mut a = B1Rng::new(0xB100_0001);
    let mut b = B1Rng::new(0xB100_0001);
    assert_eq!((a.next_u64(), a.next_u64()), (b.next_u64(), b.next_u64()));
    let mut rng = B1Rng::new(7);
    for _ in 0..256 {
        let dev = hex_in_namespace(&mut rng, 16, B1Split::Dev);
        let test = hex_in_namespace(&mut rng, 16, B1Split::Test);
        assert!(b"01234567".contains(&dev[0]), "{dev:?}");
        assert!(b"89abcdef".contains(&test[0]), "{test:?}");
        assert!(dev.iter().chain(&test).all(|c| c.is_ascii_hexdigit()));
    }
}

#[test]
fn b1_near_miss_has_exact_hamming_distance_and_keeps_namespace() {
    let key = b"3a5f00112233c0de".to_vec();
    let mut rng = B1Rng::new(9);
    for distance in [1usize, 2] {
        for _ in 0..64 {
            let near = near_miss_key(&mut rng, &key, distance);
            let diff = key.iter().zip(&near).filter(|(x, y)| x != y).count();
            assert_eq!(diff, distance);
            assert_eq!(near[0], key[0]);
        }
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test --locked -p sml-store --test longctx_b1
```

Expected: FAIL to compile — the new items are not found in `sml_store::longctx_b1`.

- [ ] **Step 3: Write minimal implementation**

Append to `crates/sml-store/src/longctx_b1.rs`:

```rust
/// Deterministic SplitMix64 stream; the only randomness source of the generator.
#[derive(Debug, Clone)]
pub struct B1Rng {
    state: u64,
}

impl B1Rng {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `0..bound` (bound > 0) by rejection, no modulo bias.
    pub fn below(&mut self, bound: u64) -> u64 {
        assert!(bound > 0, "below(0)");
        let zone = u64::MAX - (u64::MAX % bound);
        loop {
            let v = self.next_u64();
            if v < zone {
                return v % bound;
            }
        }
    }
}

const HEX: &[u8; 16] = b"0123456789abcdef";

/// `n` lowercase hex digits; the first nibble is forced into the split namespace.
pub fn hex_in_namespace(rng: &mut B1Rng, n: usize, split: B1Split) -> Vec<u8> {
    let (lo, hi) = split.first_nibble_range();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let nibble = if i == 0 {
            lo + rng.below(u64::from(hi - lo) + 1) as u8
        } else {
            rng.below(16) as u8
        };
        out.push(HEX[usize::from(nibble)]);
    }
    out
}

/// Near-miss key at hex Hamming distance `distance` (1 or 2); nibble 0 is kept
/// so the near-miss stays in the same namespace.
pub fn near_miss_key(rng: &mut B1Rng, key: &[u8], distance: usize) -> Vec<u8> {
    assert!((1..=2).contains(&distance) && key.len() >= 3);
    let mut out = key.to_vec();
    let first = 1 + rng.below(key.len() as u64 - 1) as usize;
    let mut positions = vec![first];
    while positions.len() < distance {
        let p = 1 + rng.below(key.len() as u64 - 1) as usize;
        if !positions.contains(&p) {
            positions.push(p);
        }
    }
    for p in positions {
        let old = out[p];
        loop {
            let c = HEX[rng.below(16) as usize];
            if c != old {
                out[p] = c;
                break;
            }
        }
    }
    out
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --locked -p sml-store --test longctx_b1
```

Expected: PASS — `test result: ok. 4 passed; 0 failed`.

- [ ] **Step 5: Commit**

```bash
git add crates/sml-store/src/longctx_b1.rs crates/sml-store/tests/longctx_b1.rs
git commit -m "feat(longctx-b1): deterministic rng and split key namespaces"
```

---

## Task 3: Filler hook, filler-pin gate, needle-in-filler abort, exact-span splice

`B1FillerSource` is the hook where the pinned open-license corpus plugs in later. `FixtureFiller` always returns `pinned_sha256() == None`, so it can never pass the campaign gate. `splice_haystack` runs the §9 exact scan and aborts with `NeedleInFiller` on any hit, before anything is spliced.

**Files:**
- Modify: `crates/sml-store/tests/longctx_b1.rs`
- Modify: `crates/sml-store/src/longctx_b1.rs`

- [ ] **Step 1: Write the failing tests**

Append to `crates/sml-store/tests/longctx_b1.rs`:

```rust
#[test]
fn b1_abort_when_needle_appears_in_filler() {
    let poisoned = b"prefix KEY=3a5f00112233c0de suffix";
    assert_eq!(
        ensure_absent_from_filler(poisoned, &[b"3a5f00112233c0de"]),
        Err(B1Error::NeedleInFiller { offset: 11 })
    );
    assert_eq!(
        ensure_absent_from_filler(FILLER, &[b"3a5f00112233c0de"]),
        Ok(())
    );
}

#[test]
fn b1_splice_hits_exact_span_and_reports_offsets() {
    let line = b"KEY=0000000000000000 VALUE=11111111111111111111111111111111\n".to_vec();
    let (hay, ins) = splice_haystack(
        &filler(),
        0xB100_0001,
        B1Split::Dev,
        1000,
        vec![(500, line.clone())],
    )
    .unwrap();
    assert_eq!(hay.len(), 1000);
    assert_eq!(ins.len(), 1);
    let off = ins[0].offset as usize;
    assert_eq!(off, 500 * (1000 - line.len()) / 1000);
    assert_eq!(&hay[off..off + line.len()], line.as_slice());
    assert_eq!(
        splice_haystack(&filler(), 0xB100_0001, B1Split::Dev, 10, vec![(0, line)]),
        Err(B1Error::SpanTooSmall {
            span: 10,
            inserted: 60
        })
    );
}

#[test]
fn b1_fixture_filler_is_never_campaign_pinned() {
    assert_eq!(filler().pinned_sha256(), None);
    assert_eq!(
        verify_filler_pin(&filler(), FILLER),
        Err(B1Error::FillerNotPinned)
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test --locked -p sml-store --test longctx_b1
```

Expected: FAIL to compile — the new items are not found in `sml_store::longctx_b1`.

- [ ] **Step 3: Write minimal implementation**

Append to `crates/sml-store/src/longctx_b1.rs`:

```rust
/// Filler hook (prereg §5). The open-license corpus pin is a later PR; until a
/// source returns a pinned SHA-256, generated instances are fixture-only.
pub trait B1FillerSource {
    /// Stable identifier recorded in every cell.
    fn id(&self) -> &str;
    /// SHA-256 pinned by the fixture-freeze note, `None` while unpinned.
    fn pinned_sha256(&self) -> Option<ContextContentHash>;
    /// Exactly `len` deterministic filler bytes for `(seed, split)`.
    fn fill(&self, seed: u64, split: B1Split, len: usize) -> Vec<u8>;
}

/// Hand-written fixture filler for unit tests only. Never campaign-admissible.
#[derive(Debug, Clone)]
pub struct FixtureFiller {
    pub text: &'static [u8],
}

impl B1FillerSource for FixtureFiller {
    fn id(&self) -> &str {
        "fixture-filler-unpinned"
    }

    fn pinned_sha256(&self) -> Option<ContextContentHash> {
        None
    }

    fn fill(&self, _seed: u64, _split: B1Split, len: usize) -> Vec<u8> {
        self.text.iter().copied().cycle().take(len).collect()
    }
}

/// Campaign gate: the filler must be pinned and its corpus bytes must hash to the pin.
pub fn verify_filler_pin(
    source: &dyn B1FillerSource,
    corpus_bytes: &[u8],
) -> Result<ContextContentHash, B1Error> {
    let pin = source.pinned_sha256().ok_or(B1Error::FillerNotPinned)?;
    let actual = sha256_raw_bytes(corpus_bytes).map_err(|_| B1Error::Hash)?;
    if actual == pin {
        Ok(pin)
    } else {
        Err(B1Error::FillerShaMismatch)
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Prereg §9: needle strings never appear in filler; exact scan, abort on hit.
pub fn ensure_absent_from_filler(filler: &[u8], needles: &[&[u8]]) -> Result<(), B1Error> {
    for needle in needles {
        if let Some(offset) = find(filler, needle) {
            return Err(B1Error::NeedleInFiller { offset });
        }
    }
    Ok(())
}

/// One inserted line and its byte offset in the final haystack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Insertion {
    pub depth_permille: u16,
    pub line: Vec<u8>,
    pub offset: u64,
}

/// Splices `lines` into filler so the haystack is exactly `span` byte-tokens.
/// Depth `d` maps to filler position `d * filler_len / 1000`; ties keep input order.
pub fn splice_haystack(
    source: &dyn B1FillerSource,
    seed: u64,
    split: B1Split,
    span: u64,
    lines: Vec<(u16, Vec<u8>)>,
) -> Result<(Vec<u8>, Vec<Insertion>), B1Error> {
    let inserted: u64 = lines.iter().map(|(_, l)| l.len() as u64).sum();
    if inserted >= span {
        return Err(B1Error::SpanTooSmall { span, inserted });
    }
    let filler_len = (span - inserted) as usize;
    let filler = source.fill(seed, split, filler_len);
    let mut planned: Vec<(usize, usize, u16, Vec<u8>)> = Vec::with_capacity(lines.len());
    for (order, (depth, line)) in lines.into_iter().enumerate() {
        if depth > 1000 {
            return Err(B1Error::InvalidDepth(depth));
        }
        let at = usize::from(depth) * filler_len / 1000;
        planned.push((at, order, depth, line));
    }
    let needle_refs: Vec<&[u8]> = planned.iter().map(|p| p.3.as_slice()).collect();
    ensure_absent_from_filler(&filler, &needle_refs)?;
    planned.sort_by_key(|p| (p.0, p.1));
    let mut out = Vec::with_capacity(span as usize);
    let mut cursor = 0usize;
    let mut insertions = Vec::with_capacity(planned.len());
    for (at, _, depth, line) in planned {
        out.extend_from_slice(&filler[cursor..at]);
        cursor = at;
        insertions.push(Insertion {
            depth_permille: depth,
            offset: out.len() as u64,
            line: line.clone(),
        });
        out.extend_from_slice(&line);
    }
    out.extend_from_slice(&filler[cursor..]);
    debug_assert_eq!(out.len() as u64, span);
    Ok((out, insertions))
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --locked -p sml-store --test longctx_b1
```

Expected: PASS — `test result: ok. 7 passed; 0 failed`.

- [ ] **Step 5: Commit**

```bash
git add crates/sml-store/src/longctx_b1.rs crates/sml-store/tests/longctx_b1.rs
git commit -m "feat(longctx-b1): filler hook, pin gate and needle-in-filler abort"
```

---

## Task 4: Task 1 generator: exact needle-in-haystack

Prereg §5 task 1: one `KEY=<16 hex> VALUE=<32 hex>` line at depth d ∈ {0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0}, plus 16 near-miss distractors (Hamming-1/2 hex) with different values. The answer is the exact value bytes.

**Files:**
- Modify: `crates/sml-store/tests/longctx_b1.rs`
- Modify: `crates/sml-store/src/longctx_b1.rs`

- [ ] **Step 1: Write the failing tests**

Append to `crates/sml-store/tests/longctx_b1.rs`:

```rust
#[test]
fn b1_task1_generator_is_deterministic_and_well_formed() {
    let f = filler();
    let a = generate_task1(&f, 0xB100_0003, 4096, 250, 0).unwrap();
    let b = generate_task1(&f, 0xB100_0003, 4096, 250, 0).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.split, B1Split::Test);
    assert_eq!(a.haystack.len(), 4096);
    assert_eq!(a.insertions.len(), 1 + TASK1_DISTRACTORS);
    assert_eq!((a.query_key.len(), a.answer.len()), (16, 32));
    let needle = needle_line(&a.query_key, &a.answer);
    let hits = a
        .haystack
        .windows(needle.len())
        .filter(|w| *w == needle.as_slice())
        .count();
    assert_eq!(hits, 1);
    let key_line = [b"KEY=".as_slice(), &a.query_key, b" "].concat();
    let key_hits = a
        .haystack
        .windows(key_line.len())
        .filter(|w| *w == key_line.as_slice())
        .count();
    assert_eq!(
        key_hits, 1,
        "near-miss distractors must not equal the query key"
    );
    assert_ne!(a, generate_task1(&f, 0xB100_0003, 4096, 250, 1).unwrap());
    assert_eq!(
        generate_task1(&f, 0xB100_0003, 4096, 300, 0).unwrap_err(),
        B1Error::InvalidDepth(300)
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test --locked -p sml-store --test longctx_b1
```

Expected: FAIL to compile — the new items are not found in `sml_store::longctx_b1`.

- [ ] **Step 3: Write minimal implementation**

Append to `crates/sml-store/src/longctx_b1.rs`:

```rust
/// `KEY=<16 hex> VALUE=<32 hex>\n` (prereg §5 task 1).
#[must_use]
pub fn needle_line(key: &[u8], value: &[u8]) -> Vec<u8> {
    let mut line = Vec::with_capacity(4 + key.len() + 7 + value.len() + 1);
    line.extend_from_slice(b"KEY=");
    line.extend_from_slice(key);
    line.extend_from_slice(b" VALUE=");
    line.extend_from_slice(value);
    line.push(b'\n');
    line
}

/// Exact-needle instance (task 1). `answer` is the exact value bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task1Instance {
    pub seed: u64,
    pub split: B1Split,
    pub span: u64,
    pub depth_permille: u16,
    pub haystack: Vec<u8>,
    pub query_key: Vec<u8>,
    pub answer: Vec<u8>,
    pub insertions: Vec<Insertion>,
}

/// Generates one task-1 instance. Distractors get uniform random depths.
/// `span` is not ladder-checked here so fixtures can use small spans; the
/// campaign entry point must call [`validate_span`].
pub fn generate_task1(
    source: &dyn B1FillerSource,
    seed: u64,
    span: u64,
    depth_permille: u16,
    instance: u64,
) -> Result<Task1Instance, B1Error> {
    let split = B1Split::for_seed(seed)?;
    if !TASK1_DEPTHS_PERMILLE.contains(&depth_permille) {
        return Err(B1Error::InvalidDepth(depth_permille));
    }
    let mut rng = B1Rng::new(seed ^ (1 << 56) ^ (u64::from(depth_permille) << 32) ^ instance);
    let key = hex_in_namespace(&mut rng, 16, split);
    let value = hex_in_namespace(&mut rng, 32, split);
    let mut lines = vec![(depth_permille, needle_line(&key, &value))];
    for i in 0..TASK1_DISTRACTORS {
        let mut near = near_miss_key(&mut rng, &key, 1 + i % 2);
        while near == key {
            near = near_miss_key(&mut rng, &key, 1 + i % 2);
        }
        let other = hex_in_namespace(&mut rng, 32, split);
        let depth = rng.below(1001) as u16;
        lines.push((depth, needle_line(&near, &other)));
    }
    let (haystack, insertions) = splice_haystack(source, seed, split, span, lines)?;
    Ok(Task1Instance {
        seed,
        split,
        span,
        depth_permille,
        haystack,
        query_key: key,
        answer: value,
        insertions,
    })
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --locked -p sml-store --test longctx_b1
```

Expected: PASS — `test result: ok. 8 passed; 0 failed`.

- [ ] **Step 5: Commit**

```bash
git add crates/sml-store/src/longctx_b1.rs crates/sml-store/tests/longctx_b1.rs
git commit -m "feat(longctx-b1): task 1 exact-needle generator"
```

---

## Task 5: Task 2 generator: multi-needle and set membership

Prereg §5 task 2: k ∈ {2, 4, 8} needles at stratified depths, k value queries and k membership queries with exactly half of the membership keys absent. Absent keys are checked against the full haystack, not only the filler.

**Files:**
- Modify: `crates/sml-store/tests/longctx_b1.rs`
- Modify: `crates/sml-store/src/longctx_b1.rs`

- [ ] **Step 1: Write the failing tests**

Append to `crates/sml-store/tests/longctx_b1.rs`:

```rust
#[test]
fn b1_task2_generator_has_half_absent_membership_keys() {
    let f = filler();
    for k in TASK2_K_VALUES {
        let inst = generate_task2(&f, 0xB100_0001, 4096, k, 0).unwrap();
        assert_eq!(inst, generate_task2(&f, 0xB100_0001, 4096, k, 0).unwrap());
        assert_eq!(inst.haystack.len(), 4096);
        assert_eq!(inst.insertions.len(), k);
        let values = inst
            .queries
            .iter()
            .filter(|q| matches!(q, Task2Query::ValueOf { .. }))
            .count();
        let absent = inst
            .queries
            .iter()
            .filter(|q| matches!(q, Task2Query::IsPresent { present: false, .. }))
            .count();
        let present = inst
            .queries
            .iter()
            .filter(|q| matches!(q, Task2Query::IsPresent { present: true, .. }))
            .count();
        assert_eq!((values, present + absent), (k, k));
        assert_eq!(absent, k / 2);
    }
    assert_eq!(stratified_depth(0, 2), 250);
    assert_eq!(stratified_depth(1, 2), 750);
    assert_eq!(
        generate_task2(&f, 0xB100_0001, 4096, 3, 0).unwrap_err(),
        B1Error::InvalidK(3)
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test --locked -p sml-store --test longctx_b1
```

Expected: FAIL to compile — the new items are not found in `sml_store::longctx_b1`.

- [ ] **Step 3: Write minimal implementation**

Append to `crates/sml-store/src/longctx_b1.rs`:

```rust
/// Task-2 query: value lookup or membership (prereg §5 task 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Task2Query {
    ValueOf { key: Vec<u8>, answer: Vec<u8> },
    IsPresent { key: Vec<u8>, present: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task2Instance {
    pub seed: u64,
    pub split: B1Split,
    pub span: u64,
    pub k: usize,
    pub haystack: Vec<u8>,
    pub queries: Vec<Task2Query>,
    pub insertions: Vec<Insertion>,
}

/// Stratified depth of needle `i` among `k`: centre of the i-th of k strata.
#[must_use]
pub fn stratified_depth(i: usize, k: usize) -> u16 {
    ((2 * i + 1) * 1000 / (2 * k)) as u16
}

/// Generates one task-2 instance: k value queries plus k membership queries,
/// exactly half of the membership keys absent (prereg: 50% absent).
pub fn generate_task2(
    source: &dyn B1FillerSource,
    seed: u64,
    span: u64,
    k: usize,
    instance: u64,
) -> Result<Task2Instance, B1Error> {
    let split = B1Split::for_seed(seed)?;
    if !TASK2_K_VALUES.contains(&k) {
        return Err(B1Error::InvalidK(k));
    }
    let mut rng = B1Rng::new(seed ^ (2 << 56) ^ ((k as u64) << 32) ^ instance);
    let mut keys: Vec<Vec<u8>> = Vec::with_capacity(k);
    let mut lines = Vec::with_capacity(k);
    let mut queries = Vec::with_capacity(2 * k);
    for i in 0..k {
        let mut key = hex_in_namespace(&mut rng, 16, split);
        while keys.contains(&key) {
            key = hex_in_namespace(&mut rng, 16, split);
        }
        let value = hex_in_namespace(&mut rng, 32, split);
        lines.push((stratified_depth(i, k), needle_line(&key, &value)));
        queries.push(Task2Query::ValueOf {
            key: key.clone(),
            answer: value,
        });
        keys.push(key);
    }
    let mut absent: Vec<Vec<u8>> = Vec::with_capacity(k / 2);
    while absent.len() < k / 2 {
        let key = hex_in_namespace(&mut rng, 16, split);
        if !keys.contains(&key) && !absent.contains(&key) {
            absent.push(key);
        }
    }
    for key in keys.iter().take(k - k / 2) {
        queries.push(Task2Query::IsPresent {
            key: key.clone(),
            present: true,
        });
    }
    let (haystack, insertions) = splice_haystack(source, seed, split, span, lines)?;
    let absent_refs: Vec<&[u8]> = absent.iter().map(Vec::as_slice).collect();
    ensure_absent_from_filler(&haystack, &absent_refs)?;
    for key in absent {
        queries.push(Task2Query::IsPresent {
            key,
            present: false,
        });
    }
    Ok(Task2Instance {
        seed,
        split,
        span,
        k,
        haystack,
        queries,
        insertions,
    })
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --locked -p sml-store --test longctx_b1
```

Expected: PASS — `test result: ok. 9 passed; 0 failed`.

- [ ] **Step 5: Commit**

```bash
git add crates/sml-store/src/longctx_b1.rs crates/sml-store/tests/longctx_b1.rs
git commit -m "feat(longctx-b1): task 2 multi-needle/membership generator"
```

---

## Task 6: Task 3 generator: 2-hop chains separated by ≥ L/2

Prereg §5 task 3: `LINK=A NEXT=B` at d1 and `KEY=B VALUE=C` at d2 with |d1−d2| ≥ 500‰ (≥ L/2 apart up to the inserted line bytes). One distractor chain shares the first 8 hex digits of A and B.

**Files:**
- Modify: `crates/sml-store/tests/longctx_b1.rs`
- Modify: `crates/sml-store/src/longctx_b1.rs`

- [ ] **Step 1: Write the failing tests**

Append to `crates/sml-store/tests/longctx_b1.rs`:

```rust
#[test]
fn b1_task3_hops_are_at_least_half_span_apart() {
    assert!(task3_depth_pairs()
        .iter()
        .all(|(a, b)| a.abs_diff(*b) >= 500));
    let f = filler();
    for i in 0..8 {
        let inst = generate_task3(&f, 0xB100_0002, 4096, i).unwrap();
        assert_eq!(inst.haystack.len(), 4096);
        assert!(inst.d1_permille.abs_diff(inst.d2_permille) >= 500);
        let first = inst
            .insertions
            .iter()
            .find(|x| x.line.starts_with(b"LINK=") && x.line[5..21] == inst.start_key[..])
            .unwrap();
        let second = inst
            .insertions
            .iter()
            .find(|x| x.line.ends_with(&[inst.answer.as_slice(), b"\n"].concat()))
            .unwrap();
        assert!(first.offset.abs_diff(second.offset) * 2 >= 4096 - 4 * 60);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test --locked -p sml-store --test longctx_b1
```

Expected: FAIL to compile — the new items are not found in `sml_store::longctx_b1`.

- [ ] **Step 3: Write minimal implementation**

Append to `crates/sml-store/src/longctx_b1.rs`:

```rust
/// `LINK=<16 hex> NEXT=<16 hex>\n`, the first hop of task 3.
#[must_use]
pub fn link_line(from: &[u8], to: &[u8]) -> Vec<u8> {
    let mut line = Vec::with_capacity(5 + from.len() + 6 + to.len() + 1);
    line.extend_from_slice(b"LINK=");
    line.extend_from_slice(from);
    line.extend_from_slice(b" NEXT=");
    line.extend_from_slice(to);
    line.push(b'\n');
    line
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task3Instance {
    pub seed: u64,
    pub split: B1Split,
    pub span: u64,
    pub d1_permille: u16,
    pub d2_permille: u16,
    pub haystack: Vec<u8>,
    pub start_key: Vec<u8>,
    pub answer: Vec<u8>,
    pub insertions: Vec<Insertion>,
}

/// Frozen depth pairs with |d1 − d2| ≥ 500 permille (prereg: hops ≥ L/2 apart).
#[must_use]
pub fn task3_depth_pairs() -> Vec<(u16, u16)> {
    let mut pairs = Vec::new();
    for d1 in TASK1_DEPTHS_PERMILLE {
        for d2 in TASK1_DEPTHS_PERMILLE {
            if d1.abs_diff(d2) >= 500 {
                pairs.push((d1, d2));
            }
        }
    }
    pairs
}

/// Generates one task-3 instance: `LINK=A NEXT=B` at d1, `KEY=B VALUE=C` at d2,
/// plus one distractor chain whose keys share the first 8 hex digits of A and B.
pub fn generate_task3(
    source: &dyn B1FillerSource,
    seed: u64,
    span: u64,
    instance: u64,
) -> Result<Task3Instance, B1Error> {
    let split = B1Split::for_seed(seed)?;
    let pairs = task3_depth_pairs();
    let mut rng = B1Rng::new(seed ^ (3 << 56) ^ instance);
    let (d1, d2) = pairs[rng.below(pairs.len() as u64) as usize];
    let a = hex_in_namespace(&mut rng, 16, split);
    let b = hex_in_namespace(&mut rng, 16, split);
    let c = hex_in_namespace(&mut rng, 32, split);
    let mut a2 = a[..8].to_vec();
    a2.extend(hex_in_namespace(&mut rng, 8, split));
    let mut b2 = b[..8].to_vec();
    b2.extend(hex_in_namespace(&mut rng, 8, split));
    if a2 == a || b2 == b {
        return Err(B1Error::AccountingInvariant(
            "distractor chain collided with target chain",
        ));
    }
    let c2 = hex_in_namespace(&mut rng, 32, split);
    let lines = vec![
        (d1, link_line(&a, &b)),
        (d2, needle_line(&b, &c)),
        (d2, link_line(&a2, &b2)),
        (d1, needle_line(&b2, &c2)),
    ];
    let (haystack, insertions) = splice_haystack(source, seed, split, span, lines)?;
    Ok(Task3Instance {
        seed,
        split,
        span,
        d1_permille: d1,
        d2_permille: d2,
        haystack,
        start_key: a,
        answer: c,
        insertions,
    })
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --locked -p sml-store --test longctx_b1
```

Expected: PASS — `test result: ok. 10 passed; 0 failed`.

- [ ] **Step 5: Commit**

```bash
git add crates/sml-store/src/longctx_b1.rs crates/sml-store/tests/longctx_b1.rs
git commit -m "feat(longctx-b1): task 3 two-hop generator"
```

---

## Task 7: Scorer stubs: exact match, H1 depth buckets, H2 F1/FPR, H3 accuracy

Prereg §4 thresholds stored as exact rationals. These are **stubs** that compute verdicts from counts. Tests feed hand-written answers only, and no number produced here is a B1 result.

**Files:**
- Modify: `crates/sml-store/tests/longctx_b1.rs`
- Modify: `crates/sml-store/src/longctx_b1.rs`

- [ ] **Step 1: Write the failing tests**

Append to `crates/sml-store/tests/longctx_b1.rs`:

```rust
#[test]
fn b1_scorers_on_handwritten_answers() {
    assert!(exact_match(Some(b"abc"), b"abc"));
    assert!(!exact_match(Some(b"abd"), b"abc"));
    assert!(!exact_match(None, b"abc"));

    let mut h1 = H1Tally::default();
    for _ in 0..19 {
        h1.record(0, true);
    }
    h1.record(0, false);
    for _ in 0..20 {
        h1.record(500, true);
    }
    assert_eq!(h1.passes(), Some(true)); // mean 0.975, min bucket 0.95
    let mut low = H1Tally::default();
    for _ in 0..16 {
        low.record(0, true);
    }
    for _ in 0..4 {
        low.record(0, false);
    }
    for _ in 0..20 {
        low.record(500, true);
    }
    assert_eq!(low.passes(), Some(false)); // bucket 0 = 0.80 < 0.85
    assert_eq!(H1Tally::default().passes(), None);

    let mut h2 = H2Tally::default();
    let v = Task2Query::ValueOf {
        key: b"k".to_vec(),
        answer: b"v".to_vec(),
    };
    let absent = Task2Query::IsPresent {
        key: b"z".to_vec(),
        present: false,
    };
    h2.record(2, &v, &Task2Answer::Value(Some(b"v".to_vec())));
    h2.record(2, &v, &Task2Answer::Value(None));
    h2.record(2, &absent, &Task2Answer::Present(true));
    h2.record(2, &absent, &Task2Answer::Present(false));
    assert_eq!(
        h2.per_k[&2],
        Confusion {
            tp: 1,
            fp: 0,
            tn: 0,
            fn_: 1
        }
    );
    assert_eq!(
        h2.membership_negatives,
        Confusion {
            tp: 0,
            fp: 1,
            tn: 1,
            fn_: 0
        }
    );

    let mut h3 = H3Tally::default();
    for _ in 0..4 {
        h3.record(true);
    }
    h3.record(false);
    assert_eq!(h3.passes(), Some(true)); // 4/5 = 0.80
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test --locked -p sml-store --test longctx_b1
```

Expected: FAIL to compile — the new items are not found in `sml_store::longctx_b1`.

- [ ] **Step 3: Write minimal implementation**

Append to `crates/sml-store/src/longctx_b1.rs`:

```rust
/// Frozen gate thresholds (prereg §4) as exact rationals (numerator, denominator).
pub const H1_MEAN_MIN: (u64, u64) = (95, 100);
pub const H1_BUCKET_MIN: (u64, u64) = (85, 100);
pub const H2_F1_MIN: (u64, u64) = (90, 100);
pub const H2_FPR_MAX: (u64, u64) = (2, 100);
pub const H3_ACC_MIN: (u64, u64) = (80, 100);
pub const H4_EXPONENT_MAX: (u64, u64) = (75, 100);

/// Source-exact scoring: byte-for-byte equality, no normalisation.
#[must_use]
pub fn exact_match(predicted: Option<&[u8]>, gold: &[u8]) -> bool {
    predicted == Some(gold)
}

/// Per-depth exact-match counts for H1.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct H1Tally {
    /// depth_permille -> (correct, total)
    pub buckets: std::collections::BTreeMap<u16, (u64, u64)>,
}

impl H1Tally {
    pub fn record(&mut self, depth_permille: u16, correct: bool) {
        let e = self.buckets.entry(depth_permille).or_insert((0, 0));
        e.0 += u64::from(correct);
        e.1 += 1;
    }

    /// Mean over depth buckets of bucket accuracy, as f64 for reports only.
    #[must_use]
    pub fn mean_over_depths(&self) -> Option<f64> {
        if self.buckets.is_empty() {
            return None;
        }
        let sum: f64 = self
            .buckets
            .values()
            .map(|(c, t)| *c as f64 / *t as f64)
            .sum();
        Some(sum / self.buckets.len() as f64)
    }

    /// H1 verdict: mean ≥ 0.95 and every bucket ≥ 0.85; `None` with no data.
    #[must_use]
    pub fn passes(&self) -> Option<bool> {
        if self.buckets.is_empty() {
            return None;
        }
        let n = self.buckets.len() as u128;
        let bucket_ok = self.buckets.values().all(|(c, t)| {
            u128::from(*c) * u128::from(H1_BUCKET_MIN.1)
                >= u128::from(*t) * u128::from(H1_BUCKET_MIN.0)
        });
        // mean_i(c_i/t_i) >= p/q  <=>  sum_i c_i * (L/t_i) * q >= p * n * L, L = prod t_i.
        let l: u128 = self.buckets.values().map(|(_, t)| u128::from(*t)).product();
        let lhs: u128 = self
            .buckets
            .values()
            .map(|(c, t)| u128::from(*c) * (l / u128::from(*t)))
            .sum::<u128>()
            * u128::from(H1_MEAN_MIN.1);
        let rhs = u128::from(H1_MEAN_MIN.0) * n * l;
        Some(bucket_ok && lhs >= rhs)
    }
}

/// Binary confusion counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Confusion {
    pub tp: u64,
    pub fp: u64,
    pub tn: u64,
    pub fn_: u64,
}

impl Confusion {
    pub fn record(&mut self, predicted: bool, gold: bool) {
        match (predicted, gold) {
            (true, true) => self.tp += 1,
            (true, false) => self.fp += 1,
            (false, false) => self.tn += 1,
            (false, true) => self.fn_ += 1,
        }
    }

    /// F1 = 2tp / (2tp + fp + fn); `None` when undefined.
    #[must_use]
    pub fn f1(&self) -> Option<f64> {
        let d = 2 * self.tp + self.fp + self.fn_;
        (d > 0).then(|| (2 * self.tp) as f64 / d as f64)
    }

    /// FPR on membership negatives = fp / (fp + tn); `None` with no negatives.
    #[must_use]
    pub fn fpr(&self) -> Option<f64> {
        let d = self.fp + self.tn;
        (d > 0).then(|| self.fp as f64 / d as f64)
    }
}

/// Task-2 scorer per instance: a value query is a positive answered correctly
/// (tp) or not (fn; a wrong value also counts fp); membership feeds `membership`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct H2Tally {
    /// k -> confusion over value + present-membership answers
    pub per_k: std::collections::BTreeMap<usize, Confusion>,
    /// absent-key membership negatives across all k
    pub membership_negatives: Confusion,
}

/// Arm answer for one task-2 query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Task2Answer {
    Value(Option<Vec<u8>>),
    Present(bool),
}

impl H2Tally {
    pub fn record(&mut self, k: usize, query: &Task2Query, answer: &Task2Answer) {
        let c = self.per_k.entry(k).or_default();
        match (query, answer) {
            (Task2Query::ValueOf { answer: gold, .. }, Task2Answer::Value(pred)) => match pred {
                Some(p) if p == gold => c.tp += 1,
                Some(_) => {
                    c.fn_ += 1;
                    c.fp += 1;
                }
                None => c.fn_ += 1,
            },
            (Task2Query::IsPresent { present: true, .. }, Task2Answer::Present(p)) => {
                c.record(*p, true);
            }
            (Task2Query::IsPresent { present: false, .. }, Task2Answer::Present(p)) => {
                self.membership_negatives.record(*p, false);
            }
            // Mismatched answer kind counts as a miss on a positive / fp on a negative.
            (Task2Query::IsPresent { present: false, .. }, Task2Answer::Value(_)) => {
                self.membership_negatives.record(true, false);
            }
            (_, _) => c.fn_ += 1,
        }
    }

    /// Macro-F1 over k values present in the tally.
    #[must_use]
    pub fn macro_f1(&self) -> Option<f64> {
        let f: Vec<f64> = self.per_k.values().filter_map(Confusion::f1).collect();
        (!f.is_empty()).then(|| f.iter().sum::<f64>() / f.len() as f64)
    }
}

/// Task-3 scorer: exact match on the final value C.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct H3Tally {
    pub correct: u64,
    pub total: u64,
}

impl H3Tally {
    pub fn record(&mut self, correct: bool) {
        self.correct += u64::from(correct);
        self.total += 1;
    }

    #[must_use]
    pub fn passes(&self) -> Option<bool> {
        (self.total > 0).then(|| {
            u128::from(self.correct) * u128::from(H3_ACC_MIN.1)
                >= u128::from(self.total) * u128::from(H3_ACC_MIN.0)
        })
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --locked -p sml-store --test longctx_b1
```

Expected: PASS — `test result: ok. 11 passed; 0 failed`.

- [ ] **Step 5: Commit**

```bash
git add crates/sml-store/src/longctx_b1.rs crates/sml-store/tests/longctx_b1.rs
git commit -m "feat(longctx-b1): fixture-only H1-H3 scorer stubs"
```

---

## Task 8: Per-cell §8 accounting record, frozen caps, arm hook and exact-scan smoke arm

Prereg §8 field list, with `None` for anything the harness does not measure yet (RSS, VRAM, latency). `campaign_admissible` refuses unpinned filler and off-ladder rungs. `B1Arm` is the hook for CONTEXT-3, B-lex and B-attn. `ExactScanSmokeArm` is a harness smoke test only (prereg §6: “running only `sml-ctx-raw-scan` is a harness smoke test, not a B1 result”).

**Files:**
- Modify: `crates/sml-store/tests/longctx_b1.rs`
- Modify: `crates/sml-store/src/longctx_b1.rs`

- [ ] **Step 1: Write the failing tests**

Append to `crates/sml-store/tests/longctx_b1.rs`:

```rust
fn record() -> CellRecord {
    CellRecord {
        tokenizer: SML_BYTE_V0,
        rung: 4096,
        arm: B1ArmId::SmlCtxRawScan,
        seed: 0xB100_0001,
        split: B1Split::Dev,
        filler_id: "fixture-filler-unpinned".into(),
        filler_sha256: None,
        bytes_ingested: 4096,
        byte_tokens: 4096,
        resident_c0_bytes: None,
        segments_shortlisted: 4,
        segments_decoded: 2,
        raw_bytes_reread: 32,
        rss_peak_bytes: None,
        mapped_bytes: None,
        disk_bytes: None,
        vram_peak_bytes: None,
        latency_p50_us: None,
        latency_p95_us: None,
        latency_p99_us: None,
        host: "fixture".into(),
        commit_sha: "0000000".into(),
        outcome: CellOutcome::Completed,
    }
}

#[test]
fn b1_cell_record_invariants_and_campaign_gate() {
    assert_eq!(record().validate(), Ok(()));
    assert_eq!(
        record().campaign_admissible(),
        Err(B1Error::FillerNotPinned)
    );
    let mut r = record();
    r.byte_tokens = 4095;
    assert!(r.validate().is_err());
    let mut r = record();
    r.segments_decoded = 5;
    assert!(r.validate().is_err());
    let mut r = record();
    r.split = B1Split::Test;
    assert!(r.validate().is_err());
    assert_eq!(
        apply_caps(60_001, None),
        CellOutcome::BudgetFail("per-query wall clock > 60 s")
    );
    assert_eq!(
        apply_caps(10, Some(CAP_PROCESS_RSS_BYTES + 1)),
        CellOutcome::BudgetFail("process RSS > 16 GiB")
    );
    assert_eq!(apply_caps(10, Some(1)), CellOutcome::Completed);
}

#[test]
fn b1_exact_scan_smoke_arm_answers_generated_fixtures() {
    let f = filler();
    let mut arm = ExactScanSmokeArm;
    let t1 = generate_task1(&f, 0xB100_0001, 4096, 900, 0).unwrap();
    assert!(exact_match(
        arm.lookup_value(&t1.haystack, &t1.query_key).as_deref(),
        &t1.answer
    ));
    let t2 = generate_task2(&f, 0xB100_0001, 4096, 4, 0).unwrap();
    for q in &t2.queries {
        match q {
            Task2Query::ValueOf { key, answer } => {
                assert_eq!(arm.lookup_value(&t2.haystack, key).as_ref(), Some(answer))
            }
            Task2Query::IsPresent { key, present } => {
                assert_eq!(arm.contains_key(&t2.haystack, key), *present)
            }
        }
    }
    let t3 = generate_task3(&f, 0xB100_0001, 4096, 0).unwrap();
    let b = arm.follow_link(&t3.haystack, &t3.start_key).unwrap();
    assert!(exact_match(
        arm.lookup_value(&t3.haystack, &b).as_deref(),
        &t3.answer
    ));
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test --locked -p sml-store --test longctx_b1
```

Expected: FAIL to compile — the new items are not found in `sml_store::longctx_b1`.

- [ ] **Step 3: Write minimal implementation**

Append to `crates/sml-store/src/longctx_b1.rs`:

```rust
/// Frozen caps (prereg §8).
pub const CAP_QUERY_WALL_CLOCK_MS: u64 = 60_000;
pub const CAP_PROCESS_RSS_BYTES: u64 = 16 * 1024 * 1024 * 1024;
pub const CAP_RUNG_WALL_CLOCK_MS: u64 = 6 * 60 * 60 * 1000;

/// Arms and baselines (prereg §6–§7). `BHybCompetitor` is a competitor only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum B1ArmId {
    SmlCtxRawScan,
    SmlCtxSbgMorTopK,
    SmlCtxRecurrent,
    SmlCtxCompacted,
    BLex,
    BAttn,
    BHybCompetitor,
}

/// Fail-closed cell outcome (prereg §8): no silent retry with a larger budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CellOutcome {
    Completed,
    BudgetFail(&'static str),
    ProvenanceMismatch,
    HashMismatch,
    Oom,
    Timeout,
    NotApplicable(&'static str),
}

/// Per-cell accounting record; fields mirror prereg §8 exactly.
/// `None` = not measured by this harness (e.g. RSS without a host probe).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellRecord {
    pub tokenizer: &'static str,
    pub rung: u64,
    pub arm: B1ArmId,
    pub seed: u64,
    pub split: B1Split,
    pub filler_id: String,
    pub filler_sha256: Option<ContextContentHash>,
    pub bytes_ingested: u64,
    pub byte_tokens: u64,
    pub resident_c0_bytes: Option<u64>,
    pub segments_shortlisted: u64,
    pub segments_decoded: u64,
    pub raw_bytes_reread: u64,
    pub rss_peak_bytes: Option<u64>,
    pub mapped_bytes: Option<u64>,
    pub disk_bytes: Option<u64>,
    pub vram_peak_bytes: Option<u64>,
    pub latency_p50_us: Option<u64>,
    pub latency_p95_us: Option<u64>,
    pub latency_p99_us: Option<u64>,
    pub host: String,
    pub commit_sha: String,
    pub outcome: CellOutcome,
}

impl CellRecord {
    /// Structural invariants that hold for fixtures and campaigns alike.
    pub fn validate(&self) -> Result<(), B1Error> {
        if self.tokenizer != SML_BYTE_V0 {
            return Err(B1Error::AccountingInvariant(
                "tokenizer must be sml-byte-v0",
            ));
        }
        if self.byte_tokens != self.bytes_ingested {
            return Err(B1Error::AccountingInvariant(
                "sml-byte-v0 requires byte_tokens == bytes_ingested",
            ));
        }
        if self.segments_shortlisted < self.segments_decoded {
            return Err(B1Error::AccountingInvariant("shortlisted >= decoded"));
        }
        if B1Split::for_seed(self.seed)? != self.split {
            return Err(B1Error::AccountingInvariant("split does not match seed"));
        }
        if let (Some(p50), Some(p95), Some(p99)) = (
            self.latency_p50_us,
            self.latency_p95_us,
            self.latency_p99_us,
        ) {
            if !(p50 <= p95 && p95 <= p99) {
                return Err(B1Error::AccountingInvariant("p50 <= p95 <= p99"));
            }
        }
        Ok(())
    }

    /// Campaign admission: structural validity, ladder rung and a pinned filler.
    pub fn campaign_admissible(&self) -> Result<(), B1Error> {
        self.validate()?;
        validate_span(self.rung)?;
        if self.filler_sha256.is_none() {
            return Err(B1Error::FillerNotPinned);
        }
        Ok(())
    }
}

/// Applies frozen caps to observed values; any excess is `budget-fail`.
#[must_use]
pub fn apply_caps(max_query_ms: u64, rss_peak_bytes: Option<u64>) -> CellOutcome {
    if max_query_ms > CAP_QUERY_WALL_CLOCK_MS {
        return CellOutcome::BudgetFail("per-query wall clock > 60 s");
    }
    if rss_peak_bytes.is_some_and(|r| r > CAP_PROCESS_RSS_BYTES) {
        return CellOutcome::BudgetFail("process RSS > 16 GiB");
    }
    CellOutcome::Completed
}

/// Arm hook: later PRs implement this for CONTEXT-3, B-lex and B-attn.
pub trait B1Arm {
    fn id(&self) -> B1ArmId;
    /// Exact value bytes for `key`, or `None` to abstain.
    fn lookup_value(&mut self, haystack: &[u8], key: &[u8]) -> Option<Vec<u8>>;
    /// Membership answer for `key`.
    fn contains_key(&mut self, haystack: &[u8], key: &[u8]) -> bool;
    /// `NEXT=` target of `LINK=<from>`, or `None`.
    fn follow_link(&mut self, haystack: &[u8], from: &[u8]) -> Option<Vec<u8>>;
}

/// Harness smoke arm: exhaustive exact scan. Not a B1 result (prereg §6).
#[derive(Debug, Default)]
pub struct ExactScanSmokeArm;

fn value_after(haystack: &[u8], prefix: &[u8], width: usize) -> Option<Vec<u8>> {
    let at = find(haystack, prefix)? + prefix.len();
    haystack.get(at..at + width).map(<[u8]>::to_vec)
}

impl B1Arm for ExactScanSmokeArm {
    fn id(&self) -> B1ArmId {
        B1ArmId::SmlCtxRawScan
    }

    fn lookup_value(&mut self, haystack: &[u8], key: &[u8]) -> Option<Vec<u8>> {
        let mut prefix = b"KEY=".to_vec();
        prefix.extend_from_slice(key);
        prefix.extend_from_slice(b" VALUE=");
        value_after(haystack, &prefix, 32)
    }

    fn contains_key(&mut self, haystack: &[u8], key: &[u8]) -> bool {
        let mut prefix = b"KEY=".to_vec();
        prefix.extend_from_slice(key);
        prefix.push(b' ');
        find(haystack, &prefix).is_some()
    }

    fn follow_link(&mut self, haystack: &[u8], from: &[u8]) -> Option<Vec<u8>> {
        let mut prefix = b"LINK=".to_vec();
        prefix.extend_from_slice(from);
        prefix.extend_from_slice(b" NEXT=");
        value_after(haystack, &prefix, 16)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --locked -p sml-store --test longctx_b1
```

Expected: PASS — `test result: ok. 13 passed; 0 failed`.

- [ ] **Step 5: Commit**

```bash
git add crates/sml-store/src/longctx_b1.rs crates/sml-store/tests/longctx_b1.rs
git commit -m "feat(longctx-b1): per-cell accounting, caps and arm hook"
```

---

## Task 9: Local gates and harness PR (no merge)

**Files:** none modified.

- [ ] **Step 1: Format check**

```bash
cargo fmt --all -- --check
```

Expected: exit 0, no output. If it fails, run `cargo fmt --all`, re-run Step 1, and commit with `style(longctx-b1): rustfmt`.

- [ ] **Step 2: Clippy with warnings denied**

```bash
cargo clippy --locked --workspace --all-targets -- -D warnings
```

Expected: `Finished` with no warnings or errors.

- [ ] **Step 3: Full workspace tests**

```bash
cargo test --locked --workspace
```

Expected: every `test result:` line is `ok`; `longctx_b1` shows `13 passed; 0 failed`.

- [ ] **Step 4: Verify no metric or claim wording slipped in**

```bash
git diff origin/main -- . | grep -nEi "H[1-4] (pass|met|satisfied)|[0-9]+(k|M)-token context|1M context|accuracy (of|=) ?0\.[0-9]" || echo "no claim wording"
```

Expected: `no claim wording`.

- [ ] **Step 5: Push without bypass and open the PR, do not merge**

```bash
git config core.hooksPath   # expected: .githooks
git push -u origin HEAD     # never --no-verify
gh pr create --base main --title "feat(longctx-b1): fixture-only harness substrate (generator, scorer stubs, accounting)" --body "Implements docs/superpowers/plans/2026-11-01-longctx-b1-harness.md for prereg #765 §12 item 5. Hand-written fixtures only; no campaign, no metrics, no H1-H4 claim. Filler pin, B-lex, B-attn, CONTEXT-3 arm adapter and RemoteOps dispatch are follow-ups through the B1FillerSource / B1Arm / CellRecord hooks."
```

Expected: the PR URL is printed. Merge only after every mandatory CI check is green on the exact head SHA (`gh pr checks <PR>`, `gh pr view <PR> --json headRefOid`). After the merge, a separate docs PR checks the ROADMAP “B1 harness” sub-bullet.

---

## Self-review against the preregistration

- §2 tokenizer: `SML_BYTE_V0`, `sml_byte_v0_token_count`, and `CellRecord::validate` enforcing `byte_tokens == bytes_ingested` (Tasks 1, 8). ✔
- §3 ladder: `SPAN_LADDER`, `validate_span`, `campaign_admissible` rejects off-ladder rungs. Fixtures use 4096 = rung 1 or small non-ladder spans only through generators, never through `campaign_admissible` (Tasks 1, 8). ✔
- §4 thresholds: frozen rationals `H1_MEAN_MIN`, `H1_BUCKET_MIN`, `H2_F1_MIN`, `H2_FPR_MAX`, `H3_ACC_MIN`, `H4_EXPONENT_MAX`. Verdicts come from integer arithmetic. H4 fitting is deferred, with its reason stated (Task 7). ✔
- §5 tasks 1–3: formats, depths, 16 near-miss distractors, k ∈ {2, 4, 8}, 50% absent, hops ≥ L/2, prefix-sharing distractor chain. `INSTANCES_PER_CELL = 20` is frozen for the runner. Task 4 (deferred NL tasks) is not generated (Tasks 4–6). ✔
- §5/§9 filler: unpinned → `FillerNotPinned`; needle in filler → `NeedleInFiller` abort; absent membership keys checked against the whole haystack (Tasks 3, 5). ✔
- §6 arms: `B1ArmId` lists all four SML arms. The raw-scan smoke arm is labelled not-a-result (Task 8). ✔
- §7 baselines: `BLex`, `BAttn`, `BHybCompetitor` ids exist as hook targets only. No implementation, and HYB is a competitor only (Task 8). ✔
- §8 accounting: every listed field is present, plus fail-closed `CellOutcome` and frozen caps 60 s / 16 GiB / 6 h (Task 8). ✔
- §9 seeds/splits/leakage: seeds `0xB1000001`..`0xB1000005`, DEV = 1–2, TEST = 3–5, disjoint nibble namespaces (Tasks 1–2). ✔
- §10 stop rules / wording gate: not implemented here (runner PR). Nothing in this plan produces wording. ✔
- §12: this plan closes item 5 only after merge. Items 2–4 remain open, and item 1 is already closed by #768. ✔
