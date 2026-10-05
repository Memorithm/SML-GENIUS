# DKV-2a-N3A — hypothesis-derived ambiguity replay protocol

Status: frozen before current-stack harness implementation/execution.

Parent evidence:
- `SML-DKV-2A-N1-CURRENT-PANEL-OBSERVATION.md`;
- `SML-DKV-2A-N2-PAGE-DENSITY-OBSERVATION.md`;
- `SML-DKV-2A-N3-INPUT-REVIEW.md`.

## Question

When A is produced by the repository's actual `derive_ambiguity` rule from
noisy competing Boolean hypotheses, does the N1/N2 ActiveOnly crossover
reproduce on current Rust 1.89?

This is a conditional noise-model replay, not a representative-state
measurement.

## Frozen grid

- `SemanticState<64>`;
- H/minority-support in {(3,1),(5,1),(5,2)};
- independent per-bit hypothesis flip probability p in
  {1%,2%,5%,10%,20%,30%,40%};
- V = strict majority of the odd-sized hypothesis set;
- A = `derive_ambiguity(..., AmbiguityPolicy { min_minority_support: m })`;
- M/R = deterministic random 64-bit fields;
- K in {256,1024};
- W=16 facts per seed;
- seeds 0..15;
- projector seed = `seed*1000+17`;
- one PageId(5);
- Bipolar versus ActiveOnly;
- full-rate Q12 writes;
- strict recall threshold |value| > 128 and correct sign.

Report per cell:

- mean A popcount;
- share of states with popcount >16;
- strict recall;
- mean key cosine.

## Imported source hypotheses

These are replay targets from the supplied diff, not blind hypotheses.

H1: for each (H,m), mean A weight is non-decreasing in p.

H2: in every cell with mean A weight <=16, ActiveOnly recall is at least
Bipolar minus 1 percentage point.

H3: in every cell with mean A weight >=40, Bipolar exceeds ActiveOnly by more
than 2 percentage points. If no such cell exists, report untested.

H4: at p=1%, every configuration has mean A weight <=4; at K=1024 ActiveOnly
recall is at least 90% and Bipolar at most 60%.

H5: every K=1024 cell with mean A weight <=16 has ActiveOnly recall >=90%.

The current-stack report must also print, for each (H,m), the largest tested p
whose mean A weight is <=16.

No grid/rule changes after first output.

## Boundary

The result may translate synthetic hypothesis disagreement into A density. It
must not be described as the distribution of real SBG states.

No projector promotion follows from this replay.
