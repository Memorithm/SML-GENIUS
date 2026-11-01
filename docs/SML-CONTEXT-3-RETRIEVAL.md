# SML-CONTEXT-3 — hierarchical retrieval (design)

## Status

**DESIGN FROZEN, NOT IMPLEMENTED.** This document freezes the CONTEXT-3 tranche B design. It contains no measured numbers. No code ships with it, and no LONGCTX-B1 campaign is started or authorized by it.

## Track

SML-CONTEXT → CONTEXT-3 hierarchical retrieval (see [`SML-CONTEXT-BOOTSTRAP.md`](SML-CONTEXT-BOOTSTRAP.md)).

The primary arm `sml-ctx-sbg-mor-topk` of the LONGCTX-B1 preregistration ([`SML-LONGCTX-B1-PREREGISTRATION.md`](SML-LONGCTX-B1-PREREGISTRATION.md)) stays blocked until this design is implemented and merged with its tests. Landing this design alone unblocks nothing.

## Non-goals

- No 1M-token context claim. CONTEXT-3 is **not** a 1M context window.
- A mapped store is not a context window. Bytes on disk or ingested segments are not effective context.
- No mandatory NVIDIA stack (no CUDA/GPU requirement).
- No copy of HYB/Claude designs or code.
- No B1 run.
- CONTEXT-2 (recurrent working context) and CONTEXT-4 (retention/compaction) are out of scope.

## Architecture (approach 1)

New files, added by a follow-up implementation PR:

- `crates/sml-store/src/context_retrieve.rs` — retrieval module;
- `crates/sml-store/tests/context_retrieve.rs` — mandatory tests below;
- this document.

Reused, not rewritten:

- CONTEXT-0 ledger (byte/token/span accounting, stable segment IDs);
- CONTEXT-1a immutable SHA-256 raw segment store;
- CONTEXT-1c provenance manifest and semantic emission;
- SBG bitfield representation;
- `SemanticMorTopK` and existing MOR hierarchy primitives;
- `BoundedAssociativeMemory`;
- SHA-256 reread verification.

MOR and Core are not rewritten by this track.

## Flow

1. SBG query: build the query bitfield.
2. Shortlist: bounded SBG candidate shortlist (`max_shortlist`).
3. MOR top-k: rank shortlist with existing MOR top-k primitives (`top_k`), stable sort.
4. Optional bounded associative lookup (`associative_slots`).
5. Raw reread of selected segments plus SHA-256 hash verification against provenance.
6. Accounting emission.

## API (frozen shape)

```rust
pub struct ContextRetrievePolicy {
    pub max_shortlist: usize,
    pub top_k: usize,
    pub associative_slots: usize,
    pub fail_on_hash_mismatch: bool, // must be true
}

pub struct ContextCandidate { /* segment id, SBG score, provenance ref */ }
pub struct RetrieveQuery    { /* query bitfield + optional associative key */ }
pub struct RetrieveHit      { /* segment id, rank, verified raw bytes/span */ }
pub struct RetrieveAccounting { /* see below */ }

impl ContextRetriever {
    pub fn retrieve(&self, query: &RetrieveQuery, policy: &ContextRetrievePolicy)
        -> Result<(Vec<RetrieveHit>, RetrieveAccounting), ContextRetrieveError>;
}

/// Control only: exhaustive semantic scan over all segments.
pub fn exhaustive_semantic_scan(/* ... */);

pub enum ContextRetrieveError {
    EmptyIndex,
    BudgetOverflow,
    HashMismatch,
    MissingRaw,
    MissingProvenance,
    AssociativeDenied,
    Io(std::io::Error),
}
```

Field details beyond those named here are fixed by the implementation PR without changing this contract.

## Accounting

`RetrieveAccounting` reports:

- `shortlisted`
- `decoded`
- `raw_reread_bytes`
- `associative_hits`
- `associative_misses`

Invariant: `shortlisted >= decoded >= reread segments`.

## Mandatory tests

1. Shortlist/top-k budgets respected with stable sort.
2. Exact needle hit returned only after SHA-256 verification succeeds.
3. Hamming near-miss does not produce a false source-exact hit.
4. Hash mismatch or truncated raw segment aborts.
5. Missing raw segment or provenance returns an error.
6. Associative hit, miss and full-capacity behaviour.
7. Exhaustive scan and shortlist return the same hit; segments read ≪ N.
8. Empty index or zero budget fails validation.
9. Multi-query independence.
10. Sublinear fit: exponent α < 0.75 on L ∈ {256, 512, 1024, 2048, 4096} on a declared fixture, **and** negative control: exhaustive scan α ≈ 1.
11. Accounting invariant holds.
12. Determinism across two runs.

## Scope of the sublinear proof

The sublinear proof (test 10) is fixture-only. It does **not** satisfy LONGCTX-B1 H4 nor any wording gate, and supports no scale claim.

## Implementation plan

1. Follow-up PR implements `context_retrieve` and its tests.
2. Separate later work: filler SHA, B-lex, B-attn, B1 harness — all before any B1 campaign.
