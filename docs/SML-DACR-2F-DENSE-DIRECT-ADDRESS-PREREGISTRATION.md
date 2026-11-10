# DACR-2F — dense fixed-capacity Boolean address table

**Frozen before code, 2026-11-09**; SML-GENIUS research prototype, starting from main at `3b639c52d85838ac378a1d3818bf3e87ce0c7b55`. Independent from pending PRs #913/#915/#916/#917. Preserve source integrity and model behavior; this work does not promote the prototype to production.

## Problem and proposed mechanism

The DACR-2A frozen `DiscreteAddress` contains at most eight selected known SBG bits, yielding at most **256** distinct hard addresses. Yet `CandidateIndex` stores buckets in a `BTreeMap<u16, Vec<ContextSegmentId>>`: the read path must locate its bucket by tree search, which is `O(log(number_of_nonempty_buckets))` key comparisons. Replace this map with **an address-direct dense table**, `Vec<Vec<ContextSegmentId>>` of exactly `2^address_bits` buckets allocated during `CandidateIndex::new`. A query uses one index `buckets[code as usize]`, without a map probe or historical scan. The unique-segment `BTreeSet` remains on the *write* path only to reject duplicates. The learned coordinate encoder and each bucket's deterministic insertion order remain unchanged.

This is a source-level structural improvement: one direct vector indexing operation rather than a tree search to identify the bucket. The additional fixed bucket headers consume `2^address_bits * size_of::<Vec<ContextSegmentId>>()` bytes, excluding candidates, allocation overhead and vector capacity. On ordinary 64-bit Rust, a `Vec` header is typically 24 bytes, so 256 slots are approximately 6144 header bytes, but do **not** claim an exact byte count across architectures; no new memory consumption claim until measured. Do not claim wall-clock, physical DRAM, page-fault, bandwidth, or training-quality improvements.

## Exact semantics frozen

- Keep public `CandidateIndex::{new, insert, lookup, len, is_empty}` signatures unchanged and existing `AddressBudget` validation. `1 <= address_bits <= min(BITS,8)` implies `2^address_bits <= 256`; no unbounded allocation.
- `new` materializes exactly `1_usize << address_bits` initially empty slots. Extend the API with `address_slot_count()` reporting the fixed number of slots for diagnostic auditing.
- `insert` validates stream, nonzero segment, selected known bits, duplication, total `max_entries` and per-address `max_bucket_entries` **before any mutation**. No eviction; keep insertion order.
- `lookup` validates stream, epoch and selected known bits before querying; a missing/empty bucket returns empty candidates and `address_buckets_probed=1`. It never scores history or treats address matching as a verified source hit.
- The number of direct bucket accesses is one per exact lookup. Preserve `CandidateAccounting` semantics, all tests, source candidate identities and fail-closed behaviour.

## Frozen test fixtures / oracle

1. `address_bits=8, BITS=16`, fixed positions `[0,1,2,3,4,5,6,7]`, admit one source per each of the 256 codes in *reverse* insertion order. For each queried address 0..255, exactly one matching source ID must be returned, with `address_buckets_probed=1` and `candidates_returned=1`. A duplicate source is rejected and does not change the index. Two runs must be deterministic.
2. `address_bits=2`, source collisions in the same address, `max_bucket_entries=2`; third insertion rejected and previous candidates unchanged. Full `max_entries` rejected before mutation.
3. Unknown selected bit, wrong epoch, wrong stream, invalid budgets still reject. Empty/missing codes return empty and one bucket probe. Index length and slot count remain independent.
4. Compare all candidate IDs and per-query counters against a **test-only** `BTreeMap` oracle on the exact same deterministic fixture; do not use benchmark timing to assert speedups.

**CI:** exact PR head with `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings` and `cargo test --locked --workspace`; all required checks green before merge. If GitHub Actions returns zero-job failures, report infrastructure blockage instead of treating absent checks as success. Do not run unreviewed branch code on privileged RemoteOps/self-hosted infrastructure. No NVIDIA SDK. Longer-term model-training and physical performance qualification are separate gates.
