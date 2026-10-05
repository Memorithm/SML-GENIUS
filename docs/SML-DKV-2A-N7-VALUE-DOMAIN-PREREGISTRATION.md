# DKV-2a-N7 — within-page value-domain conditioning protocol

Status: frozen before implementation/execution.

Parent observations:
- `SML-DKV-2A-N5-HOP-FIDELITY-OBSERVATION.md`;
- `SML-DKV-2A-N6-CROSSTALK-OBSERVATION.md`.

## Motivation

N6 localized the dominant low-load L2/L3 failure to correlated writes from the
same chain. With shared M/R and changing V:

- SELF retrieval is perfect;
- count-matched cross-chain interference is perfect;
- same-chain projected-key mean cosine is about 0.67;
- same-chain writes reproduce most FULL L2/L3 failures;
- widening K does not remove the effect.

The next Development question is whether changing the **projection domain**
inside one page can break this correlation without changing Delta-KV itself.

N7 tests the most aggressive bounded diagnostic first: derive the projector
domain from the exact current V identity in addition to PageId.

This is intentionally a harness-only stress candidate because a content-derived
domain can be brittle: one wrong V bit changes the domain seed and may destroy
near-state continuity.

## Arms

Both arms use:

- current `DeltaKvAmbiguityProjection::ActiveOnly`;
- current default projection weights;
- current `DeltaKvMemory`;
- PageId(5), one selected expert;
- no provenance in the synthetic panel.

### PAGE

N3B/N4/N5/N6 control:

`page_seed = mix64(base_seed ^ page * PAGE_C)`

with `PAGE_C = 0x9e3779b97f4a7c15`.

### PAGE_VALUE

Candidate:

`page_seed = mix64(base_seed ^ page * PAGE_C)`

`domain_seed = mix64(page_seed ^ V_u64 * VALUE_C)`

with `VALUE_C = 0xd6e8feb86659fd93`.

`V_u64` is the exact 64-bit packed value field, bit i mapped to bit i.

The existing `DeltaKvSemanticProjector` is then constructed with
`domain_seed` for that state and performs all actual key projection. No
projector hashing/bucket/sign code is copied.

## Frozen fixtures

Rust 1.89.0, SemanticState<64>, 16 seeds.

### P1 — N6 low-load crosstalk replay

- K in {256,1024};
- G=4 historical chain fixture;
- terminal classes L0/L1/L2/L3;
- arms PAGE and PAGE_VALUE;
- memory subsets SELF, SELF_SAME, FULL;
- original shuffled write order.

Report exact+clear terminal recall and same-chain/cross-chain key cosine.

### P2 — teacher-forced one-hop capability

Historical chain fixture for:

- K in {256,1024};
- G in {4,16};
- every advancing true hop and every terminal self-hop.

For exact true source state h, construct the arm-specific state-derived
projector, read the full memory and compare against the true next value.

Report by source hop:

- exact signs;
- exact+clear;
- wrong coordinates;
- minimum absolute read magnitude.

### P3 — free-running chain replay

For K in {256,1024}, G in {4,16}:

1. write the historical chain facts using each fact's own arm-specific domain;
2. start from each chain root;
3. for exactly six cycles:
   - derive the projector from the current free-running state;
   - project current state;
   - read memory;
   - apply the current Delta-KV threshold rewrite to all 64 V coordinates,
     preserving M/R and updating A exactly as the current recurrent policy;
4. retain the same F1/F2/F3/F6 and non-causal ORACLE correctness definitions
   used by N4.

This loop is harness-local only because current
`DeltaKvDynamicRekeyKernel` stores one fixed projector and cannot express a
state-derived outer domain without a core API.

The harness may reimplement only the public rewrite rule:
- |read| <=128 => A=true, V unchanged;
- otherwise V=sign(read), A=false.
It may not reimplement Delta-KV memory or semantic projection.

### P4 — near-state brittleness

For every exact written non-terminal source state at G=4, K in {256,1024}:

- generate deterministic V-only perturbations at Hamming distance h in
  {1,2,4};
- preserve M/R/A and PageId;
- project/query using the perturbed state's arm-specific domain;
- compare the perturbed key with the exact source key;
- read the same full memory and test whether it still returns the exact true
  next value.

Report:
- mean absolute key cosine;
- exact-key equality;
- exact+clear next-value retrieval.

This is explicitly a robustness diagnostic, not a desired generalization test.

## Frozen hypotheses

H1 — PAGE control reproduces N6 K1024 SELF_SAME terminal clear counts:
L0=16, L1=16, L2=14, L3=8, and FULL counts L0=16, L1=16, L2=13, L3=8.

H2 — PAGE_VALUE K1024 same-chain mean absolute cosine is <=0.10 for L1/L2/L3.

H3 — PAGE_VALUE K1024 SELF_SAME terminal exact+clear recall is >=95% for
L2 and L3.

H4 — PAGE_VALUE K1024/G4 teacher-forced advancing exact+clear recall is >=90%
at source hops h0/h1/h2.

H5 — PAGE_VALUE K1024/G4 F6 free-running accuracy is >=90%.

H6 — PAGE_VALUE is brittle to a one-bit V perturbation: at K1024, h=1 mean
absolute key cosine <=0.10 and exact+clear next-value retrieval <=20%.

H7 — PAGE_VALUE exact-state projection remains deterministic: repeated
projection of the same semantic state yields byte-identical key/counters.

H8 — no exact-key collision occurs in the PAGE_VALUE written-state panel.

A failed hypothesis remains failed. H6 is a tradeoff diagnostic: if it holds,
it is negative evidence against direct promotion even if H2-H5 hold.

## Decision boundary

No core API follows directly from N7.

A full-V domain can only advance if its capability gain is weighed against P4
brittleness. If it is too brittle, the next design should seek a coarser or
transition-stable within-page domain rather than silently promoting exact-V
conditioning.

The existing gates remain:
- canonical multi-expert MOR semantics;
- unbalanced page populations/cross-page sharing;
- DKV-3a exact record/subject/provenance controls.

No controller, context or model promotion follows.
