# DKV-2a-N5 — per-hop read/rewrite fidelity protocol

Status: frozen before implementation/execution.

Parent observation:
`SML-DKV-2A-N4-CHAIN-OBSERVATION.md`.

## Question

N4 established that ActiveOnly + PageSeeded materially improves recurrent-chain
accuracy, but the historical capability gate still fails at L2/L3 and widening
K from 64 to 1024 does not remove the low-load pattern.

N5 localizes that remaining failure without proposing another projector change.

The key distinction is:

1. **teacher-forced retrieval** — given the exact true state at hop h, does
   Delta-KV return the true state at hop h+1?
2. **free-running transition** — starting from the chain root, does each
   read/rewrite remain on the true trajectory?
3. **reprojection fidelity** — after a free-running rewrite, does the resulting
   semantic state project to the same key as the exact expected next state?

This separates local associative-memory error from accumulated state/key drift.

## Candidate under test

Only the current strongest Development candidate is used:

- `DeltaKvAmbiguityProjection::ActiveOnly`;
- N3B outer PageId(5)-derived seed;
- current default semantic/MOR weights;
- current DeltaKvMemory and DeltaKvDynamicRekeyKernel;
- no new core projector API;
- no value-field weighting.

## Frozen geometry

- Rust 1.89.0;
- `SemanticState<64>`;
- K in {64,256,1024};
- G/load in {4,16};
- seeds 0..15;
- historical N4 chain generator unchanged;
- chain length = chain index mod 4;
- node values pairwise Hamming distance >=16;
- M/R shared inside a chain;
- each non-terminal node writes the next node value;
- terminal node writes itself;
- write order shuffled;
- fixed PageId(5);
- full-rate Q12 writes;
- ambiguity threshold 128.

Novel queries and controller stopping policies are out of scope. N5 is a
mechanistic fidelity probe over written chains only.

## T1 — teacher-forced one-hop retrieval

For every true node h in every written chain:

1. project the **exact true state** at h;
2. read Delta-KV directly;
3. compare the 64 read signs with the true next-node value;
4. separately record whether every coordinate has |read| >128.

Report by source hop h:

- queries;
- exact sign match;
- exact sign + clear magnitude;
- wrong coordinates;
- minimum absolute read magnitude.

Terminal h=L uses self as the target and is reported separately from advancing
hops.

Interpretation:
- failure here is local associative retrieval error;
- success here does not prove free-running recurrence.

## T2 — free-running rewrite fidelity

For each chain root, run exactly the chain's true number of advancing hops using
the current `DeltaKvDynamicRekeyKernel`.

After free-running step d, compare the produced state with the exact true node d:

- value field exact;
- ambiguity fully clear;
- complete SemanticState exact (V/M/A/R);
- Hamming distance of V from the true node.

M/R are expected to remain unchanged by the current kernel. Any complete-state
mismatch with exact V and clear A invalidates a harness assumption.

## T3 — free-running reprojection fidelity

After each free-running step d:

1. project the produced state;
2. independently project the exact true state d;
3. compare keys for exact equality;
4. record Q12 key sign Hamming distance;
5. record signed key dot product and squared norms in the research harness.

This is diagnostic geometry only.

## T4 — first divergence attribution

For each chain query, classify the first failing transition as exactly one of:

- `teacher_read_wrong`: teacher-forced read sign already wrong at the source hop;
- `teacher_read_weak`: signs are correct but at least one magnitude is <=128;
- `free_read_wrong`: teacher-forced source succeeds, but the free-running source
  state has already drifted and its read signs are wrong;
- `rewrite_ambiguous`: free-running read signs match the expected next value,
  but thresholding leaves ambiguity/non-exact rewritten state;
- `reprojection_drift`: rewritten semantic state is exact but projected key
  differs from the exact-node key; this should be impossible under a
  deterministic projector and is a fail-closed harness invariant;
- `none`: the chain remains exact through its terminal target.

A query gets one first-divergence class only.

## Frozen hypotheses

H1 — terminal self-retrieval is at least 95% exact+clear at G=4 for K256 and
K1024.

H2 — at G=4/K1024, teacher-forced advancing-hop exact+clear retrieval is at
least:
- 95% from h0;
- 90% from h1;
- 80% from h2.

H3 — at G=4/K1024, free-running exact-state fidelity is lower than or equal to
teacher-forced exact+clear fidelity at every advancing hop.

H4 — exact semantic state implies exact reprojection key in every cell. Any
violation invalidates the run.

H5 — at least 80% of failed L2/L3 chains receive a non-`none` first-divergence
classification before or at the terminal step.

H6 — if teacher-forced h1/h2 fidelity is >=90% but free-running fidelity is
<90%, accumulated recurrent drift rather than local retrieval is the leading
failure class for that cell. If the antecedent is false, report H6 as
`NOT_APPLICABLE`, not refuted.

No threshold, grid, classification rule or chain generator may change after
first execution.

## Evidence retention

CI must execute the release panel twice, require byte-identical output and print
the first output.

The observation commit may interpret the retained evidence but may not alter
this protocol or executed code after first output.

## Boundary

N5 is a diagnostic. It does not add a new projector mechanism, controller,
acceptance signal or source authority.

No context/model promotion follows.
