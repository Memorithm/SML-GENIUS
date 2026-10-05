# DKV-2a-N8 — fixed-width PAGE/PAGE_VALUE hybrid observation

Status: current-stack Development experiment under
`SML-DKV-2A-N8-HYBRID-PREREGISTRATION.md`.

## Execution identity

- protocol merged before execution in PR #178;
- protocol merge: `9081fbdefde46119ee86fca4b8a23c6a6eda9831`;
- executed scientific head: `3535c556369c7005c134561598e1cf031659ad1e`;
- GitHub Actions run: `37283646953`;
- job: `111677303446`;
- runner: `tarek-scirust-arm64-01`;
- Rust 1.89.0;
- two release-mode executions were byte-identical;
- retained output: `research/dkv-2a-n8/CURRENT-REPLAY.csv`;
- manifest: `research/dkv-2a-n8/MANIFEST.json`.

No core projector or Delta-KV memory implementation changed.

## Endpoint controls

q=0 reproduces the N7 PAGE control and q=4 reproduces PAGE_VALUE, including:

- K1024/G4 PAGE F6 = 82.8125%;
- K1024/G4 PAGE_VALUE F6 = 100%;
- PAGE_VALUE Hamming-1 mean |cosine| = 0.023576 and exact+clear retrieval = 0/96.

H1 holds.

## Fixed-width tradeoff

At K=1024, the same-chain correlation falls monotonically as the PAGE_VALUE
fraction rises.

Representative L2 mean absolute cosine:

| q | PAGE_VALUE fraction | same-chain | Hamming-1 perturbed |
|---:|---:|---:|---:|
| 0 | 0% | 0.675859 | 0.989593 |
| 1 | 25% | 0.498938 | 0.743212 |
| 2 | 50% | 0.345175 | 0.494567 |
| 3 | 75% | 0.181965 | 0.252594 |
| 4 | 100% | 0.026101 | 0.023576 |

The desired decorrelation and the undesired loss of local continuity move
smoothly in opposite directions. H2 and H3 hold.

## Low-load capability/robustness gate

K=1024/G=4:

| q | F6 | minimum advancing teacher recall | Hamming-1 exact+clear |
|---:|---:|---:|---:|
| 0 | 82.81% | 45.83% | 50.00% |
| 1 | 85.94% | 56.25% | 50.00% |
| 2 | 96.88% | 87.50% | 45.83% |
| 3 | **100%** | **100%** | **26.04%** |
| 4 | 100% | 100% | 0% |

q=2 already restores most chain capability, but fails the frozen >=90% teacher
gate because h0 is 42/48 = 87.5%.

q=3 is the **smallest interior fraction** satisfying all low-load gates:

- F6 64/64 = 100%;
- h0/h1/h2 teacher-forced exact+clear = 48/48, 32/32, 16/16;
- Hamming-1 perturbed exact+clear = 25/96 = 26.04%.

H4 holds and the preregistered low-load selection is q=3.

## High-load gate

K=1024/G=16 fixed-depth F6:

- q=0: 71.48%;
- q=1: 71.88%;
- q=2: 76.17%;
- q=3: **91.80%**;
- q=4: 100%.

q=3 again is the smallest interior fraction that combines:

- F6 >=90%: 235/256 = 91.80%;
- Hamming-1 exact+clear >=20%: 25/96 = 26.04%.

H5 holds and the preregistered high-load selection is also q=3.

The selected fraction is therefore stable across both frozen loads.

## Geometry at the selected q=3

At K=1024, q=3 same-chain mean absolute cosine:

- L1: 0.178782;
- L2: 0.181965;
- L3: 0.169247.

This is materially below PAGE (~0.68) while remaining well above exact
PAGE_VALUE (~0.02). The one-bit perturbed-query cosine is 0.252594 rather than
0.023576, preserving a non-zero similarity path.

## Collisions and work

No exact written-state key collision occurs in any K/q/load cell. H6 holds.

For fixed K, every q performs the same logical source-projection work in the
frozen harness because both PAGE and PAGE_VALUE projections are evaluated before
stitching.

At K=1024 for 16 probe hybrid keys:

- semantic coordinates read: 8,192;
- bucket accumulations: 6,176;
- quantized source coordinates: 32,768;
- final hybrid key width: 1,024.

H7 holds.

This intentionally double-projects even q=0/q=4 to keep the Development cost
comparison matched. It is not a claim that a specialized endpoint runtime
would require the same work.

## Frozen hypotheses

All N8 hypotheses hold:

- H1 endpoint controls — HOLDS;
- H2 same-chain cosine monotonicity — HOLDS;
- H3 Hamming-1 cosine monotonicity — HOLDS;
- H4 low-load interior compromise — HOLDS;
- H5 high-load interior compromise — HOLDS;
- H6 no collisions — HOLDS;
- H7 equal logical projector work — HOLDS.

Selection:

- low load: **q=3**;
- high load: **q=3**.

## Decision

**Retain q=3/4 PAGE_VALUE + 1/4 PAGE as the strongest bounded Development
domain candidate so far.**

Unlike exact PAGE_VALUE, q=3 does not fully destroy near-state recovery, and
unlike PAGE/q<=2 it crosses both frozen capability gates.

It is still not promoted into the core. The candidate is synthetic-fixture
qualified only and still lacks:

1. canonical multi-expert MOR/domain semantics;
2. unbalanced page-population and cross-page sharing/generalization controls;
3. DKV-3a exact record/subject/provenance requalification;
4. physical latency/RSS/traffic/energy measurements;
5. representative recurrent SBG evaluation.

No controller, context or model promotion follows.
