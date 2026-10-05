# DKV-2a-N7 — within-page value-domain observation

Status: current-stack Development experiment under
`SML-DKV-2A-N7-VALUE-DOMAIN-PREREGISTRATION.md`.

## Execution identity

- protocol merged before execution in PR #176;
- current-stack execution head: `0b584f604f52f987b4b90cc9fcffee50d381b2e2`;
- GitHub Actions run: `37281920659`;
- job: `111671731941`;
- runner: `tarek-scirust-arm64-01`;
- Rust 1.89.0;
- two release-mode executions were byte-identical;
- retained output: `research/dkv-2a-n7/CURRENT-REPLAY.csv`;
- manifest: `research/dkv-2a-n7/MANIFEST.json`.

No core projector or Delta-KV memory implementation changed. Both arms call the
current `DeltaKvSemanticProjector`; N7 changes only the outer seed domain in
the harness.

## PAGE control reproduces N6

At K=1024, the PAGE arm reproduces the preregistered N6 terminal counts exactly.

SELF_SAME:

- L0 16/16;
- L1 16/16;
- L2 14/16;
- L3 8/16.

FULL:

- L0 16/16;
- L1 16/16;
- L2 13/16;
- L3 8/16.

The same-chain key geometry also reproduces N6:

- L1 mean absolute cosine 0.680492;
- L2 0.675859;
- L3 0.683404.

H1 therefore holds.

## Exact-V domain removes the correlated geometry

At K=1024, PAGE_VALUE same-chain mean absolute key cosine becomes:

- L1: 0.023413;
- L2: 0.026101;
- L3: 0.024551.

For comparison, PAGE is approximately 0.68 in all three classes.

The PAGE_VALUE same-chain values are now in the same range as unrelated
cross-chain keys. No exact projected-key collision occurs.

H2 and H8 hold.

## Low-load associative capability is completely restored

At K=1024/G=4, PAGE_VALUE terminal recall is 16/16 exact+clear for every
L0/L1/L2/L3 class under SELF, SELF_SAME and FULL.

Teacher-forced advancing reads are also perfect:

- h0: 48/48;
- h1: 32/32;
- h2: 16/16.

The result is not dependent on K=1024 alone. At K=256/G=4, PAGE_VALUE also
returns every terminal and advancing target exactly and clearly.

H3 and H4 hold.

## Six-cycle recurrent capability reaches 100% on the frozen panel

PAGE_VALUE fixed-depth results:

| K | G | F1 | F2 | F3 | F6 | ORACLE |
|---:|---:|---:|---:|---:|---:|---:|
| 256 | 4 | 50.0% | 75.0% | 100% | 100% | 100% |
| 1024 | 4 | 50.0% | 75.0% | 100% | 100% | 100% |
| 256 | 16 | 42.58% | 61.72% | 75.0% | 73.44% | 80.47% |
| 1024 | 16 | 50.0% | 75.0% | 100% | 100% | 100% |

At K=1024/G=16 every advancing and terminal teacher-forced read is 100%
exact+clear, and every chain reaches the exact terminal by cycle 3.

This removes the N4/N5/N6 K1024 capability failure on the synthetic chain
fixture.

H5 holds.

## The mechanism is maximally brittle to small V error

The capability gain comes with a severe continuity cost.

At K=1024, one V-bit perturbation of an exact written non-terminal source gives:

- PAGE: mean |cosine| = 0.989593;
- PAGE_VALUE: mean |cosine| = 0.023576.

PAGE_VALUE exact next-value retrieval under the perturbed query is:

- h=1: 0/96;
- h=2: 0/96;
- h=4: 0/96.

The same candidate therefore behaves almost like a categorical content address:
a single wrong V bit changes the projection domain enough to make the query
nearly orthogonal to the exact write.

This is not a small robustness regression; it is the dominant negative result
of N7.

H6 holds, and by preregistration H6 is **negative evidence against direct
promotion**.

## Determinism and collisions

Repeated projection of every written state produces identical key and counters.
No PAGE_VALUE written-state collision occurs in any K/load cell.

H7 and H8 hold.

## Frozen hypotheses

All eight N7 hypotheses hold:

- H1 PAGE control — HOLDS;
- H2 value-domain orthogonalization — HOLDS;
- H3 SELF_SAME capability restoration — HOLDS;
- H4 teacher-forced capability — HOLDS;
- H5 free-running capability — HOLDS;
- H6 one-bit brittleness — HOLDS (**negative promotion evidence**);
- H7 determinism — HOLDS;
- H8 no written-state collisions — HOLDS.

## Mechanistic conclusion

N7 supplies a strong causal result:

**the N4/N5/N6 failure can be removed by breaking the shared projection domain
between successive V states inside one page.**

That supports N6's crosstalk diagnosis.

But exact-V domain conditioning is not a viable direct runtime policy because it
destroys near-state continuity. A single V error moves the query to a different
hash family before Delta-KV can use similarity to recover.

## Decision

**Do not promote PAGE_VALUE into the core.**

Retain it as a successful upper-bound / mechanism-isolation control.

The next Development candidate should interpolate between PAGE and PAGE_VALUE,
seeking enough within-chain decorrelation to remove crosstalk while preserving
meaningful continuity under one-bit V perturbations.

A natural bounded follow-up is a fixed-total-width hybrid key whose coordinates
are deterministically partitioned between:

- the stable PAGE projection family; and
- the exact-V PAGE_VALUE projection family.

Such a panel can sweep a small preregistered set of PAGE_VALUE fractions while
keeping total K fixed. It should jointly measure:

1. same-chain cosine;
2. terminal/teacher/free-running capability;
3. one-bit perturbed-query cosine and recall;
4. total projection work and logical key width.

No core API, controller, context or model promotion follows from N7.
