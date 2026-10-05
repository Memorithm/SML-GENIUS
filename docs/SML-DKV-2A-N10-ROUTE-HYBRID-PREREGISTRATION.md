# DKV-2a-N10 — fixed-width multi-expert route-domain hybrid protocol

Status: frozen before implementation/execution.

Parent observation:
`SML-DKV-2A-N9-MULTIEXPERT-OBSERVATION.md`.

## Motivation

N9 leaves two valid but incompatible endpoint semantics for changes in the
selected MOR route.

PRIMARY_ANCHORED:
- preserves secondary-route continuity almost perfectly;
- preserves q=3 chain capability and Hamming-1 V robustness;
- isolates primary-page changes;
- but same-state R2/R3 conflicts reach only 50% at K=1024 and exact written-key
  collisions remain.

ORDERED_ROUTE:
- gives complete route conflict isolation and no retained exact collision;
- but R2->R3/R4 continuity is 0%.

N10 asks whether one fixed-width key can interpolate between those endpoints,
as N8 did for PAGE versus PAGE_VALUE, without changing q=3 itself.

No core API is introduced.

## Endpoint keys

For every state and complete MorDecision, construct the two N9 q=3 keys:

1. PRIMARY:
   - outer route seed depends only on the first selected expert;
   - q=3/4 PAGE_VALUE + 1/4 PAGE;
   - complete MorDecision is still passed into both current projectors.

2. ORDERED:
   - outer route seed is conditioned on the complete ordered selected-expert
     tuple using the frozen N9 route constants;
   - same q=3 construction;
   - same complete MorDecision.

## N10 route hybrid

For route-domain fraction numerator r in {0,1,2,3,4}, construct one
K-dimensional key:

- coordinate i comes from ORDERED when `i % 4 < r`;
- otherwise it comes from PRIMARY.

Therefore:
- r=0 reproduces PRIMARY_ANCHORED;
- r=4 reproduces ORDERED_ROUTE;
- total final key width remains exactly K.

No renormalization is applied after stitching. DeltaKvMemory retains its current
normalization.

For matched logical-work accounting, the Development harness computes both
endpoint q=3 keys for every N10 key at every r. It therefore evaluates four
current source projections per hybrid key:
- PRIMARY/PAGE;
- PRIMARY/PAGE_VALUE;
- ORDERED/PAGE;
- ORDERED/PAGE_VALUE.

This matched-work rule is intentionally more expensive than a possible future
specialized endpoint implementation.

## Frozen routes

Retain the N9 routes:

- R1=[5];
- R2=[5,7];
- R3=[5,8];
- R4=[5,7,9];
- R5=[5,9,7];
- R6=[7,5].

R2/R3/R4/R5 share primary PageId(5); R6 switches primary.

## Frozen grid

- Rust 1.89.0;
- SemanticState<64>;
- q=3 fixed;
- r in {0,1,2,3,4};
- K in {256,1024};
- 16 deterministic seeds;
- current ActiveOnly source projectors;
- current default semantic/MOR weights;
- full MorDecision passed to every source projection;
- full-rate Q12 writes;
- clear threshold 128;
- no provenance in this mechanism panel.

## P0 — endpoint compatibility and determinism

For r=0 and r=4:
- reproduce retained N9 PRIMARY_ANCHORED and ORDERED_ROUTE keys exactly;
- singleton R1 remains N8 q=3 compatible;
- repeated projection is byte-identical;
- complete selected-expert evidence is preserved;
- duplicate selected experts remain rejected.

## P1 — secondary-route geometry

For 200 random states per seed and K, compare R2 against R3 and R4.

For every r report:
- exact-key equality;
- mean/max absolute cosine.

Also retain R2->R6 primary-switch geometry.

## P2 — secondary continuity

Write deterministic state/value facts under R2.

Query the same states under:
- R2;
- R3;
- R4.

Report exact+clear recovery for every r.

## P3 — route conflict isolation

Secondary conflict:
- same state receives independent values under R2 and R3;
- write both in shuffled order;
- read both route-specific targets.

Primary-switch conflict:
- same state receives independent values under R2 and R6.

For every r report:
- exact+clear recall;
- exact written-key collisions.

## P4 — two-expert recurrent-chain capability

Replay the N9 fixed R2=[5,7] historical chain panel for every r:

- K in {256,1024};
- G in {4,16};
- 16 seeds;
- teacher-forced advancing reads;
- six-cycle F1/F2/F3/F6/ORACLE.

The full R2 decision is passed to every source projector.

## P5 — Hamming-1 V robustness

Repeat the N9 one-bit V perturbation panel under fixed R2 for every r.

Report:
- exact+clear next-value retrieval;
- mean absolute cosine to the exact written-source key;
- exact-key equality.

## P6 — logical work and written-state collisions

For every K/r report:
- N10 hybrid-key constructions;
- semantic-coordinate reads;
- bucket accumulations;
- quantized coordinates emitted by all four current source projections;
- final key width.

Logical source-projection counters must be identical across r for fixed K and
construction count.

Retain exact written-key collisions separately for continuity, conflict and
chain cells.

## Frozen hypotheses

H1 — endpoint compatibility: r=0 reproduces N9 PRIMARY and r=4 reproduces N9
ORDERED at retained K=1024 controls.

H2 — secondary-route mean |cosine| for R2->R3 and R2->R4 is non-increasing
with r at K=1024.

H3 — secondary continuity R2->R3 and R2->R4 is non-increasing with r at
K=1024.

H4 — secondary same-state R2/R3 conflict recall is non-decreasing with r at
K=1024.

H5 — at least one interior r in {1,2,3} at K=1024 satisfies all route gates:
- R2/R3 conflict recall >=85%;
- R2/R3 continuity >=20%;
- R2/R4 continuity >=20%;
- zero exact R2/R3 written-key collisions;
- R2/R6 primary-switch conflict recall >=85%;
- R2/R6 mean |cosine| <=0.10.

H6 — at least one interior r satisfying H5 also satisfies all capability gates:
- K1024/G4 F6 >=90%;
- K1024/G16 F6 >=85%;
- K1024/G4 teacher-forced advancing exact+clear >=90% at h0/h1/h2;
- K1024 Hamming-1 exact+clear recovery >=20%.

H7 — no exact written-key collision occurs in the selected candidate's
continuity or recurrent-chain cells.

H8 — logical source-projection work counters are identical across r for fixed K
and hybrid-key construction count.

A failed hypothesis remains failed. Do not change fractions, thresholds,
constants or route fixtures after first execution.

## Selection rule

If one or more interior r values satisfy both H5 and H6, select the **smallest
r**. This preserves the largest PRIMARY_ANCHORED coordinate share while meeting
the frozen secondary-conflict isolation gate.

If no interior r satisfies both gates, report **no route-hybrid candidate**.

ORDERED_ROUTE and PRIMARY_ANCHORED remain endpoint controls and cannot be
selected by this rule.

## Boundary

N10 remains harness-only.

Even if one route hybrid is selected, core promotion still requires:
- unbalanced page-population / cross-page sharing controls;
- DKV-3a exact record/subject/provenance requalification;
- representative recurrent SBG/MOR decisions;
- physical latency/RSS/traffic/energy qualification.

No controller, context or model promotion follows.
