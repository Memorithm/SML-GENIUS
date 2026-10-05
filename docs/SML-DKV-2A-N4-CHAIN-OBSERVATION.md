# DKV-2a-N4 — combined candidate recurrent-chain observation

Status: current-stack Development replay under
`SML-DKV-2A-N4-CHAIN-REPLAY.md`.

## Execution identity

Owner-supplied input:

- `sml-dkv-0007-dkv-4i-4j.diff`;
- 84,045 bytes;
- 1,738 lines;
- SHA-256 `58bab302cba162ada16468314aa8caf07c7a22c8c3cb553ed9b03e91b8014135`.

Current-stack replay:

- protocol merged before execution in PR #170;
- protocol merge: `2482585fbeda43ae999daee97ca5c6e20082e8d6`;
- executed code head: `0e372102ab68118d6ec8652e447f48228fc1c50a`;
- GitHub Actions run: `37270311294`;
- job: `111635704678`;
- runner: `tarek-scirust-arm64-01`;
- Rust 1.89.0;
- two release-mode executions were byte-identical;
- retained output: `research/dkv-2a-n4/CURRENT-REPLAY.csv`;
- manifest: `research/dkv-2a-n4/MANIFEST.json`.

The source diff already contained Rust 1.97 outcomes. N4 is therefore a
current-library replay rather than an independent blind replication.

No core projector option from the source diff is used. The combined arm is
implemented exclusively as current `ActiveOnly` plus the N3B outer
PageId(5)-derived seed.

## Default control

The historical default cells reproduce exactly at retained precision:

| K | G | F6 |
|---:|---:|---:|
| 64 | 4 | 60.94% |
| 256 | 4 | 75.00% |
| 1024 | 4 | 75.00% |
| 64 | 16 | 17.19% |
| 256 | 16 | 21.48% |
| 1024 | 16 | 23.05% |

This establishes that the current Rust 1.89 adaptation preserves the source
chain panel before interpreting the combined arm.

## Combined projector result

Pooled fixed-depth-6 accuracy:

| K | G | Default | Combined |
|---:|---:|---:|---:|
| 64 | 4 | 60.94% | 82.81% |
| 256 | 4 | 75.00% | 82.81% |
| 1024 | 4 | 75.00% | 82.81% |
| 64 | 16 | 17.19% | 31.25% |
| 256 | 16 | 21.48% | 62.11% |
| 1024 | 16 | 23.05% | 71.48% |

The largest absolute gain is at K=1024,G=16: **+48.44 percentage points**.

The combined candidate is no worse than default in every frozen cell and
reaches the preregistered >=70% K1024/G16 target.

## Capability gate still fails

No combined cell is capable under the historical requirement that ORACLE be
correct on at least 90% of each non-zero hop class L1/L2/L3.

At G=4, all three widths have the same ORACLE pattern:

- L0: 16/16;
- L1: 16/16;
- L2: 13/16;
- L3: 8/16.

Pooled ORACLE is therefore 82.81%, below the 90% low-load target, independent
of K from 64 through 1024.

At K=1024,G=16:

- L0: 50/64;
- L1: 64/64;
- L2: 37/64;
- L3: 34/64;
- pooled ORACLE: 72.27%.

The source diff's central qualitative result therefore reproduces: the
projection correction removes much of the old capacity failure, but the
remaining error has a hop-length shape that widening K does not fix.

This observation does **not** identify the remaining cause.

## Historical controller behavior

STAB reaches the F6 accuracy ceiling in every combined cell.

Mean STAB cycles versus fixed depth 6:

- K64,G4: 2.59;
- K256,G4: 2.53;
- K1024,G4: 2.58;
- K64,G16: 4.74;
- K256,G16: 3.42;
- K1024,G16: 3.11.

KEY, AMB and MAG do not dominate STAB under the historical dominance rule.

This is descriptive compatibility evidence only. N4 does not replace or
promote the repository's later DKV-4 adaptive-control programme.

## Ambiguity is still not a qualified miss signal

Combined G=4 novel-query ambiguity rates at the STAB final state are:

- K64: 44/64 = 68.75%;
- K256: 39/64 = 60.94%;
- K1024: 35/64 = 54.69%.

At least one width therefore remains below the old 70% miss-visibility gate;
in fact all three do.

Written-query clear rates remain high:

- K64: 59/64 = 92.19%;
- K256: 58/64 = 90.63%;
- K1024: 56/64 = 87.50%.

Ambiguity is correlated with misses on this fixture but is not a reliable
presence/miss authority.

## Imported hypotheses

All N4 replay hypotheses hold:

- H0 default control — HOLDS;
- H1 combined no worse — HOLDS;
- H2 K1024/G16 F6 >=70% — HOLDS;
- H3 low-load ORACLE remains below 90% — HOLDS;
- H4 STAB matches F6 — HOLDS;
- H5 no KEY/AMB/MAG dominance — HOLDS;
- H6 ambiguity miss gate fails in at least one width — HOLDS.

No combined cell passes the historical capability gate.

## Value-field weighting from the source diff

The supplied DKV-4j source result is retained as negative evidence and was not
turned into a current core API.

Its reported w={1,2,4,8} value-field sweep improves pooled accuracy by at most
about 1.6 points and never creates a capable cell. The source's own H1/H2/H4
are refuted.

Therefore N4 does not add `with_value_weight`, and the remaining L2/L3 failure
must not be attributed to insufficient V weight without new evidence.

## Decision

**Retain ActiveOnly + PageSeeded as the strongest current DKV-2a projection
candidate, but do not promote it into the core yet.**

N4 materially improves recurrent-chain capability while proving that projection
repair alone is insufficient for multi-hop success.

The next useful investigation is not another key-width or field-weight sweep.
It should localize the L2/L3 failure inside the recurrent transition itself,
for example with a separately frozen per-hop fidelity probe that compares each
read/rewrite against the true next chain node.

Before any core PageSeeded API, the N3 gates still apply:

1. canonical multi-expert MOR semantics;
2. unbalanced page populations and explicit cross-page-sharing semantics;
3. DKV-3a exact record/subject/provenance requalification.

No context/model promotion follows.
