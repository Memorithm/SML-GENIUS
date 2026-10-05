# DKV-2a-N11 — two-bank PRIMARY / ORDERED resolver protocol

Status: frozen before implementation/execution.

Parent observations:
- `SML-DKV-2A-N9-MULTIEXPERT-OBSERVATION.md`;
- `SML-DKV-2A-N10-ROUTE-HYBRID-OBSERVATION.md`.

## Motivation

N9/N10 show that one fixed-width key cannot simultaneously provide both:

1. transparent continuity across secondary MOR-route drift; and
2. strong isolation for independent route-specific values.

PRIMARY_ANCHORED preserves continuity but secondary conflicts remain coupled.
ORDERED_ROUTE isolates conflicts but makes secondary-route drift effectively a
different address.

N11 separates those semantic roles into two independent associative banks.

## Fixed q=3 endpoint keys

Both banks retain the selected N8 q=3 construction:

- 3/4 PAGE_VALUE coordinates;
- 1/4 PAGE coordinates;
- current ActiveOnly source projectors;
- complete MorDecision passed to every source projector.

The two outer route-domain policies are exactly the retained N9 endpoints:

### PRIMARY bank

Outer seed depends only on the first selected expert.

Purpose: shared retrieval across secondary-route drift.

### ORDERED bank

Outer seed depends on the complete ranked selected-expert tuple.

Purpose: explicit route-specific override/conflict isolation.

No source-projector, DeltaKvMemory or MOR implementation is changed.

## Explicit override directory

N11 does **not** use read magnitude, cosine, agreement or a successful
associative read as evidence that an override exists.

The harness carries an explicit bounded directory:

`(semantic_record_id, ordered_route_fingerprint) -> override_present`.

The synthetic panel assigns a deterministic semantic_record_id to every source
fact. This sidecar is authoritative test metadata only; it is not stored in the
Q12 matrix and does not participate in the projected key.

Resolver:

1. if the exact (record_id, ordered route) binding exists, query ORDERED;
2. otherwise query PRIMARY.

No fallback from a failed ORDERED read to PRIMARY is allowed after a declared
override. A declared override that fails is an override failure, not a reason to
reinterpret retrieval strength as absence.

This isolates resolver semantics. DKV-3a remains responsible for later
qualification of real record/subject/provenance binding.

## Frozen routes

Retain N9/N10 routes:

- R1=[5];
- R2=[5,7];
- R3=[5,8];
- R4=[5,7,9];
- R5=[5,9,7];
- R6=[7,5].

## Frozen grid

- Rust 1.89.0;
- SemanticState<64>;
- q=3 fixed;
- K in {256,1024};
- 16 deterministic seeds;
- full-rate Q12 writes;
- clear threshold 128;
- no provenance in this mechanism panel.

## P0 — endpoint compatibility

For identical states/routes, N11 PRIMARY and ORDERED keys must reproduce the
retained N9 endpoint keys exactly.

Repeated projections must be byte-identical and retain the complete
selected-expert evidence.

Duplicate selected experts remain rejected.

## P1 — transparent secondary continuity

For 256 deterministic state/value facts per K:

- assign unique record_id;
- write the value to PRIMARY under R2;
- do **not** create any override binding;
- query the same record/state under R2, R3 and R4.

The resolver must choose PRIMARY for all three routes.

Report:
- exact+clear recall by query route;
- selected bank counts;
- false override count.

Also compute the ORDERED read counterfactually for R3/R4 and report how often
it would have been exact+clear. This is diagnostic only and must not affect
resolution.

## P2 — explicit secondary override

For every deterministic record/state:

- write base target A to PRIMARY under R2;
- write independent target B to ORDERED under R3;
- create the exact (record_id,R3) override binding.

Queries:
- R2 must return A from PRIMARY;
- R3 must return B from ORDERED;
- R4 has no binding and must return A from PRIMARY.

Report exact+clear recall and selected-bank counts.

This panel measures whether continuity and conflict isolation can coexist when
their semantic roles are represented separately.

## P3 — primary-switch override

Repeat P2 with ORDERED override route R6=[7,5].

R2 query must resolve PRIMARY A; R6 must resolve ORDERED B.

## P4 — stale/foreign override guard

Construct deterministic negative metadata controls:

- same state and route R3 but wrong record_id;
- same record_id but route R4;
- valid override record_id/route paired with a different state.

The directory must select ORDERED **only** for the exact bound
(record_id,route) pair.

Report false override selections and resulting false acceptances separately.

No associative signal may create a binding.

## P5 — two-expert recurrent-chain capability

Replay the historical chain panel under fixed R2=[5,7]:

- K in {256,1024};
- G in {4,16};
- 16 seeds;
- every chain fact is a shared PRIMARY fact;
- override directory is empty;
- teacher-forced advancing reads;
- six-cycle F1/F2/F3/F6/ORACLE.

This must preserve the N9/N10 q=3 PRIMARY capability because secondary override
semantics are not needed for the fixed-route chain.

## P6 — route-specific recurrent override

At G=4 only, deterministically select one advancing transition per L2/L3 chain:

- base next value A is written to PRIMARY;
- independent override B is written to ORDERED under R3;
- exact override binding is installed for that record/R3.

Teacher-forced queries under R2 must retrieve A and under R3 must retrieve B.

This tests route-specific transition composition without a free-running route
change policy.

## P7 — local V robustness

Repeat the N9/N10 Hamming-1 V perturbation panel under fixed R2 with an empty
override directory.

Resolver must select PRIMARY. Report exact+clear next-value recovery and mean
absolute cosine.

## P8 — logical cost

Report separately:

- PRIMARY key constructions;
- ORDERED key constructions;
- PRIMARY reads/updates;
- ORDERED reads/updates;
- directory probes;
- final key width;
- semantic-coordinate reads and bucket accumulations.

A resolver lookup must require only one bank read after the directory decision.
Counterfactual reads in P1 are reported separately and excluded from resolver
cost.

## Frozen hypotheses

H1 — endpoint compatibility: N11 PRIMARY/ORDERED reproduce N9 endpoints exactly.

H2 — secondary continuity at K=1024: P1 R2/R3/R4 resolved exact+clear recall is
>=95%, with zero false override selections.

H3 — secondary override at K=1024: P2 R2/R3/R4 resolved exact+clear recall is
>=95% for all three routes.

H4 — primary-switch override at K=1024: P3 R2 and R6 resolved exact+clear
recall are both >=95%.

H5 — metadata guard: P4 produces zero false override selections and zero false
acceptances caused by a non-bound pair.

H6 — recurrent capability: K1024 F6 is >=90% at G=4 and >=85% at G=16; G4
teacher-forced advancing exact+clear is >=90% at h0/h1/h2.

H7 — route-specific transition override: P6 K1024 R2-base and R3-override
teacher-forced exact+clear recall are both >=90%.

H8 — local V robustness: P7 K1024 Hamming-1 exact+clear recovery remains >=20%.

H9 — one-bank-read invariant: excluding explicitly labelled counterfactual
diagnostics, every resolved query performs exactly one DeltaKvMemory read.

H10 — no use of retrieval strength for override presence: override bank choice
is a pure function of exact directory membership. This is enforced structurally
by the harness.

## Selection rule

If H1-H10 all hold, retain the two-bank resolver as the leading
**Development memory composition semantics**.

This does not promote any core API.

If any gate fails, report no resolver candidate. Do not add magnitude,
agreement, fallback or heuristic presence rules after execution.

## Boundary

N11 is still a synthetic mechanism panel.

Before any core promotion:

- unbalanced page-population / cross-page sharing controls;
- DKV-3a exact semantic-record/subject/provenance replay;
- representative recurrent SBG/MOR decisions;
- physical latency/RSS/traffic/energy qualification.

No controller, context or model promotion follows.
