# DKV-4d — MOR actionability gate preregistration

Status: frozen before implementation/execution.
Base: `cdc2e13811fbae6cf7aef8defbc8e73f877de3ca`.

## Motivation

DKV-4c correctly removed absolute Q12 residual magnitude as an unconditional
refinement signal. Its remaining MOR inputs are structural disagreement and
precision escalation.

In the current DKV-2b frozen-key recurrence, the memory kernel rewrites selected
V coordinates and their ambiguity bits. It does not rewrite the SBG routing
field R. Native semantic MOR routing is driven by R. Therefore, when all MOR
inputs and PreviousRoute are otherwise held fixed, a MOR disagreement caused
only by R is invariant under another DKV-2b cycle.

This protocol tests whether refusing to spend recurrent work on an invariant
MOR signal removes futile refinement without changing semantic outcomes.

## Question

When the current recurrent kernel cannot change the evidence on which a MOR
signal depends, should that MOR signal be allowed to force another DKV-4 cycle?

## New candidate

DKV-4d adds explicit refinement capabilities to DKV-4c.

The capability relevant to this panel is:

- `mor_evidence_can_change`.

If false, structural disagreement and MOR precision escalation remain recorded
as telemetry but cannot independently request another DKV-2b cycle.

If true, DKV-4d must preserve DKV-4c behavior for those signals.

Provenance precedence, semantic state-change handling, ambiguity handling and
hard depth limits remain unchanged.

The capability is declared by the composition/runtime, not inferred from task
truth, answer correctness or observed success.

## Actual MOR signal generation

The experiment must compute all MOR decisions through the native
`MixtureOfRouters` and `MorRouterRegistry`; no `MorDecision` field may be
manually populated for scored rows.

Three frozen MOR fixtures are required:

1. CONSENSUS: coherent routing with no structural disagreement and no precision
   escalation.
2. MARGIN_ESCALATION: zero structural disagreement but a sufficiently narrow
   final expert margin to request precision escalation.
3. STRUCTURAL_DISAGREEMENT: two coherent router groups support different
   experts, producing disagreement at or above the native precision threshold.

Router inputs are synthetic controls, but MOR signals themselves are produced by
the actual native engine.

## Recurrent fixtures

Use a one-coordinate DKV-2b Boolean target under the frozen query key.

For each MOR fixture evaluate:

- STABLE: the initial Boolean value already equals the stored target;
- CORRECT_ONCE: the first DKV-2b read changes V to the target, after which the
  discrete semantic state is stable.

Compare:

- unchanged DKV-4c;
- DKV-4d with `mor_evidence_can_change=false`;
- DKV-4d with `mor_evidence_can_change=true`;
- existing SBG stability-only recurrent control.

All arms use the same memory state, target and maximum depth.

## Frozen hypotheses

H1: on CONSENSUS rows, DKV-4d(false) and DKV-4c make identical recurrent-depth
and final-state decisions.

H2: on MARGIN_ESCALATION and STRUCTURAL_DISAGREEMENT rows, DKV-4c reaches the
hard budget when the MOR signal remains unresolved, while DKV-4d(false) stops
after the semantic state stabilizes.

H3: DKV-4d(true) reproduces DKV-4c decisions on all panel rows.

H4: DKV-4d(false) preserves the same final Boolean answer as DKV-4c and
stability-only on every row.

H5: Missing/Conflict provenance still abstains before any matrix read;
Unverified still cannot StopCandidate.

H6: the native MOR fixtures satisfy their declared signal class through measured
`structural_disagreement_ppm` and `request_precision_escalation`, not by
fixture labels alone.

## Interpretation boundary

If the hypotheses hold, the result supports an **actionability rule**:
a control signal may request more compute only when the next admitted refinement
step can in principle change the evidence behind that signal.

This does not establish that DKV-4d improves model quality. It only prevents
known-invariant control evidence from consuming recurrent budget.

If future recurrence can rewrite R or otherwise update MOR evidence between
cycles, `mor_evidence_can_change=true` is required and this optimization must
not suppress MOR-driven refinement.

## Required evidence

- exact protocol and code SHAs;
- complete deterministic row output;
- native MOR inputs and measured decisions;
- DKV-4c, DKV-4d(false), DKV-4d(true), stability-only cycle counts;
- final Boolean states and decision classes;
- zero-read Missing/Conflict checks;
- two byte-identical reruns under pinned Rust 1.89;
- retained negative results.

No threshold may be changed after observation.

## Non-claims

Synthetic MOR controls are not language-model routing quality. Logical cycle
savings are not wall-clock latency. This panel does not qualify K, learned
routing, context integration, or the 25B target.

Scientific/model promotion remains BLOCKED.
