# DKV-4e — dynamic MOR recurrent evidence preregistration

Status: frozen before implementation/execution.
Base: `61cc15fb589ed1a665f425af49543a81ccd64324`.

## Motivation

DKV-4d established that static MOR disagreement/escalation must not spend
DKV-2b recurrent budget when the admitted composition cannot change the evidence
behind that MOR signal.

The complementary case remains open: MOR evidence can change between cycles
even when R itself is unchanged, because native MOR uses recurrent
`PreviousRoute` hysteresis. A narrow first-route margin may therefore clear
after one routed cycle.

This protocol isolates that case.

## Question

When MOR is recomputed between recurrent cycles with the previous route carried
forward, can DKV-4 use a genuinely changing precision-escalation signal without
falling back to either:

- ignoring MOR entirely; or
- spending the full recurrent budget on a stale first-cycle decision?

## Dynamic routing procedure

Every scored dynamic cycle must:

1. execute one DKV-2b frozen-key recurrent rewrite;
2. compute a fresh native MOR decision through
   `MorRouterRegistry -> MixtureOfRouters`;
3. pass the current `PreviousRoute` into MOR;
4. build DKV-4 signals from that exact cycle and fresh MOR decision;
5. let DKV-4d decide Refine / StopCandidate / Abstain with
   `mor_evidence_can_change=true`;
6. update `PreviousRoute` from the fresh MOR primary/selected set only after
   the decision evidence has been retained.

The dynamic loop is research-only in this slice. It must not replace the merged
bounded runners until the experiment is interpreted.

## Frozen MOR fixtures

All decisions are computed by the native MOR engine.

### CONSENSUS

Four routers coherently support PageId(0). Expected measured class:

- structural disagreement = 0;
- precision escalation = false.

### RESOLVING_MARGIN

Two routers produce different singleton top experts.

Frozen proposal scores:

- PageId(0): 1,000,000 ppm;
- PageId(1): 920,000 ppm.

With empty PreviousRoute, the final expert margin is expected to be 40,000 ppm,
which is below the default 50,000-ppm precision-escalation threshold.

After PageId(0) becomes the previous primary while both experts remain selected,
default recurrent hysteresis should increase the margin. The next fresh MOR
decision must be measured rather than assumed. Promotion of the fixture label
requires the second-cycle precision escalation to become false.

### PERSISTENT_MARGIN

Two singleton router groups use equal 1,000,000-ppm proposal scores. Previous
route hysteresis may change the margin but the fixture is intended to remain at
or below the default precision threshold. The measured decision controls the
label.

### STRUCTURAL_DISAGREEMENT

Four routers form two coherent 2-router groups supporting different experts.
The measured structural disagreement must remain at or above the native
precision threshold across the dynamic cycles used by this panel.

## Recurrent state fixtures

For each MOR fixture:

- STABLE: the Boolean V target is already correct before cycle 1;
- CORRECT_ONCE: cycle 1 changes V to the stored positive target.

The DKV memory, key, learning/update state and hard recurrent depth are identical
across arms.

## Comparison arms

1. SBG stability-only DKV-2b recurrence.
2. DKV-4d with the first MOR decision treated as fixed/invariant
   (`mor_evidence_can_change=false`).
3. Unchanged DKV-4c with the first MOR decision frozen across the run.
4. DKV-4e research dynamic loop: fresh MOR each cycle + DKV-4d dynamic
   actionability.

No arm receives expected answer correctness or evaluator truth as a controller
input.

## Frozen hypotheses

H1: CONSENSUS dynamic behavior matches stability-only depth and final state.

H2: RESOLVING_MARGIN begins with precision escalation and clears it after
carrying forward the first native PreviousRoute.

H3: on RESOLVING_MARGIN/STABLE, the dynamic arm uses more cycles than
stability-only/fixed-MOR DKV-4d but fewer than frozen-decision DKV-4c, and ends
with StopCandidate rather than hard-budget abstention.

H4: on RESOLVING_MARGIN/CORRECT_ONCE, the useful semantic correction and MOR
resolution fit within the same two cycles as stability-only, so dynamic MOR adds
no extra cycle beyond the semantic correction.

H5: PERSISTENT_MARGIN and STRUCTURAL_DISAGREEMENT do not falsely report resolved
MOR evidence; if the relevant signal remains unresolved through max depth, the
dynamic arm must abstain.

H6: all arms preserve the same final Boolean answer on every scored row.

H7: Missing/Conflict still abstain before a matrix read; Unverified never
StopCandidate.

Any failed hypothesis remains retained.

## Measurements

For each cycle retain:

- depth;
- V/M/A/R semantic delta;
- ambiguity count;
- correction magnitude;
- MOR structural disagreement;
- MOR precision escalation;
- ranked primary;
- selected experts;
- route margin;
- PreviousRoute entering and leaving the cycle;
- DKV-4 decision/reason.

Report total recurrent cycles and final result by arm.

## Interpretation boundary

Clearing a MOR precision-escalation flag is a control-plane result. It is not
task-quality improvement by itself. A dynamic arm that spends extra cycles
without changing downstream correctness must be reported as control resolution,
not as better model accuracy.

A later task-level experiment would need a preregistered decision where the
extra routed/refinement work changes a verified task outcome.

## Required evidence

- exact protocol/code identities;
- complete deterministic rows and per-cycle traces;
- two byte-identical reruns under Rust 1.89;
- native MOR path only;
- retained failures and persistent-disagreement controls;
- no threshold retuning after observation.

Scientific/model promotion remains BLOCKED.
