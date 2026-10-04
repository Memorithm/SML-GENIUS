# MOR-H2 — adaptive 1 -> 2 -> 4 hierarchical widening

Status: experimental reference implementation. No model-quality, latency, or
deployment-promotion claim.

## Purpose

MOR-H1 introduced bounded coarse-to-fine SBG/MOR candidate narrowing. MOR-H2
makes the coarse pool width conditional instead of fixed.

The schedule is deliberately small and auditable:

```text
1 cluster
   |
   | unresolved observable routing evidence
   v
2 clusters
   |
   | unresolved observable routing evidence
   v
4 clusters
```

The existing native `MixtureOfRouters` remains the only final expert-selection
engine. A widening stage is not a recurrent reasoning cycle and does not inherit
the previous stage's route as hysteresis state; every stage sees the same
external `PreviousRoute`.

## Observable widening signals

MOR-H2 may widen only from inference-visible state:

- coarse cluster frontier margin;
- final MOR top-1/top-2 route margin;
- native MOR structural disagreement;
- density of active bits in the SBG ambiguity field.

No task label, expected expert, evaluator truth, protected answer, or downstream
accept/reject outcome enters the widening policy.

The default thresholds are experimental engineering defaults. They are not
calibrated probabilities or scientific results.

## Work accounting and budgets

Cluster signatures are ranked once per adaptive decision. Expert scoring is
currently recomputed when the pool widens, so cumulative work is reported
truthfully rather than pretending earlier stages were free.

The report retains:

- cluster similarity evaluations;
- cumulative router similarity evaluations;
- cumulative expert similarity evaluations;
- cumulative candidate edges;
- per-stage candidate accounting;
- the exact widening reasons for every stage;
- whether unresolved evidence remained after the 4-cluster ceiling.

Separate hard budgets cover cumulative expert similarity evaluations and
candidate edges. Exceeding either fails closed.

## Exhaustive research control

`route_adaptive_with_exhaustive_control` runs a full-cluster route after the
adaptive decision and reports:

- whether the exhaustive primary was present in the final adaptive pool;
- whether the adaptive and exhaustive primary pages match;
- exhaustive control accounting separately from adaptive accounting.

The exhaustive arm is a research control only. Its work is never hidden in the
native adaptive-work counters.

Matching the exhaustive primary is not proof that the primary expert is
task-correct. Correct-expert recall requires an independently specified task
oracle on a frozen development/evaluation protocol.

## Qualification sequence

1. Software conformance and exact-head CI.
2. Frozen development protocol with exhaustive, fixed-1, fixed-2, fixed-4,
   adaptive-1/2/4 and matched-random-pool arms.
3. Measure exhaustive-primary shortlist recall separately from task-correct
   expert recall.
4. Report final task quality, abstention, active pages, expert exposure and
   cumulative routing work.
5. Measure cold/warm physical latency, traffic and residency only after logical
   work qualification.
6. Promote to SML-D3/Elastic integration only if the predeclared quality/recall
   gates hold at lower matched cost.

## Scientific boundary

MOR-H2 is a compute-allocation mechanism. A lower candidate count is not a model
quality gain, and exhaustive-primary agreement is not a correctness oracle.
The mechanism does not authorize an answer, relax SML-D calibration, or change
source/provenance requirements.
