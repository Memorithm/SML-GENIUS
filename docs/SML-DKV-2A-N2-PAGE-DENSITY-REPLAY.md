# DKV-2a-N2 — page discrimination and ambiguity-density replay

Status: current-stack replay protocol frozen before implementation/execution.

Parent evidence:
- `SML-DKV-2A-N1-AMBIGUITY-NEUTRAL-PREREGISTRATION.md`;
- `SML-DKV-2A-N1-CURRENT-PANEL-OBSERVATION.md`.

## Owner-supplied input

Input: `sml-dkv-0005-dkv-4e.patch`.

- bytes: 42,511;
- lines: 761;
- SHA-256: `6d8d60b570f7a8fbefa0089b01eb4633625f08bbd23aa18aaef4e71fb5f5d71f`.

The supplied patch contains a Rust 1.97 requalification harness and its
observation. It introduces no mechanism that is still missing from current
`main`: its `SparseA` fold is semantically the already-merged
`DeltaKvAmbiguityProjection::ActiveOnly`.

The useful new questions are instead:

1. how ActiveOnly behaves as ambiguity density moves from 0 to 64 active bits;
2. whether the current MOR PageId contribution materially separates otherwise
   identical state/value memories.

Because the supplied patch already contains outcomes, the hypotheses below are
**imported source hypotheses**, not an independently blind preregistration.
This document freezes their current-stack Rust 1.89 replay before the adapted
harness is implemented or executed.

## Library boundary

No core projector change is permitted in this slice.

Both arms must call the current library directly:

- `DeltaKvSemanticProjector::new(...)` / explicit `Bipolar`;
- `DeltaKvSemanticProjector::new_with_ambiguity_projection(..., ActiveOnly)`.

The existing default remains Bipolar. Source/provenance authority, semantic
record identity and DKV-4 acceptance rules are unchanged.

## Frozen environment

- Rust 1.89.0;
- `SemanticState<64>`;
- key widths K = 256 and 1024;
- seeds 0..15;
- projector seed = `seed * 1000 + 17`;
- semantic weight = 1;
- MOR weight = 8;
- pages 1..=8;
- value width = 64;
- full-rate Q12 writes;
- strict recall requires every output coordinate to have absolute magnitude
  >128 and the correct sign.

## Contract checks

The replay is invalid if any contract fails.

C1. Legacy constructor and explicit Bipolar return identical complete evidence.

C2. Repeating the same projection under either mode returns identical keys and
counters.

C3. With A all ones, Bipolar and ActiveOnly keys are equal.

C4. Every `SemanticState<64>` projection reads exactly 256 semantic
coordinates.

C5. With one selected MOR expert, Bipolar performs 257 bucket accumulations;
ActiveOnly performs `193 + popcount(A)`.

C6. ActiveOnly must not produce `ZeroProjectedKey` over 3,200 deterministic
zero-ambiguity states per K.

No harness-local reimplementation of the projection hash, bucket or sign rule
is allowed.

## E1 — page diversity

For each K, seed and W in {8,16,32}:

- generate W random zero-ambiguity states;
- assign one PageId uniformly from 1..=8;
- assign one independent 64-bit target value;
- shuffle write order;
- write each (state,page)->value;
- read the same state under:
  - its own page;
  - a deterministically chosen different page among the other seven.

Report own-page and wrong-page strict recall for Bipolar and ActiveOnly.

This tests key discrimination, not source authority. A wrong-page successful
read is evidence that the page component is weak relative to the rest of the
projected key.

## E2 — page conflict

For each K, seed and W' in {4,8}:

- generate W' random zero-ambiguity states;
- for each state choose two distinct pages;
- assign an independent target value to each page;
- write both pairs in shuffled order;
- read both page-specific values.

Report strict-correct recall over the 2W' reads.

If two different page values cannot coexist, page identity is not sufficiently
isolating for this use even if state-level recall is high.

## E3 — ambiguity-density crossover

K=1024, W=16 only.

For n in {0,1,2,4,8,16,32,64}, generate states with exactly n active ambiguity
bits at distinct random positions. Compare Bipolar and ActiveOnly on:

- strict recall;
- mean pair cosine;
- projection counters.

The remaining V/M/R fields and target stream are otherwise matched.

## Imported hypotheses to replay

These hypotheses come from the supplied patch and are retained unchanged for
the current-stack replay.

H1 — page diversity: in every E1 cell, ActiveOnly own-page strict recall is at
least Bipolar minus 2 percentage points.

H2 — weak page discrimination: at K=1024, W=8, wrong-page strict recall is at
least 0.8 times own-page strict recall in both arms.

H3 — page conflict unresolved: at K=1024, W'=8, E2 strict-correct recall is at
most 60% in both arms.

H4 — no E2 regression: in every E2 cell, ActiveOnly is at least Bipolar minus
2 percentage points.

H5 — density crossover: ActiveOnly is at least Bipolar minus 1 percentage point
for n in {0,1,2,4,8}, is below Bipolar at n=32, and equals Bipolar at n=64.

A failed hypothesis remains failed. No grid, threshold or rule may change after
the current-stack run.

## Evidence retention

The CI evidence step must execute the adapted panel twice in release mode,
require byte-identical output, and print the retained first output.

The observation commit may record the exact current-stack output and interpret
it, but may not edit this protocol or the executed panel after first output.

## Decision boundary

Even if all imported hypotheses reproduce, ActiveOnly remains non-default until
representative SBG ambiguity density is measured.

If H2 or H3 reproduce, the MOR page discriminator becomes a separate DKV-2a
design problem shared by Bipolar and ActiveOnly; it must not be hidden by
projector promotion.

No context/model promotion follows from this replay.
