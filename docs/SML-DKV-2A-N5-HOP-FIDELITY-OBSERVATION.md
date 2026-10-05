# DKV-2a-N5 — per-hop read/rewrite fidelity observation

Status: current-stack Development diagnostic under
`SML-DKV-2A-N5-HOP-FIDELITY-PREREGISTRATION.md`.

## Execution identity

- protocol merged before implementation in PR #172;
- protocol merge: `6cef85d13aff83535497a3c06fb63666e341576d`;
- executed code head: `c8cd883848ace1e3b21db44504217015fe32b1f7`;
- GitHub Actions run: `37278718915`;
- job: `111661502767`;
- runner: `tarek-scirust-arm64-01`;
- Rust 1.89.0;
- two release-mode executions were byte-identical;
- retained output: `research/dkv-2a-n5/CURRENT-FIDELITY.csv`;
- manifest: `research/dkv-2a-n5/MANIFEST.json`.

The tested projector is the N4 Development candidate only:
current `ActiveOnly` plus the outer PageId(5)-derived seed. No core API or
projector implementation changed.

## Main result: the remaining N4 failure is already present in local retrieval

N4 left a low-load K-independent capability pattern:

- L0: 16/16;
- L1: 16/16;
- L2: 13/16;
- L3: 8/16.

N5 shows that this pattern is not primarily created by accumulated free-running
recurrent drift.

At K=1024,G=4, teacher-forced reads start from the **exact true semantic state**:

| source hop | advancing queries | exact+clear read |
|---:|---:|---:|
| h0 | 48 | 22/48 = 45.83% |
| h1 | 32 | 18/32 = 56.25% |
| h2 | 16 | 8/16 = 50.00% |

The exact source state is therefore insufficient to retrieve the intended next
node reliably even before free-running state error can accumulate.

## Terminal self-retrieval reproduces the N4 capability ceiling

At G=4, K=256 and K=1024 have the same terminal self-retrieval pattern:

- terminal of L0 chain: 16/16 exact+clear;
- terminal of L1 chain: 16/16 exact+clear;
- terminal of L2 chain: 13/16 exact+clear;
- terminal of L3 chain: 8/16 exact+clear.

Aggregated terminal self-retrieval is therefore **53/64 = 82.81%**.

This exactly matches the N4 pooled low-load combined ORACLE ceiling and its
class pattern.

The strongest current interpretation is consequently narrower than the N4
working hypothesis: the L2/L3 capability failure is already an associative
memory/crosstalk failure on exact stored states. It is not necessary to invoke
recurrent drift to explain it.

## Free-running fidelity

At K=1024,G=4:

| depth | exact semantic state | teacher exact+clear from corresponding true source |
|---:|---:|---:|
| 1 | 22/48 = 45.83% | h0: 22/48 = 45.83% |
| 2 | 16/32 = 50.00% | h1: 18/32 = 56.25% |
| 3 | 8/16 = 50.00% | h2: 8/16 = 50.00% |

Free-running fidelity never exceeds teacher-forced fidelity, as preregistered,
but it is not the primary source of the observed failures.

At high load K=1024,G=16, the same conclusion remains:

- failed L2/L3 chains: 62;
- first divergence `teacher_read_wrong`: 61;
- first divergence `teacher_read_weak`: 1;
- `free_read_wrong`: 0;
- `rewrite_ambiguous`: 0;
- `reprojection_drift`: 0.

## First-divergence attribution

For G=4:

| K | failed L2/L3 | teacher wrong | teacher weak | free wrong | rewrite ambiguity | reprojection drift |
|---:|---:|---:|---:|---:|---:|---:|
| 64 | 11 | 11 | 0 | 0 | 0 | 0 |
| 256 | 11 | 11 | 0 | 0 | 0 | 0 |
| 1024 | 11 | 11 | 0 | 0 | 0 | 0 |

Every failed L2/L3 chain is classified before or at the terminal step, and
every low-load failure begins with a wrong teacher-forced associative read.

This is the decisive N5 localization result.

## Rewrite and reprojection are not the observed bottleneck

Whenever the free-running semantic state exactly equals the expected true node,
reprojection produces the exact expected key.

Examples at K=1024,G=4:

- depth 1 exact states: 22/48; exact keys: 22/48;
- depth 2 exact states: 16/32; exact keys: 16/32;
- depth 3 exact states: 8/16; exact keys: 8/16.

All explicit `exact semantic state => exact key` checks pass.

No first divergence in the frozen panel is attributed to
`reprojection_drift`, and none of the failed L2/L3 chains is first attributed
to `rewrite_ambiguous`.

## Frozen hypotheses

- H1 terminal self-retrieval >=95% at G4 K256/K1024 — **REFUTED**;
- H2 teacher-forced advancing-hop fidelity thresholds — **REFUTED**;
- H3 free-running fidelity <= teacher-forced fidelity — HOLDS;
- H4 exact semantic state implies exact reprojection key — HOLDS;
- H5 >=80% first-divergence coverage on failed L2/L3 chains — HOLDS;
- H6 accumulated-drift condition — **NOT_APPLICABLE**.

H6 is not applicable because teacher-forced h1/h2 fidelity at K1024,G4 is only
56.25% and 50.00%, far below its >=90% antecedent.

## Decision

The next mechanism search should move **inside the associative memory
interference geometry**, not further downstream into recurrence control.

The next useful diagnostic should separate:

1. interference from other writes in the same chain, where M/R are shared and V
   changes;
2. interference from writes belonging to other chains;
3. self-write contribution versus summed cross-talk at the failed terminal
   states;
4. key dot/cosine structure for same-chain and cross-chain pairs;
5. the effect of write count/order without changing the projector.

This should be done before any new field weight, key-width sweep or controller
rule. N4 already showed that larger K does not remove the low-load L2/L3
pattern, and the source DKV-4j value-weight sweep was negative.

No core projector, controller, context or model promotion follows.
