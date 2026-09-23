# DKV-2c dynamic re-keying preregistration

Status: preregistered mechanism experiment. No result, winner, model-quality claim, or runtime promotion is made by this document.

## Research question

DKV-2b freezes one DKV-2a Q12 query key across recurrent cycles. DKV-2c changes exactly one mechanism: after each accepted semantic rewrite, recompute the next Delta-KV query key from the updated SBG state while preserving the already-produced MOR selection.

Question: can deterministic dynamic re-keying follow a bounded chain of associative states without introducing an additional router, and what extra projection/read work and ambiguity does it incur relative to the frozen-key DKV-2b path?

## Frozen mechanism

The experiment MUST use:

- `SemanticState<8>`;
- DKV key dimension: 8;
- Delta-KV value dimension: 2;
- value coordinates: `[0, 1]`;
- DKV Q12 scale and normalized delta update already merged in DKV-0a;
- DKV-2a projector seed: 17;
- projection policy: `semantic_weight=1`, `mor_weight=32`;
- frozen MOR selected experts: exactly `[PageId(5)]`;
- no second MOR/router invocation during recurrence;
- recurrent ambiguity threshold: 128 Q12;
- adaptive policy: min depth 1, max depth 4, stable Hamming threshold 0;
- matched fixed-depth control: depth 4;
- full-rate Q12 writes when seeding fixture associations;
- raw/source provenance excluded from key mathematics.

Dynamic re-keying MUST call the same DKV-2a projector on the current SBG state before each memory read. The frozen-key arm MUST reuse only the initial projected key.

## Frozen fixture panel

Four deterministic fixture classes are frozen before implementation.

### F0 — single association

- initial semantic state is the existing DKV-2b reference state;
- seed only the initial projected key with target V bits `[true, false]`;
- purpose: expose whether dynamic re-keying leaves a known one-step case stable or moves into an unseeded key.

### F1 — two-state chain

- derive `K0` from the initial state;
- apply target `[true, false]` to derive state `S1`;
- derive `K1` from `S1` using the same frozen MOR decision;
- seed `K0 -> [true, false]`;
- seed `K1 -> [true, true]`;
- final declared chain target: V coordinates `[true, true]`.

### F2 — reversal chain

- derive `K0` from the initial state;
- derive `K1` after applying `[true, false]`;
- seed `K0 -> [true, false]`;
- seed `K1 -> [false, true]`;
- final declared chain target: V coordinates `[false, true]`.

### F3 — missing successor

- derive and seed only `K0 -> [true, false]`;
- do not seed the dynamically reprojected successor key;
- purpose: require explicit ambiguity/max-depth behavior rather than silently treating a missing dynamic association as success.

If a derived fixture key unexpectedly equals a preceding key, the fixture MUST fail closed rather than silently changing the seed, projector, state, or target.

## Frozen arms

For every fixture evaluate:

1. `frozen_adaptive`: merged DKV-2b frozen-key adaptive recurrence;
2. `dynamic_adaptive`: reproject current SBG state before every Delta-KV read;
3. `dynamic_fixed4`: same dynamic re-keying at exactly four recurrent cycles;
4. `no_delta`: identity recurrence with the same adaptive halting policy.

No arm may change MOR selection or learn/write new associations during the measured recurrent run.

## Required evidence

For each fixture and arm record:

- final V bits at coordinates 0 and 1;
- exact-match flag against the fixture's declared final target where a target exists;
- recurrent depth and halt reason;
- per-cycle key Q12 vector identity/digest;
- number of key projections;
- DKV semantic projection counters summed across projections;
- DKV read count and read MACs;
- ambiguity count per cycle;
- whether the key changed from the previous cycle;
- total distinct keys visited;
- seeded DKV update count and saturation count, reported separately from measured reads;
- no-Delta-KV control state delta.

All arithmetic remains integer/fixed-point. No wall-clock, throughput, RAM, or energy claim is permitted from operation counts.

## Exploratory interpretation

No overall winner is preregistered.

A useful dynamic mechanism must at minimum demonstrate that:

- F1 or F2 can visit a successor key distinct from K0;
- the dynamic path can consume the pre-seeded successor association;
- F3 fails explicitly through ambiguity/max-depth or another declared fail-closed state rather than fabricating a confident value;
- projection/read work is accounted separately from frozen-key recurrence.

If the dynamic arm is worse on F0/F3, preserve that negative evidence. Do not modify projector seed, weights, targets, thresholds, or fixtures after observation.

## Promotion boundary

DKV-2c is complete only after its observation is preserved without retuning.

Even a favorable result does not establish long-context capability. It only permits DKV-3 to use the qualified memory/recurrent mechanisms in an explicit SML-CONTEXT experiment with raw provenance retained.
