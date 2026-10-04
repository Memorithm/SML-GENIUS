# DKV-2a-N1 — current-stack ambiguity panel observation

Status: Development observation under
`SML-DKV-2A-N1-CURRENT-PANEL-PROTOCOL.md`.

## Execution identity

Owner-supplied source input:

- `sml-dkv-0004-dkv-4d.patch`;
- 33,038 bytes;
- 593 lines;
- SHA-256 `4d3e2b2fd6db9a884c72e0d20f8a8597e8a9b5f495250c421bdb362fd2cf002a`.

Current-stack execution:

- code head: `f4721a6c2a8ce1eaef553506a5a3af585656d065`;
- GitHub Actions run: `37234176336`;
- job: `111529948744`;
- runner: `tarek-scirust-arm64-01`;
- Rust: 1.89.0;
- two explicit release-mode executions were byte-identical;
- retained machine-readable output:
  `research/dkv-2a-n1/CURRENT-PANEL.csv`;
- manifest: `research/dkv-2a-n1/MANIFEST.json`.

The current panel calls the actual library `DeltaKvSemanticProjector` for both
arms. It does not reuse the supplied patch's local projection implementation.

## Compatibility

Legacy `DeltaKvSemanticProjector::new` and explicit `Bipolar` produced
identical complete evidence on **9,600 / 9,600** compatibility probes.

The N1 candidate therefore does not change the existing constructor/default
behavior.

## Zero ambiguity

Mean independent-key cosine:

| K | Bipolar | ActiveOnly |
|---:|---:|---:|
| 64 | 0.209256 | 0.020063 |
| 256 | 0.248044 | 0.008659 |
| 1024 | 0.249681 | 0.003418 |

Strict written-fact recall:

| K | arm | W=8 | W=16 | W=32 | W=64 |
|---:|---|---:|---:|---:|---:|
| 64 | Bipolar | 57.81% | 28.52% | 11.52% | 5.27% |
| 64 | ActiveOnly | 92.19% | 60.94% | 28.71% | 13.09% |
| 256 | Bipolar | 53.91% | 24.61% | 11.52% | 5.08% |
| 256 | ActiveOnly | 100% | 94.53% | 62.89% | 26.27% |
| 1024 | Bipolar | 53.91% | 24.61% | 11.52% | 4.59% |
| 1024 | ActiveOnly | 100% | 97.66% | 78.71% | 37.89% |

At W=16, K=256 improves from 24.61% to 94.53% strict recall and K=1024
from 24.61% to 97.66%. This reproduces the supplied mechanism finding on the
current Rust 1.89/library path.

The candidate still has finite capacity. At W=64 strict recall is only 13.09%,
26.27% and 37.89% for K=64/256/1024 respectively. Wider keys therefore help
after the common ambiguity vector is removed, but do not make the normalized
delta matrix an exact high-load store.

## Sparse ambiguity

SPARSE contains exactly two active ambiguity bits per state.

Mean cosine:

| K | Bipolar | ActiveOnly |
|---:|---:|---:|
| 64 | 0.183160 | 0.019440 |
| 256 | 0.220753 | 0.010182 |
| 1024 | 0.220655 | 0.005565 |

Strict recall at W=16:

| K | Bipolar | ActiveOnly |
|---:|---:|---:|
| 64 | 30.86% | 63.28% |
| 256 | 33.20% | 94.92% |
| 1024 | 33.59% | 98.83% |

The ZERO result is therefore not confined to an exactly empty A field. It
persists under this declared two-bit sparse control.

## Dense ambiguity is the counterexample

DENSE_CONTROL is not a promotion success case. Bipolar is better once ambiguity
itself carries dense state-dependent information.

At K=1024:

- W=16: Bipolar 100%, ActiveOnly 94.53%;
- W=32: Bipolar 88.67%, ActiveOnly 65.63%;
- W=64: Bipolar 48.54%, ActiveOnly 30.37%.

Mean cosine is 0.002884 for Bipolar versus 0.073435 for ActiveOnly.

This refines the mechanism: ActiveOnly removes a harmful common vector when
ambiguity is absent/sparse, but discards the negative half of a useful dense
ambiguity code. It is therefore not justified as a universal projector
replacement without measuring the ambiguity distribution produced by
representative SBG recurrence.

## Projection work

All 256 semantic coordinates are still read per SemanticState<64> projection.

For ZERO ambiguity, the W=16 aggregate bucket accumulations are:

- Bipolar: 131,584;
- ActiveOnly: 98,816.

That is 32,768 fewer bucket accumulations, about 24.9%, while matrix dimensions,
matrix semantic bits, reads, read MACs, updates and weight updates are unchanged.

This is a projection-work reduction only. It is not evidence of lower physical
latency, RSS or energy.

## Near-miss exact-key aliasing

ZERO-ambiguity exact-key alias rates:

| K | arm | h=1 | h=2 | h=4 | h=8 |
|---:|---|---:|---:|---:|---:|
| 64 | Bipolar | 43.13% | 18.28% | 2.50% | 0.31% |
| 64 | ActiveOnly | 36.41% | 14.69% | 2.19% | 0% |
| 256 | Bipolar | 11.88% | 0.78% | 0% | 0% |
| 256 | ActiveOnly | 8.28% | 0.78% | 0% | 0% |
| 1024 | Bipolar | 1.09% | 0% | 0% | 0% |
| 1024 | ActiveOnly | 0.47% | 0% | 0% | 0% |

ActiveOnly does not increase exact-key near-miss aliasing in any frozen cell.
This is useful but insufficient: at K=64 one-bit aliasing remains 36.41%, and
exact-key equality is not semantic-record identity or source authority.

## Frozen hypotheses

The retained output reports:

- H1 compatibility — HOLDS;
- H2 ZERO projection-work reduction — HOLDS;
- H3 ZERO key-geometry improvement — HOLDS;
- H4 ZERO W=16 recall improvement at K=256/1024 — HOLDS;
- H5 finite-capacity degradation retained — HOLDS;
- H6 SPARSE distribution measured separately — HOLDS;
- H8 near-miss bound — HOLDS.

H7 is a scope invariant rather than a performance result: source authority is
unchanged. The panel contains no path that upgrades a Delta-KV key/read into
source presence, correctness or an external-action authorization.

## Decision

**Retain ActiveOnly as a qualified Development candidate, but do not promote it
to the default DKV-2a projector.**

The current-stack panel establishes a causal and reproducible advantage for
zero/sparse ambiguity and exposes a reproducible dense-ambiguity regression.
The next gate is therefore not another projector-width sweep.

Before promotion:

1. measure the actual A-field density/distribution on representative SBG
   recurrent states;
2. run the existing exact semantic-record / subject-bound DKV-3a alias controls
   with ActiveOnly;
3. retain Missing/Conflict/Unverified provenance boundaries;
4. compare matched physical latency/traffic after the logical projection-work
   reduction is established;
5. only then consider projection-agreement or DKV-4 factorial composition.

No context/model promotion follows from this result.
