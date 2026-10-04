# DKV-4d — MOR actionability observation

Status: Development observation under the frozen protocol in
`SML-DKV-4D-MOR-ACTIONABILITY-PREREGISTRATION.md`.

Protocol commit: `d119d83a97857817980db638fde43c925b2f9f50`.
Executed code: `6c04630a969becb0427913cb82e9fc0389490952`.
RemoteOps run: `37199917734`.
Host: `tarek`, aarch64. Rust: 1.89.0.
Two explicit runs were byte-identical.
Output SHA-256:
`ecbf25dd257e62beebe792b80bc3dc3d7f57051db54599ec37a5cc2b7591841e`.

Complete retained evidence is in `research/dkv-4d/`.

## Native MOR fixtures

The scored MOR decisions were produced by the actual
`MorRouterRegistry -> MixtureOfRouters` path. No `MorDecision` field was
manually populated for scored rows.

| fixture | active routers | ranked experts | selected experts | structural disagreement | precision escalation |
|---|---:|---:|---:|---:|---|
| CONSENSUS | 4 | 1 | 1 | 0 ppm | false |
| MARGIN_ESCALATION | 2 | 2 | 2 | 0 ppm | true |
| STRUCTURAL_DISAGREEMENT | 4 | 2 | 2 | 500,000 ppm | true |

The labels therefore match measured native-MOR behavior rather than assumed
fixture names.

## Recurrent result

Two recurrent starting states were used:

- STABLE: V already equals the stored positive target;
- CORRECT_ONCE: the first DKV-2b read flips V to the target and the next
  discrete state is stable.

All final Boolean outputs were correct in every arm.

| MOR fixture | state | stability-only depth | DKV-4c depth/result | DKV-4d fixed-MOR depth/result | DKV-4d dynamic-MOR depth/result |
|---|---|---:|---|---|---|
| CONSENSUS | STABLE | 1 | 1 / StopCandidate | 1 / StopCandidate | 1 / StopCandidate |
| CONSENSUS | CORRECT_ONCE | 2 | 2 / StopCandidate | 2 / StopCandidate | 2 / StopCandidate |
| MARGIN_ESCALATION | STABLE | 1 | 4 / Abstain | 1 / StopCandidate | 4 / Abstain |
| MARGIN_ESCALATION | CORRECT_ONCE | 2 | 4 / Abstain | 2 / StopCandidate | 4 / Abstain |
| STRUCTURAL_DISAGREEMENT | STABLE | 1 | 4 / Abstain | 1 / StopCandidate | 4 / Abstain |
| STRUCTURAL_DISAGREEMENT | CORRECT_ONCE | 2 | 4 / Abstain | 2 / StopCandidate | 4 / Abstain |

Across the six scored rows, unchanged DKV-4c uses 19 recurrent cycles and
DKV-4d with fixed-MOR actionability uses 9, a reduction of 10/19 = 52.63%.
On the four escalation/disagreement rows alone, DKV-4c uses 16 cycles and
DKV-4d fixed-MOR uses 6, a reduction of 62.5%.

Those logical savings are mechanism evidence, not wall-clock latency.

## Why the gate is valid only for this composition

The current DKV-2b frozen-key kernel rewrites configured V coordinates and their
ambiguity bits. It does not rewrite R. The MOR fixtures in this panel are
therefore invariant under another admitted DKV-2b step.

DKV-4d makes that fact explicit through
`Dkv4RefinementCapabilities::dkv2b_fixed_mor()`. Structural disagreement and
precision escalation remain visible telemetry, but cannot independently spend
another recurrent cycle when the composition declares that MOR evidence cannot
change.

The `dynamic_mor()` capability is the counter-control. On all six rows it
reproduces unchanged DKV-4c depth, final state and decision. DKV-4d therefore
does not globally suppress MOR signals; it suppresses them only when the
composition explicitly declares them invariant.

If a future recurrent composition can change R or otherwise recompute different
MOR evidence between cycles, `mor_evidence_can_change=true` is required.

## Provenance boundary

For all three MOR fixtures, Missing provenance produced zero matrix reads and an
immediate `Abstain(MissingEvidence)`.

Unverified provenance never produced `StopCandidate`; it consumed the bounded
four-cycle diagnostic budget and ended in `Abstain(UnverifiedEvidence)`.

Actionability does not weaken the provenance gate.

## Frozen hypotheses

- **H1 holds.** Under CONSENSUS, DKV-4d(false) and DKV-4c have identical depths
  and final decisions.
- **H2 holds.** Under margin escalation and structural disagreement, DKV-4c
  exhausts the hard budget while DKV-4d(false) stops when semantic state
  stability is reached.
- **H3 holds.** DKV-4d(true) reproduces DKV-4c on all six scored rows.
- **H4 holds.** Final Boolean answers match DKV-4c and stability-only on every
  scored row.
- **H5 holds.** Missing/Conflict and Unverified fail-closed behavior is
  preserved by implementation tests and the retained Missing/Unverified rows.
- **H6 holds.** The native MOR engine measured the declared signal classes.

## Decision

DKV-4d is a better composition rule than unconditional MOR-driven refinement
for the current fixed-MOR DKV-2b runner. It prevents compute from being spent on
control evidence that the admitted recurrent step cannot change.

This does **not** yet show that MOR-driven recurrence is useful when MOR evidence
is actually dynamic. The next scientifically relevant experiment is a recurrent
composition in which R or other MOR evidence can change between cycles, with a
separately frozen baseline demonstrating whether those additional cycles repair
a measurable downstream decision rather than merely consuming budget.

No context/model promotion follows from this mechanism panel.

**Scientific/model promotion remains BLOCKED.**
