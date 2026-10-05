# DKV-2a-N4 — combined candidate recurrent-chain replay protocol

Status: frozen before current-stack harness implementation/execution.

Parent evidence:
- `SML-DKV-2A-N3B-PAGE-SEEDED-OBSERVATION.md`;
- `SML-DKV-2A-N4-INPUT-REVIEW.md`.

The supplied source already contains Rust 1.97 results. All hypotheses below
are imported replay targets, not independent blind hypotheses.

## Arms

### Default

Current `DeltaKvSemanticProjector::new` with the base seed.

### Combined

Current
`DeltaKvSemanticProjector::new_with_ambiguity_projection(..., ActiveOnly)`
with:

`seed' = mix64(seed ^ 5 * 0x9e3779b97f4a7c15)`

for the panel's fixed one-expert PageId(5).

The existing projector then receives the same PageId(5) MOR decision as the
default arm. No core hash/bucket/sign implementation is copied.

## Frozen panel

Replay the historical source chain panel:

- SemanticState<64>;
- 16 seeds;
- K in {64,256,1024};
- G/load in {4,16};
- chain hop length L = chain index mod 4;
- random 64-bit node codes with pairwise Hamming distance >=16;
- M and R shared within one chain;
- every non-terminal node writes the next node value;
- terminal node writes itself;
- writes shuffled;
- one query from each chain start plus G novel states;
- six-cycle current DKV-2c dynamic re-key trace;
- full 64 value coordinates rewritten;
- PageId(5), one selected expert;
- controllers F1/F2/F3/F6, historical STAB, KEY, AMB, MAG and non-causal
  ORACLE;
- correct = value equals chain terminal and ambiguity is clear.

The historical STAB rule must still be checked against
`run_semantic_recurrent` for every query.

## Imported replay hypotheses

H0: default pooled F6 reproduces the source historical cells within exact
integer-count equivalence for K64/K256 at G4/G16.

H1: combined pooled F6 is no worse than default in every cell.

H2: combined pooled F6 at K1024,G16 is >=70%.

H3: combined low-load ORACLE remains below 90% pooled at K256 and K1024,G4.
This intentionally replays the source capability failure rather than trying to
turn it into a success criterion.

H4: in every combined cell, STAB accuracy equals F6 within 1 percentage point.

H5: in every combined cell, KEY, AMB and MAG do not dominate STAB under the
historical dominance definition.

H6: for combined G=4 cells, novel-final ambiguity rate remains below 70% in at
least one width, reproducing that ambiguity is not a qualified miss signal.

The report must retain per-hop L0/L1/L2/L3 ORACLE counts so the remaining
multi-hop failure shape is visible.

## Interpretation

If H1/H2 hold and H3 also holds, the combined candidate materially improves
memory capability but still does not pass the historical capability gate.

That is evidence for the projector mechanism, not for controller promotion.

The DKV-4j value-weight source result is retained as negative evidence only and
is not replayed through a new core API in this slice.

No grid or rule may change after first current-stack output.

## Promotion boundary

No core PageSeeded/combined constructor is added.

Before any core API:
- freeze multi-expert MOR semantics;
- test unbalanced page populations and desired cross-page sharing;
- replay DKV-3a exact record/subject/provenance controls.

No context/model promotion follows.
