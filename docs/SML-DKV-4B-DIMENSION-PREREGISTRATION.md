# DKV-4b — paired K=64/K=256 dimension panel preregistration

Status: frozen before implementation/execution.
Base: `0bcdeb1608f0588d0238255bcf03338e2f80e7e5`.

## Purpose

Measure whether key width changes upstream Delta-KV behavior while the DKV-4
compute policy remains identical. This panel is specifically designed to prevent
a controller threshold from being tuned separately for K=64 and K=256.

The experiment does not use the owner's reported 1.0%/13.0% result as a target,
acceptance threshold, or expected outcome.

## Frozen common information

For every seed, the exact same SemanticState values, write order, two-bit targets,
MOR selection, provenance labels, recurrent budget and DKV-4 policy are evaluated
at K=64 and K=256.

- state width: 64 coordinates in each V/M/A/R field;
- seeds: 0..7;
- 16 written facts per seed;
- target values cycle through all four two-bit Boolean pairs independently of K;
- one fixed MOR decision selecting PageId(5), with zero declared disagreement,
  used only to isolate key width;
- DKV-2a default projector; projector seed = experiment seed;
- full-rate Q12 writes in fact order;
- DKV-2b value coordinates [0,1], default ambiguity threshold;
- DKV-4a default policy, unchanged between dimensions;
- frozen query key during recurrence.

This fixed MOR arm is not a qualification of MOR competition.

## Query families

For each written fact:

1. WRITTEN: exact state, provenance = VerifiedPresent.
2. NEAR(h): a state not written, produced by exactly h distinct bit flips across
   V/M/A/R, h in {1,2,4,8,16,32}, provenance = Missing.
3. INDEPENDENT: a deterministic independently generated state not in the written
   set, provenance = Missing.

All generated unknown states must be checked against the complete written-state
set before scoring. Accidental equality invalidates the cell rather than being
relabelled after observation.

## Two distinct unknown-state measurements

### Safety path

Run the production-style DKV-4 runner with provenance = Missing.

Required invariant: zero Delta-KV matrix reads and zero StopCandidate decisions.
This measures the controller/provenance boundary, not memory discrimination.

### Latent diagnostic path

For the same unknown state, run the same DKV-2b query as research-only evidence
with provenance = Unverified. DKV-4 is not permitted to StopCandidate in this
mode. Record whether the first memory read resolves zero, one, or two output
coordinates and record correction magnitude/read MACs.

A resolved unknown latent output is NOT called an accepted answer. It is an
upstream memory/projection diagnostic.

## Written-state measurements

For each dimension report:

- complete final two-bit correctness;
- StopCandidate / Abstain counts;
- recurrent cycle count distribution;
- read MACs;
- first-cycle and final correction magnitude;
- ambiguity counts;
- projection work and transient key bits.

Known-state coverage must be reported alongside unknown-state safety. A mechanism
that abstains on everything is not a win.

## Baselines

For WRITTEN queries, compare:

- fixed depth 1 DKV-2b;
- fixed depth 4 DKV-2b;
- existing SBG stability-only DKV-2b;
- DKV-4a controller.

All arms use the same matrix and frozen query key. No arm may receive gold labels
as an input.

## Frozen interpretation

- DKV-4 policy is dimension-independent by construction.
- Different K results are upstream projection/memory effects unless the measured
  controller inputs differ.
- A lower latent unknown-resolution rate is not automatically better if written
  correctness/coverage falls.
- Missing provenance short-circuiting is a safety property, not proof that the
  matrix recognizes absence.
- No threshold, seed, Hamming set or write order may change after first execution.

## Required retained evidence

Store complete deterministic rows, summary, code SHA, protocol SHA, Rust version,
host/architecture and output SHA-256. Re-run at least twice byte-for-byte before
writing an observation document.

## Non-claims

Synthetic states are not learned language representations. Logical read-MAC/cycle
counts are not wall-clock latency or RSS. This panel cannot establish optimal K,
25B scaling, long-context quality, or language-model accuracy.

Scientific/model promotion remains BLOCKED.
