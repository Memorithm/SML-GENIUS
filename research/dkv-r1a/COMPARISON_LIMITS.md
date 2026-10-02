# Candidate A: residual limits to keep visible during code comparison

This note does not alter the frozen protocol, executed code or retained output.
It clarifies the scope of the counters and experimental construction after review.
No new experiment or favorable result is introduced.

## Setup is not peak live storage

The CSV's `SETUP numeric_matrix_instances=2` counts the persistent numeric world
matrix and the source-adapter shadow matrix. `Kernel::new` additionally clones
the world matrix for each DKV episode; that clone is dropped before the next arm.
Thus the DKV arm has THREE simultaneously live coefficient arrays, not two.
With value dimension 2 and i16 coefficients this is `3 * key_dim * 2 * 2` bytes
of coefficient payload, excluding every container, key, ledger, direct-memory
object, state, trace, artifact and allocation overhead. Direct arms similarly
clone their direct memory. Cached DKV retains its matrix; the measured saving
is read operations, not memory residency.

Source-adapter element plans cover that adapter only, not the whole harness.
The report exports common setup and per-arm query MACs, but is not a complete
allocator/RSS, wall-clock, energy or OS-resource accounting system. Comparisons
that claim those benefits require new complete measurements. Clone/allocation
costs must not be silently amortized away when confronting a different design.

## Source and recurrence budgets are distinct

Source verification is shared diagnostic work outside recurrence. All recorded
sources are parsed and bound to admitted identities; no source-resolved answer
replaces the latent answer. The separate raw-only pass and artifacts are also
harness work, not free production inference. Total costs need both paths.

The latent DKV-2b path is intentionally not gated by a sidecar membership result:
otherwise the missing-key failure mode would be masked. There is no public
Accept decision in this example and a wrong-known latent value is not allowed
to claim source authority. Absence of SBG ambiguity is not calibrated confidence.

## Seed and representation limitations

A seed controls the projector and cycles the declared source payload pattern;
therefore across-seed differences are not solely projector randomness. At a fixed
seed/group, all dimensions and arms share payloads and source order. The eight
families are small synthetic constructions, not eight representative NLP tasks.

Near addresses are distinct before projection but can collide afterwards. The
CSV reports actual query/write dot products and norms and counts exact collisions
between distinct written subjects. It does not export the entire pairwise Gram
matrix or claim constant cosine. Any stronger geometric analysis needs additional
reported geometry without changing the historical result.

## Static read and sequential propagation

The expert is a hand-authored two-stage Boolean circuit using previous-cycle
scratch. Three cycles typically mean memory injection, output propagation and
stability confirmation; they do not demonstrate three steps of learned reasoning.
The output records final states through scores and per-cycle change traces;
fresh/cached tests compare final states and full change traces. Q12 first-read
values are exported and remain constant under the static matrix/frozen key.

Conditional read caching is supported only for that invariant. A counterpart
that updates memory or reprojects the query must implement explicit invalidation
or demonstrate a different contract. No optimization is promoted into sml-core
or another repository on the strength of this tiny panel.

Full context promotion, learned representation quality, sidecar crash durability,
physical resource qualification and independent confirmation remain BLOCKED/open.
