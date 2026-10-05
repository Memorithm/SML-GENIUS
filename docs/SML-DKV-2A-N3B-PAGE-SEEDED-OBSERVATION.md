# DKV-2a-N3B — page-conditioned projector observation

Status: Development replay under
`SML-DKV-2A-N3B-PAGE-SEEDED-REPLAY.md`.

## Execution identity

Current-stack replay:

- source input: `sml-dkv-0006-dkv-4f-4g-4h.diff`, SHA-256
  `8b201dbde1a5655fc049536c1457f5f6ed3a352eb0001c7d38ffe803344a9664`;
- protocol merged before implementation in PR #168;
- executed code head: `eb0938627c5f02eb4844b43b202911a76fea66a5`;
- GitHub Actions run: `37268594839`;
- job: `111630572185`;
- Rust 1.89.0;
- two release-mode executions were byte-identical;
- retained output: `research/dkv-2a-n3/N3B.csv`.

The candidate does not copy the DKV-2a projector. It derives one outer seed
from PageId and calls the existing ActiveOnly projector unchanged.

## Result

| K | arm | own-page recall | wrong-page recall | page-conflict E2 | independent states / same page cosine | same state / different pages cosine |
|---:|---|---:|---:|---:|---:|---:|
| 256 | Additive | 94.14% | 92.97% | 47.27% | 0.007837 | 0.987623 |
| 256 | PageSeeded | 100% | 0% | 100% | 0.008137 | -0.000981 |
| 1024 | Additive | 98.44% | 98.44% | 49.61% | 0.005005 | 0.992988 |
| 1024 | PageSeeded | 100% | 0% | 100% | 0.004711 | -0.000299 |

The Additive control exactly reproduces N2 at K=1024.

PageSeeded makes the same state under two pages approximately orthogonal,
eliminates wrong-page retrieval in the frozen panel, and resolves every
same-state/two-page value conflict.

## Work accounting

For matched K, both arms execute exactly the same current-projector logical
work.

At each arm over the complete panel:

- 10,880 projections;
- 2,785,280 semantic-coordinate reads;
- 2,099,840 bucket accumulations;
- equal quantized-coordinate counts for the same K.

The only new operation outside these counters is deterministic derivation of
the outer page-specific seed. No physical latency/RSS/energy claim follows.

## Frozen hypotheses

- H0 Additive control — HOLDS;
- H1 page isolation — HOLDS;
- H2 page-conflict resolution — HOLDS;
- H3 own-page recall within +/-3 points — **REFUTED as written**;
- H4 geometry — HOLDS;
- matched projector work counters — HOLDS.

H3 is refuted because PageSeeded is **better**, not because it lost recall:
at K=256 own-page recall rises from 94.14% to 100%, a +5.86 point change outside
the preregistered two-sided tolerance. The result is retained as a refutation.

## Relation to additive PageSpread

The supplied diff also reports an additive PageSpread control. Spreading a
page-specific additive vector over more buckets reduces wrong-page leakage only
by letting page mass dominate the key; it raises overlap among all states in the
same page and does not resolve the two-page conflict.

N3B reproduces the structurally different solution: condition the **entire**
projection hash family on page identity. It separates page domains without
increasing current projector accumulation counts on this one-page panel.

## Decision

**PageSeeded is retained as the strongest DKV-2a page-isolation candidate so
far, but it is not promoted into the core.**

Three gates remain mandatory:

1. define and preregister canonical behavior for MOR decisions containing more
   than one selected expert;
2. test unbalanced page populations and decide whether strict page isolation is
   semantically desired versus cross-page sharing/generalization;
3. replay DKV-3a exact record/subject alias and provenance controls under the
   page-conditioned key.

A later core API should only follow those gates. A likely implementation surface
would be an explicit projection-domain policy rather than silently changing the
meaning of the existing seed or PageId term.

No context/model promotion follows.
