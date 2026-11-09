# SML-DACR-2A — frozen paired SBG discrete-address experiment

**2026-11-09 — frozen BEFORE executable implementation.** This is a *structural research fixture*, not a trained language-model retrieval or quality claim. Track: DACR-2; separate from source-exact DACR-1, CONTEXT-3, CONTEXT-7/LONGCTX-B1 protected TEST and native MOR/ANF. Native SML model semantics and default inference unchanged.

## Scientific question and refutable hypothesis

Can supervision from **same/different meaning** pairs select discrete SBG coordinates that address context candidates on held-out nuisance-bit combinations, with fewer logical candidate comparisons than a full scan? This is not semantic natural-language generalization, not new-context knowledge extraction and not evidence that the learner trains from downstream next-token loss. Source verification stays with CONTEXT-0/1c/DACR-1. Latent lookups return *candidate IDs only*.

## Fixed task and splits

Width `BITS=16`, but admitted feature positions only 0..7. Semantic class has exactly three bits at positions `[4,6,7]`; nuisance bits occupy `[0,1,2,3,5]`. Eight semantic classes `0..7`. `TRAIN`: for each class, one positive (same-class) pair with nuisance masks 0 and 31; for each class and each of three class bits, one negative (different-class) pair whose class ID differs in exactly that bit, with identical nuisance mask 10 on both sides. Total 8 positive and 24 negative pairs, 32 pairs. `TEST`: for each class, source address computed on a nuisance pattern `0b01001` and query on `0b10110`. Neither nuisance combination is used in TRAIN. All eight semantic class identities appear in TRAIN, therefore this tests *nuisance invariance only*, not unseen semantic classes or language.

The fixed control selects positions `[0,1,2]`. The learned candidate selects three coordinates by a deterministic pairwise disagreement score: for each observed-known coordinate, `score = (# negative pairs with differing bit) - (# positive pairs with differing bit)`; tie break by ascending coordinate. Require both positive and negative examples, positive selected scores and at least three eligible coordinates; otherwise fail. Training pairs convey a pairwise task relation, not target address bits or expert page labels. Freeze selected coordinates before testing; no retuning on TEST.

Every candidate is indexed by the **same** source IDs and the hard discrete code computed from its SBG values at selected coordinates. At query, only the matching exact code bucket is read, with caps that reject overfilled buckets before mutation. No in-query history scan, no Hamming ranking or softmax. Unknown SBG mask on a selected feature -> reject (never silently treat unknown as false). Distinct sources with same code are candidate collisions, not a verified source match. Source verification, authorization, disambiguation and causal model integration are explicitly out of scope.

## Hard guards

- `1 <= BITS <= 64`; `1 <= address_bits <= min(8,BITS)`; `max_pairs`, `max_entries`, `max_bucket_entries` positive and capped by caller.
- Stream ID and nonzero index epoch checked on **every** lookup; cross-stream source insert rejected; duplicate source binding rejected.
- Unknown masks must be respected in both learning and addressing; incomplete selected features fail closed.
- Index insertion with full/budget violation must not alter prior entries.
- Return accounting `address_buckets_probed` and `candidates_returned`, without pretending it represents physical DRAM traffic or latency.
- No fake exact-source claim from a same-code result.

## Test requirements (implementation gate)

1. Frozen task learned positions exactly `[4,6,7]`; eight out of eight held-out nuisance-variant classes retrieve the corresponding ID, fixed `[0,1,2]` control obtains fewer than eight.
2. Positive/negative support failures, invalid budgets and zero-width/oversized width errors.
3. Unknown mask refusal for a selected bit (including unknown-but-value-true to ensure no unknown->false coercion).
4. Collision returns multiple candidate IDs without asserting semantic or source correctness; bounded bucket admission failure and unchanged prior state.
5. Duplicate segment, cross-stream, stale-epoch and wrong-stream retrieval fail closed.
6. Determinism across two trainings and repeated queries; zero history score/candidate work for missing exact address beyond bucket lookup.
7. No mutation or additional model residency at query time beyond a copied bounded candidate list.

Use `cargo test --locked -p sml-store dacr_latent`, `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace`; the exact final SHA must pass mandatory CI before merge. No self-hosted physical run without RemoteOps admission.

## Reporting / interpretation

A positive result validates only this synthetic supervised feature-selection fixture. Report learned/fixed TEST hit count (not source truth), matched bucket costs, collisions and false candidates. A negative result is retained. The next scientifically meaningful stage is unseen natural-language paraphrases and learned end-to-end task loss, independently frozen corpora and CPU hardware counters; no 1M context, 25B, inference-quality, latency, throughput or novelty claim is implied.

Authority: `AGENTS.md`, `agent/ecosystem-roadmap` off-main, `docs/SML-DACR-0-DIRECT-ASSOCIATIVE-CIRCUIT-ROUTING.md`, `docs/SML-DACR-1-EXACT-SOURCE-PREREGISTRATION.md`, `docs/SML-0.6-LATENT-DIRECT-ROUTING.md`.
