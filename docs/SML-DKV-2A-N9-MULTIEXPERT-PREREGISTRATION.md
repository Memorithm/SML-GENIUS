# DKV-2a-N9 — canonical multi-expert domain semantics

Status: frozen before implementation/execution.

Parent observation:
`SML-DKV-2A-N8-HYBRID-OBSERVATION.md`.

## Motivation

N8 selects a fixed-width q=3/4 PAGE_VALUE + 1/4 PAGE hybrid on the synthetic
single-expert chain fixture.

Before any core domain API, the candidate needs a deterministic meaning when
MOR selects more than one expert.

Current MOR already gives `selected_experts` an ordered meaning:

- experts are ranked by descending final score;
- PageId is the deterministic tie break;
- `select_experts` preserves that ranked order;
- the current Delta-KV projector also folds PageId together with its rank.

N9 therefore treats order as intentional MOR semantics, not incidental vector
ordering.

## Candidate: PRIMARY_ANCHORED

The outer domain is anchored only on the first selected expert:

`route_seed = mix64(base_seed ^ primary_page * PAGE_C)`.

The current q=3 hybrid is then constructed exactly as N8:

- 1/4 coordinates from PAGE using `route_seed`;
- 3/4 coordinates from PAGE_VALUE using
  `mix64(route_seed ^ V_u64 * VALUE_C)`.

Crucially, both current library projectors still receive the **complete
MorDecision**. Secondary experts and their ranks therefore remain encoded by the
existing MOR bucket contributions. They are not discarded; they simply do not
select an entirely different hash family.

For a singleton decision, PRIMARY_ANCHORED is byte-for-byte the selected N8
q=3 construction.

## Control: ORDERED_ROUTE

A full-route control derives an outer seed from the complete ranked expert
tuple.

For selected pages `p[0..n]`:

1. start from the same primary page seed;
2. for every secondary rank r>=1:
   `seed = mix64(seed ^ ROUTE_TAG ^ p[r]*ROUTE_C ^ r*RANK_C)`;
3. if n>1:
   `seed = mix64(seed ^ n*LEN_C)`.

Frozen constants:

- `PAGE_C = 0x9e3779b97f4a7c15`;
- `VALUE_C = 0xd6e8feb86659fd93`;
- `ROUTE_TAG = 0x6d756c74695f6d6f`;
- `ROUTE_C = 0xc2b2ae3d27d4eb4f`;
- `RANK_C = 0x165667b19e3779f9`;
- `LEN_C = 0x85ebca77c2b2ae63`.

ORDERED_ROUTE also uses the same q=3 hybrid and passes the complete MorDecision
to the current projector.

Singleton ORDERED_ROUTE is defined to equal PRIMARY_ANCHORED/N8 exactly.

The control is not assumed to be better. It tests the cost of turning every
change in the selected route into a new projection domain.

## Frozen decisions

Use these deterministic selected-expert routes:

- R1 = [5];
- R2 = [5,7];
- R3 = [5,8];
- R4 = [5,7,9];
- R5 = [5,9,7];
- R6 = [7,5].

R2/R3/R4/R5 share primary PageId(5). R6 switches the primary.

Rank order is significant. R4 and R5 intentionally contain the same expert set
with different secondary rank order.

## Frozen grid

- Rust 1.89.0;
- SemanticState<64>;
- q=3 only;
- K in {256,1024};
- 16 deterministic seeds;
- current ActiveOnly projector;
- current default semantic/MOR weights;
- no provenance in the mechanism panel;
- full-rate Q12 writes;
- clear threshold 128.

## P0 — compatibility and determinism

For both domain policies:

- R1 keys/evidence must reproduce N8 q=3 singleton keys exactly;
- repeated projections are identical;
- current projector evidence retains the full selected-expert vector;
- duplicate selected experts remain rejected by the current projector.

## P1 — route-change geometry

For 200 random states per seed and K, compare R2 against:

- R3: same primary, secondary replacement;
- R4: same primary, secondary addition;
- R5: same primary/set expansion with secondary rank change;
- R6: primary switch.

Report exact-key equality and mean/max absolute cosine.

## P2 — continuity of an existing memory across secondary-route drift

For each state/value written under R2, query the same state under R2, R3 and R4.

Report exact+clear recovery.

This asks whether changing only secondary experts preserves access to an
otherwise identical semantic fact.

## P3 — same-state route conflict isolation

For each state, assign independent target values under:

- R2 = [5,7];
- R3 = [5,8].

Write both in shuffled order, then read both route-specific values.

Report exact+clear recall.

This is a diagnostic tradeoff. It does not assert that secondary-route changes
must define independent memories.

A separate primary-switch conflict writes independent targets under R2 and R6
and reports recall.

## P4 — two-expert recurrent-chain capability

Replay the N8 q=3 historical chain panel using fixed R2=[5,7]:

- K in {256,1024};
- G in {4,16};
- 16 seeds;
- teacher-forced advancing reads;
- six-cycle F1/F2/F3/F6/ORACLE.

Both domain policies receive the complete R2 decision.

## P5 — local V robustness

Repeat the N8 Hamming-1 V perturbation control under fixed R2 for both domain
policies.

Report exact+clear next-value recovery and mean absolute cosine.

## Frozen hypotheses

H1 — singleton compatibility: both policies reproduce N8 q=3 exactly.

H2 — PRIMARY_ANCHORED secondary continuity: at K=1024, R2->R3 and R2->R4
mean |cosine| >=0.15 and exact+clear continuity retrieval >=20%.

H3 — primary-switch separation: under PRIMARY_ANCHORED, R2->R6 mean |cosine|
<=0.10 and primary-switch conflict recall >=85%.

H4 — ORDERED_ROUTE secondary isolation: R2/R3 same-state conflict recall >=85%.

H5 — ORDERED_ROUTE secondary brittleness: R2->R3 continuity retrieval <=10%.
This is negative evidence against using the full route tuple as a transparent
continuation domain.

H6 — q=3 multi-expert chain capability: PRIMARY_ANCHORED K1024 F6 is >=90% at
G=4 and >=85% at G=16.

H7 — PRIMARY_ANCHORED teacher-forced advancing recall at K1024/G4 is >=90% for
h0/h1/h2.

H8 — Hamming-1 V robustness under PRIMARY_ANCHORED remains >=20% exact+clear at
K1024.

H9 — no exact written-key collision occurs in any P2/P3/P4 cell.

## Semantic selection rule

If H1, H2, H3, H6, H7, H8 and H9 hold, PRIMARY_ANCHORED is retained as the
canonical multi-expert **Development domain semantics**.

ORDERED_ROUTE is retained only as a control. H4/H5 describe its
isolation/continuity tradeoff and do not override the PRIMARY_ANCHORED selection
rule.

If the PRIMARY_ANCHORED gate fails, report no canonical multi-expert candidate.
Do not retune constants or thresholds after execution.

## Boundary

Selecting a Development domain semantics is not a core API promotion.

Still required afterwards:

- unbalanced page-population / cross-page sharing controls;
- DKV-3a exact record/subject/provenance replay;
- representative recurrent SBG/MOR decisions;
- physical cost qualification.

No controller, context or model promotion follows.
