# SML-DACR-2C — learned-address → verified-source replay (frozen before code)

Date: 2026-11-09. **Status:** bounded synthetic research integration only. Prerequisite: PR #913 direct candidate source-verification API. The SML-0.6, DACR-1, DACR-2A and CONTEXT-3 contracts remain authoritative.

## Problem and non-claims

DACR-2A trains a hard SBG feature selector from *same/different* supervision and indexes candidate CONTEXT segment IDs. DACR-2B can verify a caller-supplied shortlist through existing MOR top-k and immutable raw reread. **DACR-2C** connects these two code paths through one callable Rust API without a historical scan, silently treating unknown SBG bits as false, accepting an oversized bucket, or returning a source assertion solely because the latent code matched.

Not a learned next-token model, not learned weights from downstream task loss, not a natural-language semantic retrieval qualification, not 25B performance or source authorization. No changes to the default SML inference route.

## Frozen API

`ContextRetriever::<BITS>::retrieve_with_latent_candidates(&self, index: &CandidateIndex<BITS>, epoch: u64, address_query: &SbgAddress<BITS>, mor_query: &SemanticBitField<BITS>, policy: &ContextRetrievePolicy) -> Result<(Vec<RetrieveHit>, RetrieveAccounting, CandidateAccounting), ContextRetrieveError>`.

Validation order: (1) validate existing retrieval policy; (2) lookup by this retriever's `stream_id`, explicit nonzero index epoch and selected known SBG address bits, preserving the latent index's fail-closed errors; (3) pass the **entire** returned shortlist to `retrieve_from_candidate_ids`; if it exceeds the frozen `max_shortlist`, fail **without** truncating or rereading; (4) use existing MOR Hamming top-k and verified CONTEXT-1c raw/provenance reread. No fallback to `retrieve()` or an all-store scan. Separate candidate work counters from verified reread counters.

A source-verified hit establishes that the raw artifact is intact and source-bound, **not** that the selected source answers the user's question. An exact SBG signature match does not establish literal source-key equality.

## Frozen test fixture (not new unseen semantic classes)

On the existing CONTEXT-1c ingestion fixture, use classes 0..3, map each class deterministically to segment ID class+1. Build synthetic 64-bit SBG address inputs with two semantic class bits at positions `[4,6]` and three nuisance bits `[0,1,2]`. Training pairs, frozen *before evaluation*: 4 positive pairs (same class, nuisance 000 vs 111) and 8 negative pairs (same nuisance 010, class flipped in one of two semantic bits). Training consists only of pairwise same/different labels, not target addresses. Each source inserted with nuisance `001`. TEST query nuisance `110` is absent from training and indexing. Compare learned selected `[4,6]` to fixed `[0,1]` at equal index budgets. Query the existing CONTEXT-3 raw/provenance store using the class-specific MOR fixture signature.

Criterion: learned direct address should return the class-matched, source-integrity-verified segment in all four synthetic cases; fixed control should return fewer than four (exact counts retained). This proves only invariance to nuisance changes on seen class identities. Zero hallucination/unknown mask treatment: if a selected bit's known-mask is unset, lookup errors; there must be no fallback. A latent code collision has no source-authority semantics, and the bucket cap is enforced before source work.

Additional negative controls: query wrong epoch, duplicate/conflicting candidates, source artifact corrupted between opening and lookup, bucket size exceeding `policy.max_shortlist`, valid `source_exact=false` when deliberately using an unrelated MOR query, and deterministic repeat. Source tamper failure follows CONTEXT-3 code, never a new SHA-256 reimplementation.

## Qualification

`cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace` and the exact final PR head required GitHub checks must all complete successfully. The PR remains separate/stacked until #913 can be merged. Never promote a CI-green fixture to model-quality or throughput claims. Physical experiments require a separately approved RemoteOps task and valid PMU/latency protocol; none here.

Sources: `AGENTS.md`, `agent/ecosystem-roadmap` (off-main), `docs/ROADMAP.md`, `docs/SML-DACR-0-DIRECT-ASSOCIATIVE-CIRCUIT-ROUTING.md`, `docs/SML-DACR-2A-PAIRWISE-SBG-PREREGISTRATION.md`, `docs/SML-DACR-2B-VERIFIED-CANDIDATES-PREREGISTRATION.md`.
