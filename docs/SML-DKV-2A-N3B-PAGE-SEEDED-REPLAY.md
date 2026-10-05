# DKV-2a-N3B — page-conditioned projector replay protocol

Status: frozen before current-stack harness implementation/execution.

Parent evidence:
- `SML-DKV-2A-N2-PAGE-DENSITY-OBSERVATION.md`;
- `SML-DKV-2A-N3-INPUT-REVIEW.md`.

## Question

Can page-sensitive Delta-KV memories be isolated by conditioning the entire
projection hash family on the selected PageId, rather than adding a sparse page
term to an otherwise shared state key?

## Candidate

Single-page `PageSeeded` is harness-only.

Given base projector seed `s` and selected page `p`, derive:

`page_seed = mix64(s ^ p * 0x9e3779b97f4a7c15)`.

Construct the existing current library projector with:

- `DeltaKvAmbiguityProjection::ActiveOnly`;
- default semantic/MOR weights;
- `page_seed`;
- the original one-page `MorDecision`.

No library hash/bucket/sign logic is copied.

Control `Additive` is the current ActiveOnly projector with the unmodified
base seed.

The harness may define `mix64` only for deriving the outer seed, not for
reimplementing projection internals.

## Frozen panel

- Rust 1.89;
- `SemanticState<64>`, zero ambiguity;
- K in {256,1024};
- seeds 0..15;
- base projector seed `seed*1000+17`;
- PageId uniformly in 1..=8;
- one selected expert per decision;
- full-rate Q12 writes;
- strict recall as N2.

E1, W=16:
- write 16 random (state,page,value) facts;
- read own page;
- read a deterministically selected different page.

E2, W'=8:
- generate 8 states;
- each state receives two distinct pages and independent values;
- write all 16 pairs shuffled;
- read both page-specific values.

E3:
- 200 pairs per seed;
- mean cosine for independent states under the same page;
- mean cosine for the same state under two distinct pages.

## Imported source hypotheses

H0: Additive reproduces N2 at K=1024: own 98.4%, wrong 98.4%, E2 49.6%
within retained integer-count equivalence.

H1: PageSeeded wrong-page strict recall <=5% at both widths.

H2: PageSeeded E2 strict recall >=85% at both widths.

H3: PageSeeded own-page recall is within 3 percentage points of Additive at
both widths. A result better by more than 3 points refutes the two-sided wording
rather than being silently relabelled a hold.

H4: PageSeeded absolute cross-page cosine <=0.1 and same-page independent-state
cosine <=0.05 at both widths.

## Required accounting

Report projector counters for both arms. The page-conditioned arm must have
the same semantic-coordinate reads, bucket accumulations and quantized
coordinates as Additive for a matched state/page. Seed conditioning is not
counted as free in later physical-cost work, but this logical panel only records
the existing projector counters.

## Boundary

Page isolation is a semantic choice. This panel does not establish that facts
should never generalize/share across pages.

Multi-expert MOR decisions are out of scope. No core API can be proposed until
a separately frozen rule for multiple selected pages exists and DKV-3a
record/subject alias controls are replayed.

No context/model promotion follows.
