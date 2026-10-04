# DKV-4c — semantic-impact adaptive controller preregistration

Status: frozen before implementation/execution.
Base: `97c0e39929af4d63963251eb725c67b9384611ad`.

## Motivation

DKV-4a remained over-conservative on the frozen K=64/K=256 panel: it used
about 1.95x the recurrent cycles of the existing SBG stability-only control
without improving written-state correctness. The retained observation identified
absolute Q12 correction magnitude as the likely cause when the discrete semantic
state was already stable.

DKV-4c is a new controller. DKV-4a remains unchanged and remains a comparison arm.

## Question

Can a controller driven only by observable semantic impact and MOR escalation
recover useful adaptive stopping while preserving the fail-closed provenance
boundary introduced by DKV-4a?

## Inputs

DKV-4c receives the same executed-cycle evidence object as DKV-4a:

- current recurrent depth;
- exact V/M/A/R state delta;
- number of ambiguous memory outputs;
- Delta-KV correction magnitude, retained for telemetry only;
- MOR structural disagreement;
- MOR precision-escalation request;
- external provenance state.

The policy does **not** contain a correction-magnitude threshold.
Key dimension K, key vector/hash, source bytes, expected answer and gold label
remain excluded from the policy.

## Actions and provenance precedence

Actions remain `Refine`, `StopCandidate`, and `Abstain`; there is no
`Accept` action.

1. Missing or Conflict -> `Abstain` before any matrix read.
2. Unverified -> `Refine` while budget remains, then `Abstain`.
3. Before `min_depth` -> `Refine`.
4. Ambiguous memory output -> `Refine` while budget remains.
5. MOR precision escalation -> `Refine` while budget remains.
6. MOR structural disagreement above the frozen threshold -> `Refine`.
7. V/M/A/R delta above the frozen stability threshold -> `Refine`.
8. Otherwise -> `StopCandidate`.

At the hard maximum depth, any unresolved signal yields `Abstain`.

Correction magnitude is reported but can never independently force refinement.

## Frozen policy

- min depth: 1;
- max depth: 4;
- semantic Hamming threshold: 0;
- MOR disagreement threshold: 250,000 ppm.

These values are not tuned separately by K.

## Development comparison

Reuse the paired deterministic K=64/K=256 task generator from DKV-4b.

For written queries compare:

- fixed depth 1;
- fixed depth 4;
- existing SBG stability-only control;
- unchanged DKV-4a;
- DKV-4c semantic-impact controller.

For unknown queries:

- Missing provenance must short-circuit before any matrix read;
- Unverified is diagnostic only and can never `StopCandidate`.

Report correctness, coverage, abstentions, cycle totals, read MACs, correction
telemetry and K-dependent logical storage/work.

## Frozen hypotheses

H1: DKV-4c uses no more recurrent cycles than DKV-4a on every paired dimension.

H2: DKV-4c preserves written-state final correctness relative to DKV-4a on the
frozen panel.

H3: Missing/Conflict continue to execute zero matrix reads.

H4: DKV-4c never permits Unverified evidence to produce `StopCandidate`.

H5: on a constructed positive-sign fixture where the Boolean value is already
correct but analog Q12 magnitude differs from +4096, DKV-4c stops after one
stable cycle while unchanged DKV-4a continues because of residual magnitude.

All hypotheses are falsifiable. Refuted clauses remain reported as refuted.

## Non-claims

The panel is synthetic. Logical cycle/MAC reductions are not wall-clock speedups.
DKV-4c is not an unknown-state detector; provenance supplies the production
unknown-state gate. No language quality, long-context, RSS, latency or 25B-scale
claim follows from this experiment.

Scientific/model promotion remains BLOCKED.
