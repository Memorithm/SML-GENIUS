# DKV-4a — conservative adaptive compute controller preregistration

Status: frozen design before implementation/execution on branch `research/dkv-4a-adaptive-controller`.
Base: `4405d122cb1d1836998141a04422b2c8a5cba886`.

## Question

Can SML reduce unnecessary recurrent work without turning Delta-KV magnitude,
SBG ambiguity, or MOR agreement into an answer-validity signal?

DKV-4a controls compute only. It never authorizes an answer or a source claim.

## Controller inputs

One bounded decision receives:

- current recurrent depth;
- exact V/M/A/R state-change count from SBG;
- number of ambiguous memory output coordinates;
- maximum absolute Delta-KV correction signal, used only to request more work;
- MOR structural disagreement and precision-escalation request;
- an external provenance state: verified-present, missing, conflict, or unverified.

No key dimension K, key vector, key hash, source bytes, expected answer, gold label,
or task identifier enters the policy.

## Actions

- `Refine`: execute another bounded recurrent cycle.
- `StopCandidate`: stop computation; downstream provenance/calibration/action gates still apply.
- `Abstain`: do not produce an accepted candidate from this controller path.

The controller has no `Accept` action.

## Frozen precedence

1. Missing or conflicting provenance -> `Abstain` immediately.
2. Unverified provenance -> `Refine` while budget remains, otherwise `Abstain`.
3. Before `min_depth` -> `Refine`.
4. Any unresolved compute signal -> `Refine` while budget remains.
5. Unresolved signal at `max_depth` -> `Abstain`.
6. Verified-present plus all stop conditions satisfied -> `StopCandidate`.

Unresolved signals are: excessive state change, memory ambiguity, correction magnitude
above the refine threshold, MOR disagreement above threshold, or MOR precision escalation.

## Dimension hypothesis

Because K is not an input, the decision rule is dimension-invariant by construction.
K=64 and K=256 may still produce different upstream signals. Such differences must be
reported as upstream memory/projection effects, not hidden in controller thresholds.

The controller must not be tuned separately for K.

## Baselines for the later execution slice

- fixed depth 1;
- fixed maximum depth;
- current SBG stability-only halting;
- DKV-4a conservative controller.

Unknown-state false acceptance must be reported separately from known-state coverage.
A lower false-acceptance rate obtained by refusing everything is not a win.

## Required tests before execution

- invalid depth/range/ppm policies fail closed;
- missing/conflict always abstain;
- unverified evidence never stops a candidate;
- each unresolved signal independently requests refinement below budget;
- unresolved signals abstain at the hard budget;
- verified stable evidence can stop;
- decisions are deterministic;
- no policy field or decision path depends on K.

## Non-claims

This is a control-plane mechanism, not evidence that Delta-KV improves language quality,
memory accuracy, provenance, or latency. Logical cycle savings are not wall-clock speedups.
No result from DKV-3a or the owner-supplied DKV work is overwritten.

Scientific/model promotion remains BLOCKED.
