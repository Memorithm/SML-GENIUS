# DACR-2D — bounded unknown-mask address expansion (frozen before code)

2026-11-09. SML-GENIUS research-only additive feature. Source provenance remains with CONTEXT-1c/CONTEXT-3; DACR-2A pairwise training, DACR-2B verification and DACR-2C composition are unchanged. Prototype source base: branch `research/dacr2c-learned-verified-replay-20261109` at `8e46cbe6cc34187bce1443c2896438250ccc83d1`. This is not a trained language model, semantic generalization, task-loss result, benchmark or proof of constant physical I/O.

## Problem

DACR-2A's exact hard address rejects *any* unknown selected SBG bit to avoid unknown->false coercion. That is safe but may cause avoidable misses if one or two selected address coordinates are unknown. Implement **opt-in** bounded exhaustive enumeration of both assignments of the unknown selected bits only, with strict preflight and total candidate budgets, no source read and no token/history scan. This is uncertainty expansion over a small **selected address**, not dense similarity retrieval.

## New interfaces

- `CandidateIndex::lookup_partial(stream, epoch, address, max_unknown_bits, max_address_probes, max_candidates)`. Existing `lookup` remains exact / fail closed.
- `ContextRetriever::retrieve_with_partial_latent_candidates(index, epoch, address_query, mor_query, policy, max_unknown_bits, max_address_probes)` invokes `lookup_partial` with `max_candidates = policy.max_shortlist`, then existing `retrieve_from_candidate_ids` (MOR top-k and raw/provenance SHA-256 reread). It must not relax any existing exact source or epoch checks.

## Frozen semantics / hard budget

1. `max_unknown_bits <= encoder.address_bits`, `1 <= max_address_probes <= 256`, `1 <= max_candidates <= max_entries`. Reject invalid caps `ProbeBudget` before lookup.
2. Wrong stream or epoch rejects before any lookup. Known selected feature bits define a base code; for **every unknown selected feature**, enumerate both assignments irrespective of its underlying value bit. Unknown is never silently coerced false or silently ignored.
3. The number of probes is exactly `2 ^ unknown_selected_count`. Reject `UnknownSelectedFeature` if `unknown_selected_count > max_unknown_bits`; reject `ProbeBudget` if the resulting number of probes exceeds `max_address_probes` — both before bucket access. No query path iteration over all populated buckets.
4. Probe codes in increasing assignment order over unknown-feature positions as ordered by the frozen encoder. Use direct `BTreeMap::get` for each code; collect existing source IDs in stored per-bucket insertion order. A source cannot appear in two address buckets due to existing unique source admission.
5. If cumulative returned source IDs exceed `max_candidates`, reject **all** with `CandidateOverflow`; never truncate or emit an arbitrarily selected subset. No raw source reread should occur on this rejection.
6. Work accounting: `address_buckets_probed` = number of exact code probes (including absent ones), `candidates_returned` = emitted candidate segment IDs. These logical counters do not measure physical page faults, bytes or latency.
7. The final path authenticates only the integrity/provenance of returned sources. It must never claim that a source is relevant or that latent matching proves a literal fact.

## Test protocol, predeclared and synthetic

- Two-bit address frozen on value positions 4 and 6. Source ID 1 has code 0; ID 2 code 1; ID 3 code 2; ID 4 code 3; each source is a valid existing CONTEXT-1c fixture. With bit 4 unknown and bit 6 known false, lookup exactly two addresses (codes 0 and 1) and return IDs [1,2]. Changing the underlying unknown value from false to true produces **identical** candidate IDs. The ordinary exact `lookup` must reject.
- With both positions unknown, exactly four codes are visited and IDs [1,2,3,4] are returned when allowed. With one-probe budget or only one allowed unknown, hard error (not partial output). Candidate budget below 4 errors without truncating.
- Wrong epoch/stream and invalid zero/over-ceiling budgets reject. Zero unknown bits probes one code only and equals exact lookup.
- Retrieval integration with valid source fixtures uses MOR query for ID 2 and `top_k=1` to return a single **raw verified** hit from the [1,2] shortlist; record both address and source accounting.
- Tampered raw source ID 2 fails source SHA-256; an oversized candidate shortlist rejects *before* any source read, even when one candidate file is missing.
- Deterministic duplicate requests return equal IDs and counters; no changes in default inference, ONNX/Transformer attention, NVIDIA SDK, or host state.

## CI and promotion

Use `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace`; exact-head GitHub checks fully green before any merge. PRs #913 and #915 are prerequisites and remain separate gates. Keep LONGCTX-B1 protected TEST untouched. The synthetic feature may be merged as **research code**, but only natural-language held-out / model-training / physical RemoteOps qualification can establish quality or cost benefit.
