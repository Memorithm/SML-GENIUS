# DKV-2a-N1 — current-stack ambiguity panel execution protocol

Status: frozen before current-stack harness implementation or execution.

Parent protocol:
`SML-DKV-2A-N1-AMBIGUITY-NEUTRAL-PREREGISTRATION.md`.

## New owner-supplied input

Input: `sml-dkv-0004-dkv-4d.patch`.

- bytes: 33,038;
- lines: 593;
- SHA-256: `4d3e2b2fd6db9a884c72e0d20f8a8597e8a9b5f495250c421bdb362fd2cf002a`.

The supplied patch independently implements a harness-local `SparseA` fold
whose semantics match the now-merged `DeltaKvAmbiguityProjection::ActiveOnly`
candidate: ambiguity bit true contributes the existing signed projection term,
ambiguity bit false contributes no bucket update. The supplied harness reports
Rust 1.97 results for zero, two-bit sparse and dense-random ambiguity, plus
near-miss identical-key rates.

Those numbers remain external input observations. This protocol does not copy
the patch's local key implementation and does not treat its Rust 1.97 run as
current repository qualification.

## Purpose

Execute the already-preregistered DKV-2a-N1 panel against the actual current
library implementation under the workspace Rust version.

The comparison is:

- qualified/default `Bipolar`;
- research candidate `ActiveOnly`.

No local duplicate of the projection hash/bucket/sign algorithm is permitted in
the execution path. Both arms must call `DeltaKvSemanticProjector` directly.

## Frozen environment and grid

- Rust: 1.89.0;
- `SemanticState<64>`;
- key widths K = 64, 256, 1024;
- write loads W = 1, 8, 16, 32, 64;
- deterministic seeds = 0..15;
- projector seed = `seed * 1000 + 17`;
- semantic weight = 1;
- MOR weight = 8;
- native `MorDecision` with one selected PageId(5);
- value width = 64;
- full-rate Q12 writes;
- strict read threshold: every output coordinate has absolute magnitude > 128.

Ambiguity distributions:

- ZERO: all A bits false;
- SPARSE: exactly two distinct A bits true, selected deterministically from the
  same per-row stream before projection;
- DENSE_CONTROL: every A bit is independently supplied by one deterministic
  64-bit stream word, yielding the balanced/dense mechanism control used by the
  supplied patch.

The SPARSE definition above is a pre-execution clarification supplied by the
new patch. It does not alter any observed current-stack result because this
document precedes the current-stack harness and run.

## Geometry panel

For every K, arm and ambiguity distribution, generate 200 independent state
pairs per seed and report the mean cosine of their Q12 keys. Floating point is
confined to this research harness; no floating-point dependency enters the
SML core or projector.

The geometry report must additionally retain projector counters from the actual
library call so ActiveOnly work reduction is not inferred from key shape alone.

## Recall panel

For every K, arm, ambiguity distribution, load and seed:

1. generate the declared number of states and independent 64-bit target values;
2. deterministically shuffle write order;
3. write each key/value at full Q12 rate into a fresh DeltaKvMemory;
4. reread every written key;
5. report:
   - facts;
   - strict-correct reads;
   - sign-correct reads;
   - total wrong output coordinates;
   - matrix reads, read MACs, updates, weight updates and saturations;
   - exact matrix logical bits.

No result from one arm changes the other arm's stream, load, write order,
threshold or target.

## Near-miss alias panel

Use ZERO ambiguity only.

For every K, arm and seed:

- generate 40 base states;
- for h in {1,2,4,8}, flip h distinct coordinates drawn only from V/M/R;
- compare the candidate key with the base key for exact equality;
- report identical-key count and total queries.

This is a key-alias diagnostic, not a source-authority test.

## Compatibility control

For 200 DENSE_CONTROL states per K and seed, compare:

- legacy `DeltaKvSemanticProjector::new(...)`;
- explicit `new_with_ambiguity_projection(..., Bipolar)`.

Every key, selected-expert list, provenance value, counter field and
transient-key bit count must be equal. Any mismatch invalidates the run.

## Hypotheses carried from the parent protocol

The current-stack execution evaluates, without retuning:

- H1: legacy and explicit Bipolar remain identical;
- H2: ZERO ActiveOnly reads all A coordinates while omitting false-A bucket
  accumulations;
- H3: ZERO ActiveOnly mean cosine is lower than Bipolar at every K;
- H4: ZERO ActiveOnly strict recall exceeds Bipolar at W=16 for K=256 and 1024;
- H5: finite-capacity degradation at W=32/64 is retained, not hidden;
- H6: SPARSE behavior is measured independently of ZERO and DENSE_CONTROL;
- H7: source authority and acceptance semantics remain unchanged.

Additional patch-derived diagnostic, frozen before this run:

- H8: ActiveOnly exact near-miss key-alias rate must not exceed Bipolar by more
  than 2 percentage points in any (K,h) cell before it can proceed to the
  record/subject requalification stage.

Dense-control behavior is descriptive. The supplied patch predicts a recall
cost for ActiveOnly under dense ambiguity, but no current-stack threshold is
introduced from that observed Rust 1.97 result.

## Evidence retention

The CI evidence step must execute the panel twice, require byte-identical
output, and print the retained first output. A later observation commit may
copy that exact output/hash and interpret it, but may not alter this protocol,
grid or implementation after seeing results.

No projector promotion, context promotion or model-quality claim follows from
the panel alone.
