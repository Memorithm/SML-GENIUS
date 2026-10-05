# DKV-2a-N3 — owner input review: hypothesis ambiguity and page conditioning

Input: `sml-dkv-0006-dkv-4f-4g-4h.diff`.

- bytes: 69,014;
- lines: 1,363;
- SHA-256: `8b201dbde1a5655fc049536c1457f5f6ed3a352eb0001c7d38ffe803344a9664`.

The supplied diff contains three Rust 1.97 Development studies labelled
DKV-4f/4g/4h in the source material. Those labels are not reused in current
SML-GENIUS because the repository already has an independent DKV-4 adaptive
control programme. The durable findings are routed to DKV-2a as N3.

## What is retained

### N3-A — ambiguity produced by the actual SBG ambiguity function

The supplied DKV-4f harness calls the repository's own `derive_ambiguity`
from noisy competing Boolean hypotheses. This is more informative than the
pure popcount sweep in N2 because it binds A density to the actual ambiguity
rule.

The supplied Rust 1.97 observation reports a crossover: ActiveOnly/SparseA is
strong when mean A weight is low, the qualified Bipolar fold becomes stronger
when ambiguity is dense, and all tested cells with mean A weight <=16 preserve
the ActiveOnly candidate within the frozen margin.

This is still a synthetic independent-bit noise model. It is not a
measurement of representative SBG states and cannot close the real-distribution
promotion gate. A current Rust 1.89 replay is nevertheless useful because it
checks the exact current `derive_ambiguity` + `ActiveOnly` composition.

### N3-B — PageSeeded projection

N2 established on current Rust 1.89 that the existing additive one-bucket
PageId term is not an isolating memory namespace.

The supplied DKV-4g experiment tried the obvious additive repair: spread each
page over 1/8/32/128 additive MOR buckets. Its reported result is negative:
wrong-page leakage falls only when page mass dominates the key, while same-page
overlap rises and page-conflict recall remains at or below about 50%.

That additive-spread mechanism is **not imported as a library candidate**.

The supplied DKV-4h experiment tests a structurally different mechanism:
condition the whole projection hash family on the selected page. Its Rust 1.97
observation reports 0% wrong-page recall and 100% page-conflict resolution on
the synthetic one-page panel at K=256/1024.

This mechanism can be replayed without copying the projector implementation:
for a single selected page, derive a page-specific projector seed in the
harness and then call the existing
`DeltaKvSemanticProjector::new_with_ambiguity_projection(..., ActiveOnly)`.
The current core remains unchanged.

## Explicitly rejected imports

- no `new_sparse_ambiguity` constructor: current `ActiveOnly` already covers
  the useful A semantics;
- no harness-local copy of DKV-2a field hashing;
- no additive PageSpread API from the supplied DKV-4g code;
- no promotion from the supplied Rust 1.97 outcomes;
- no assumption that page isolation is always desired;
- no mapping for multi-expert MOR decisions until separately designed.

## Current-stack next gates

1. replay the hypothesis-derived ambiguity boundary under Rust 1.89;
2. replay PageSeeded through the existing projector API under Rust 1.89;
3. preserve the current additive ActiveOnly path as the exact control;
4. after PageSeeded replay, require multi-expert semantics, unbalanced page
   distributions and DKV-3a record/subject alias controls before any core API;
5. keep context/model promotion blocked.
