# DKV-4e — dynamic MOR recurrent-evidence observation

Status: Development observation under the frozen protocol in
`SML-DKV-4E-DYNAMIC-MOR-PREREGISTRATION.md`.

Protocol commit: `8a239db660a63b1be2ef4ac7880d325ed5cd14f6`.
Executed code: `d6a7c425b25958863a90bdae4b6c73692f0b8f2f`.
RemoteOps run: `37200716553`.
Host: `tarek`, aarch64. Rust: 1.89.0.
Report: 42 retained lines, including 8 scored rows and 23 dynamic-cycle traces.
Two explicit executions were byte-identical.
Output SHA-256:
`91d6aa9b1359dc528d38687861e092193713bb4f92e285feac371b57edc4aa37`.

Complete retained evidence is in `research/dkv-4e/`. No MOR threshold,
proposal score, recurrent limit or classification rule was changed after
observation.

## 1. Native MOR changes under recurrent hysteresis

Every scored MOR decision is produced by the native
`MorRouterRegistry -> MixtureOfRouters` path. The dynamic arm recomputes MOR
on every recurrent cycle and carries the previous native route forward.

| fixture | first margin | first disagreement | first escalation | second margin | second disagreement | second escalation |
|---|---:|---:|---|---:|---:|---|
| CONSENSUS | 2,000,000 ppm | 0 | false | 2,000,000 ppm | 0 | false |
| RESOLVING_MARGIN | 40,000 ppm | 0 | true | 70,000 ppm | 0 | false |
| PERSISTENT_MARGIN | 0 ppm | 0 | true | 30,000 ppm | 0 | true |
| STRUCTURAL_DISAGREEMENT | 0 ppm | 500,000 ppm | true | 30,000 ppm | 500,000 ppm | true |

The preregistered resolving case therefore occurred exactly as a native control
effect: the prior PageId(0) route increases the next expert margin from 40,000
to 70,000 ppm, clearing the default 50,000-ppm precision-escalation condition.

The two negative controls also behave as intended. Equal-score margin remains
at only 30,000 ppm after hysteresis, and structural disagreement remains
500,000 ppm.

## 2. Recurrent comparison

All eight scored rows end with the same correct positive Boolean target in all
compared arms.

| fixture | state | stability-only | DKV-4d fixed MOR | frozen-decision DKV-4c | dynamic MOR + DKV-4d |
|---|---|---:|---:|---:|---:|
| CONSENSUS | STABLE | 1 | 1 | 1 / StopCandidate | 1 / StopCandidate |
| CONSENSUS | CORRECT_ONCE | 2 | 2 | 2 / StopCandidate | 2 / StopCandidate |
| RESOLVING_MARGIN | STABLE | 1 | 1 | 4 / Abstain | 2 / StopCandidate |
| RESOLVING_MARGIN | CORRECT_ONCE | 2 | 2 | 4 / Abstain | 2 / StopCandidate |
| PERSISTENT_MARGIN | STABLE | 1 | 1 | 4 / Abstain | 4 / Abstain |
| PERSISTENT_MARGIN | CORRECT_ONCE | 2 | 2 | 4 / Abstain | 4 / Abstain |
| STRUCTURAL_DISAGREEMENT | STABLE | 1 | 1 | 4 / Abstain | 4 / Abstain |
| STRUCTURAL_DISAGREEMENT | CORRECT_ONCE | 2 | 2 | 4 / Abstain | 4 / Abstain |

Aggregate recurrent cycles across the eight scored rows are:

- stability-only: **12**;
- DKV-4d with MOR declared invariant: **12**;
- DKV-4c with the first MOR decision frozen: **27**;
- dynamic native MOR + DKV-4d(dynamic): **23**.

The dynamic arm therefore removes 4/27 = **14.81%** of the frozen-decision
DKV-4c cycles over the complete panel. The benefit is localized entirely to the
RESOLVING_MARGIN fixture.

On RESOLVING_MARGIN alone, frozen-decision DKV-4c uses 8 cycles across its two
state fixtures while dynamic MOR uses 4, a **50%** reduction. Stability-only
and fixed-MOR DKV-4d use 3 cycles. Dynamic MOR therefore spends one additional
cycle relative to those simpler controls: that cycle resolves the measured MOR
uncertainty but does not change final Boolean correctness in this synthetic
task.

For CORRECT_ONCE specifically, the useful Boolean repair already requires two
cycles under stability-only. MOR resolution fits inside those same two cycles,
so dynamic MOR adds no recurrent depth. For STABLE, dynamic MOR deliberately
spends a second cycle solely to establish the wider route margin.

## 3. Per-cycle evidence

The retained traces separately expose V/M/A/R deltas. The current memory
fixture changes only V on the first CORRECT_ONCE cycle:

- `delta_v=1`;
- `delta_m=0`;
- `delta_a=0`;
- `delta_r=0`.

All subsequent semantic deltas are zero. MOR changes in RESOLVING_MARGIN
therefore come from native `PreviousRoute` hysteresis, not from an unreported
R rewrite.

The trace also retains ambiguity, Q12 correction magnitude, entering/leaving
PreviousRoute, route margin, disagreement, escalation, primary expert, selected
experts and the exact DKV-4 decision.

## 4. Persistent controls remain fail-closed

PERSISTENT_MARGIN reaches a second-cycle margin of 30,000 ppm and remains below
the default precision threshold. Dynamic DKV-4 therefore continues through the
hard depth bound and abstains.

STRUCTURAL_DISAGREEMENT retains 500,000 ppm disagreement across the observed
cycles and likewise ends in hard-budget abstention.

The dynamic mechanism therefore does not interpret any route change as
resolution. The measured controlling signal must actually clear.

## 5. Provenance boundary

For every MOR fixture:

- Missing provenance: depth 0, **0 matrix reads**, immediate
  `Abstain(MissingEvidence)`;
- Conflict provenance: depth 0, **0 matrix reads**, immediate
  `Abstain(ConflictingEvidence)`;
- Unverified provenance: four bounded reads/cycles and
  `Abstain(UnverifiedEvidence)`, never StopCandidate.

Dynamic MOR does not weaken the provenance contract.

## 6. Frozen hypotheses

- **H1 holds.** CONSENSUS dynamic depth/final state matches stability-only.
- **H2 holds.** RESOLVING_MARGIN changes from 40,000 ppm / escalation=true to
  70,000 ppm / escalation=false after carrying the first native route forward.
- **H3 holds.** RESOLVING_MARGIN/STABLE uses depth 2: more than stability/fixed
  depth 1, less than frozen DKV-4c depth 4, and ends in StopCandidate.
- **H4 holds.** RESOLVING_MARGIN/CORRECT_ONCE ends at depth 2, equal to the
  semantic repair depth of stability-only.
- **H5 holds.** PERSISTENT_MARGIN and STRUCTURAL_DISAGREEMENT remain unresolved
  and dynamic DKV-4 abstains at the hard bound.
- **H6 holds.** All arms preserve the same final Boolean answer on every scored
  row.
- **H7 holds.** Missing/Conflict are zero-read; Unverified never StopCandidate.

## Decision

DKV-4e demonstrates that a genuinely changing native MOR control signal can be
used without treating the first MOR decision as permanently unresolved. Native
recurrent hysteresis can clear a narrow-margin precision escalation and avoid
the frozen-decision hard-budget abstention.

This is **control-plane resolution, not a task-quality improvement**. Every
scored arm already reaches the same correct Boolean target. On the STABLE
resolving fixture, dynamic MOR actually costs one extra cycle relative to
stability-only in exchange for resolving route uncertainty.

The next scientifically stronger experiment must therefore bind MOR resolution
to a verified downstream task effect: a case where the additional routed or
refinement work changes a preregistered decision outcome, rather than merely
changing a control flag. It should retain a matched simpler-control arm so that
control resolution alone cannot be counted as model-quality gain.

No context/model promotion follows from this panel.

**Scientific/model promotion remains BLOCKED.**
