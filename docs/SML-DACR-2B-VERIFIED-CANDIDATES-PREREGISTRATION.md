# DACR-2B — frozen verification bridge for directly addressed candidates

**Research prototype — protocol frozen before implementation:** 2026-11-09. **Source code base:** SML-GENIUS main at `22365ea6691b099dc860a4573df8066da0b61a74`. Owner: SML-GENIUS.

## Goal

Concretely connect an external directly addressed **untrusted shortlist of CONTEXT-0 segment IDs** (for example DACR-2A's SBG address index) to **existing** CONTEXT-3 source verification and bounded MOR top-k ranking, without a global memory scan, a new format, a second SHA-256 implementation, or bypassing the immutable raw/source manifest. This path is an additive API in `sml-store::context_retrieve`. It is *not* yet full inference/model training.

## Semantics frozen

Proposed API:
```rust
ContextRetriever<BITS>::retrieve_from_candidate_ids(
    &self,
    candidates: &[ContextSegmentId],
    query: &SemanticBitField<BITS>,
    policy: &ContextRetrievePolicy,
) -> Result<(Vec<RetrieveHit>, RetrieveAccounting), ContextRetrieveError>
```

- Validate `policy` using existing `ContextRetrievePolicy::validate`; never widen frozen fanouts 1,2,4.
- Validate candidate length `<= max_shortlist` **before any source read**. For an empty list return explicit empty hits and zero shortlist and raw-reread counts (it is a candidate miss, not automatic global fallback).
- Reject cross-stream IDs, zero IDs and IDs outside the indexed `1..=segment_count` *before any source read*. Reject duplicate candidate IDs explicitly; a repeated ID must never count as independent evidence.
- Use the existing CONTEXT-3 in-memory `IndexEntry` metadata to map an accepted segment ID to a bounded index slot. No iteration over all indexed segments in the query path.
- Calculate Hamming only within the explicitly supplied candidate shortlist; call existing `rank` (stable `(distance,segment_id)`, top-k) and existing `reread` / `reread_segment`. Raw content SHA-256 and provenance checks remain authoritative. `source_exact` continues to mean *SBG signature equality*, **not** byte-exact match nor truth of candidate relevance. No source claim follows solely from a latent match.
- Harden shared CONTEXT-3 raw reread by comparing raw artifact `semantic_record_ids()` with current CONTEXT-1c manifest IDs; mismatch -> fail closed `MissingProvenance`, even if raw bytes and SHA-256 match. This affects all retrieve paths and must have a regression.
- Preserve `RetrieveAccounting::invariant_holds()` and report logical `shortlisted`, `decoded`, `raw_reread_segments`, `raw_reread_bytes`; these are **not** physical DRAM traffic, page faults, end-to-end latency or AI quality measures.
- For candidate-identity failures use a dedicated error variant, not an ambiguous `HashMismatch`. Reject same-stream invalid/out-of-index source ID. No new CUDA/vendor SDK or model behavior changes.

## Mandatory tests, synthetic only

1. Given an out-of-prefix-bucket candidate, read the supplied segment directly and verify it; usual CONTEXT-3 prefix search may miss it, but control comparisons are not quality claims.
2. Candidate shortlist 4, top-k 2: ordered rank exactly matches the full exhaustive control *when all relevant candidates are explicitly in the shortlist*; verify accounting and raw bytes.
3. Empty candidates return no hits and read no source.
4. Candidate length exceeding cap, mixed stream, unindexed ID, duplicate ID and invalid policy reject before source reads.
5. Valid raw bytes with **re-issued/forged provenance semantic record IDs** but unchanged content hash fail closed in both candidate method and original `retrieve`. Ensure fixture's provenance artifact CRC remains valid.
6. Corrupt or missing raw/provenance in indexed candidate path fail closed; source integrity and identity match remain exercised by existing CONTEXT-3 tests.
7. Repeated identical candidate inputs yield identical outputs; no scan of all signatures in the new query path.

**CI gates:** `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace`; exact PR head check-runs green before merge. No physical deployment or RemoteOps interaction in this slice. Do not consume LONGCTX-B1 protected TEST. No novelty, 25B, latency, memory-bandwidth or trained-quality claim.

**Next program milestone:** once both DACR-2A and this separate integration gate are accepted, a separately frozen DACR-2C task-level retrieval experiment may quantify true task exactness and hard negatives for learned vs fixed prefixes on a new held-out corpus. A source-verified hit is not evidence that the recovered source was semantically relevant.
