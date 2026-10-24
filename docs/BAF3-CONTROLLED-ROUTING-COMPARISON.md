# BAF-3 — Controlled matched-write comparison harness

Research-only; **no claim that BAF improves model quality, compute, or speed**.

## Purpose

Make BAF-2 falsifiable against non-Transformer controls by evaluating the *same page atlas*, *same final transition supervision* and *same write masks/priority semantics*, while varying the **router / ANF function class**. The harness reports all splits rather than hiding train or held-out performance.

Command:

```sh
cargo run --release -p sml-core --example baf_focus_compare
cargo test --locked -p sml-core --example baf_focus_compare
```

CSV fields: `model,split,cases,error_bits,selected_pages,train_candidate_evaluations,admitted`. Each value is deterministic under a pinned source revision; no wall-clock claims. `selected_pages` is the aggregate selected count on that panel, not proof of lower inference cost.

Five methods:

- **inactive**: no page gate is active.
- **unconditional**: the two writer pages and two inert distractor pages all activate. The BAF-0 capacity cap selects the highest-priority two.
- **anf_degree1**: two editable pages are jointly fitted with degree-1 ANF predicates.
- **anf_degree2**: identical fit/training budget but the feature class additionally permits degree-2 monomials. No Q/K/V or similarity scores.
- **hamming_key_router**: a **router-only** control using the HLE-0 style signature `event XOR state` and top-k Hamming distance to four independently trainable 4-bit page keys. Search uses the same train-only final-state error and final DEV veto.

**Important scientific qualification:** `hamming_key_router` is *not* the full `hybrid_logic_engine`. The latter executes learned `HardLogicNetwork` pages and XOR writes, whereas this harness deliberately executes the same fixed masked writes as BAF to isolate routing differences. Never label it an end-to-end HLE superiority result. Likewise this run does not implement a matched MOR control; that remains a separate integration project requiring equivalent page contracts and budgets.

## Data isolation

The deterministic synthetic generator produces four panels, 16 transitions each:

- TRAIN: event bits [1:0] and state bits [1:0] fully varied, nuisance event bit 2 = 0.
- VALIDATION: held-out nuisance event bit 2 = 1 with otherwise identical low bits.
- TEST: initial state bit 2 is preset; nuisance event bit 2 = 0.
- OOD: nuisance event bit 2 = 1 and state bit 3 is preset.

All four panels have distinct IDs **and** distinct exact `(event,state)` pairs. TRAIN and VALIDATION are passed to the learners. TEST and OOD are passed only to post-fit measurement. The fitter never receives the target generator's equations, hidden page choices or per-page activation labels. The generator is deterministic and public, however; this is a unit-scale generalization probe, not a blind real-world evaluation.

The target transition is a 4-bit state rewrite with two independent Boolean conjunctive activation conditions:

```text
next_state = state
    OR ( (event[0] AND state[0]) << 2 )
    OR ( (event[1] AND state[1]) << 3 )
```

These target rules are intentionally simple and only one step long. They favor degree-2 ANF expressivity by construction; this tests **expressive capability**, not universal superiority or learned natural-language ability. In TEST/OOD, some output bits are already set in the initial state, reducing observability; report this when interpreting performance.

## Budget and validity caveats

- BAF-2 default train search: up to 8 coordinate rounds, 2 editable pages, degree up to 1/2, fixed maximum candidate bank and evaluation cap. Candidate counts are emitted.
- Hamming-key control uses 4 page keys, each 4 bits, a fixed top-k cap of 2 and at most 8 greedy rounds with all 16 keys per coordinate. Its evaluation count is emitted separately; **candidate budgets and hypothesis-class cardinalities are not identical**. No compute-normalized efficiency claim follows.
- Every control has the same four page payloads/IDs and deterministic write order, but unconditional/BAF routing may choose fewer than k pages; Hamming always chooses exactly k. Router sparsity is reported explicitly.
- Exact stored bytes, heap capacity, RSS, latency, GPU throughput, and joules are **not measured** by this harness. Raw selected page counts are not a proxy for those metrics.
- No statistical uncertainty from seeds is calculated. Do not interpret a 16-example panel as a statistically powered sample.
- Validation is only an admission veto for trained controls; it is not used to select features during search. No validation-guided retry is permitted.

## Graduation requirements

1. Pin multi-seed, disjoint-task data generators with hard negatives, XOR/parity targets, temporal delayed recall and controlled interference.
2. Measure out-of-distribution generalization on genuinely independent sequences, with train/DEV/TEST protocols frozen first.
3. Implement a separately qualified MOR/ANF and **actual HLE-0 hard-page** baseline at matched total training operations/bytes.
4. Report medians, quantiles, errors, page activity, wall-clock, RSS, retained model bytes, and energy only if measured on known hardware with provenance.
5. Admit production integration only after exact-head CI and independent scientific gates.

## Limits of the technique

BAF-2 greedy coefficient flipping can be stuck when a useful combination of monomials offers no strict improvement until multiple flips are applied. Such negative results must be reported without attempting to retune on TEST. No Transformer-like attention layer is introduced.
