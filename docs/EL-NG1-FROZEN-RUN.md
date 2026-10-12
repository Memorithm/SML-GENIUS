# EL-NG1 — frozen Candide control-plane run (development evidence)

**Recorded 2026-10-12 from a completed GitHub-hosted execution.**
The earlier placeholder in this file was committed through PR #532.
This correction records the existing run and changes **no experiment
code, frozen source corpus, model criterion or selected policy**.

## Reproducible source and provenance

- Original frozen methodology: `docs/EL-NG1-ELASTIC-NGRAM-PREREGISTRATION.md`.
- Source model: `crates/sml-core/src/elastic_ngram_packed.rs`, baseline
  Witten–Bell 256-way prediction with n-gram depth 0..5 and count-width
  policies `32,16,8,4,2`.
- External resource-control library: ElasticXxx git revision
  `958ce6301730ceeec4bb1252ff12222e5bb7d431`, as used by a
  *separate* benchmark crate `sml-elastic-bench`, not a mandatory
  dependency of `sml-core`.
- Source HEAD: `a6aa5379ebc26cab45e5f43dcaf23f53f1c7bbbb`, subsequently
  merged to `main` as PR #528 at
  `92954853d596c68f9145e4e1c537e772ab6e185a`.
- Successful [GitHub Actions run 38313069235](https://github.com/Memorithm/SML-GENIUS/actions/runs/38313069235),
  job `115023150994`, pinned Rust 1.89.
- [Original raw report artifact 11775023787](https://github.com/Memorithm/SML-GENIUS/actions/runs/38313069235/artifacts/11775023787),
  ZIP SHA256 as emitted by GitHub Actions
  `41982749bab6ae10f37e2a9098ea9dcb814dff92444ac664a02f8188f65f9195`
  (archive content not independently re-downloaded).
- Frozen Project Gutenberg Candide body (SHA256
  `48e04595f5fc681ef6c0498123180bce0125d8fbe2167f3bcaeef189b45bd4b9`);
  report lists TRAIN **168,524 bytes**, VAL **21,065 bytes**.
  Heldout **TEST not read by the benchmark**. VAL measures grid and
  drives the gate; therefore **no out-of-sample quality claim**.
- 30 explicit `(k,w)` profiles; EIR fingerprint
  `fp:4f4c93b6112f1aa6` is noncryptographic, not source attestation.

## Measured examples from the frozen 30-profile VAL grid

These are **exact logical packed sizes**, not RAM of the Rust program.

| Profile | Packed KiB | VAL bits/byte | VAL accuracy |
|---|---:|---:|---:|
| k=0, w=32 | 0.5 | 4.7061 | 0.1521 |
| k=2, w=16 | 29.0 | 2.8121 | 0.4121 |
| k=3, w=16 | 117.5 | 2.4700 | 0.4964 |
| **k=4, w=16** | **345.5** | **2.3489** | 0.5459 |
| k=4, w=32 | 486.5 | 2.3489 | 0.5459 |
| k=5, w=16 | 795.2 | 2.3840 | 0.5655 |
| k=5, w=32 | 1069.0 | 2.3840 | 0.5655 |

The best frozen grid log-loss is **2.3489 bits/byte (k=4)**.
The deeper k=5 profile has higher argmax accuracy but **worse**
log-loss (2.3840), demonstrating why log-loss rather than accuracy
controls the resource/quality arbiter in this experiment.
The w=16 count variant matches w=32 here because these particular
count magnitudes do not require loss by right shifting; this is not
a universal 2x memory-saving proof for deployed N-gram structures.

## Executed budget schedule

All 11 steps matched the independently derived controller oracle.
The frozen control sequence and measured values in the original log:

| Budget KiB | Outcome | Committed profile | Accounted packed KiB | VAL bits/byte | Over budget |
|---:|---|---|---:|---:|---|
| 2048 | Committed(Reencode) | k4/w16 | 345.5 | 2.3489 | no |
| 1024 | Stay | k4/w16 | 345.5 | 2.3489 | no |
| 512 | Stay | k4/w16 | 345.5 | 2.3489 | no |
| 256 | Committed(Reencode) | k3/w16 | 117.5 | 2.4700 | no |
| 128 | Stay | k3/w16 | 117.5 | 2.4700 | no |
| 64 | Committed(Reencode) | k2/w16 | 29.0 | 2.8121 | no |
| 32 | Stay | k2/w16 | 29.0 | 2.8121 | no |
| **16** | **QualityInfeasible** | k2/w16 | 29.0 | 2.8121 | **yes** |
| **8** | **QualityInfeasible** | k2/w16 | 29.0 | 2.8121 | **yes** |
| 256 | Committed(Recompute) | k3/w16 | 117.5 | 2.4700 | no |
| 2048 | Committed(Recompute) | k4/w16 | 345.5 | 2.3489 | no |

**Important:** at the 16 KiB and 8 KiB budgets, this demo
continues to *describe* the previously committed k2/w16 state
(29 KiB) as **over budget**, rather than enforcing the lower budget
through a physical allocator or terminating actual inference.
`QualityInfeasible` is a blocked transition, not demonstrated hard
memory enforcement. Production admission needs a separate hard-budget
contract, resource allocator evidence and fail-closed execution policy.

## Frozen scientific criteria

All **7/7** predefined development tests passed:

- **C1:** six context lengths bit-identical to independent
  `NGramModel` on the same VAL data.
- **C2:** all 30 profiles' packed stream lengths equal declared
  logical accounting; decode/encode yields identical model.
- **C3:** 315 narrowing pairs path-independent, 585 widening attempts refused
  **by SML's model-specific reencoder**.
- **C4:** generic ElasticXxx structural validation allows a
  *physically impossible* widening `Reencode` with attestation;
  **SML rejects the same action** on domain feasibility.
- **C5:** all 11 controller steps agree with the independent
  oracle; quality guard, budget reporting, epoch, shadow model agreement.
- **C6:** rollback keeps the original committed model and
  fingerprint, the staging frontier can be reused, double-staging refused.
- **C7:** deterministic EIR fingerprint, with
  `Reencode` and `Recompute` declared and capability-grounded.

The final literal GitHub evidence line is
`EL_NG1_RESULT,criteria_passed=7/7,frozen_corpus=true`.

## Boundary and next decision

This qualification proves a deterministic structural controller and
the **need for model-owned feasibility enforcement**. It neither
demonstrates runtime memory management or safe physical count-width
transitions, nor shows a predictive gain versus a different
estimator. In particular: n-gram depth changes and lossy count
quantisation change predictor semantics; a generic ElasticXxx
`Ok` from structural attestation cannot grant lossless reencoding.

Physical representation candidate ENG-1b has since been separately
tested on native Thor CPU, but without ElasticXxx physical actuation:
[run 38314983846](https://github.com/Memorithm/SML-GENIUS/actions/runs/38314983846),
[report](RESULTS-ENG-1B-THOR-NATIVE-5X.md).
The later independent language-quality and capacity research is
tracked at [ENG-1c issue #530](https://github.com/Memorithm/SML-GENIUS/issues/530);
generic safe actuation remains
[ElasticXxx issue #244](https://github.com/Memorithm/ElasticXxx/issues/244).

**Verdict:** `EL_NG1_DEVELOPMENT_STRUCTURAL_PASS`;
`TEST_QUALITY=NOT_EVALUATED`,
`PHYSICAL_ELASTIC_ACTUATION=NOT_QUALIFIED`,
`SML_25B_PROMOTION=NOT_AUTHORIZED`.
