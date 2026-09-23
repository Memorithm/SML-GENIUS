# DKV-2c dynamic re-keying observation

Status: preserved mechanism observation under the preregistered DKV-2c protocol. No post-observation retuning is applied.

## Provenance

- protocol: `sml-dkv-2c-dynamic-rekey/v1`
- preregistration: merged PR #85
- implementation PR: #86
- observed exact head: `433860124d12d4e07d2dcd2cf060e6c71386b4b8`
- implementation merge: `197358865d84f98ed3ae5ad09a50dbc640e430ae`
- CI run: `35857392016`
- CI job: `107169069048`
- evidence harness: `crates/sml-core/examples/dkv_2c_dynamic.rs`
- CI executed the harness twice and required byte-identical CSV output

The projector seed/policy, MOR selected-expert set, Delta-KV geometry, value coordinates, ambiguity threshold, recurrent policies and fixture definitions remained unchanged after preregistration.

## Preserved evidence

| fixture | arm | final V[0:2] | exact target | depth | halt | projections | reads | read MACs | distinct keys | key changes | ambiguity |
|---|---|---|---:|---:|---|---:|---:|---:|---:|---:|---:|
| F0 single | frozen adaptive | 1,0 | yes | 2 | Stable | 1 | 2 | 32 | 1 | 0 | 0 |
| F0 single | dynamic adaptive | 1,0 | yes | 2 | Stable | 2 | 2 | 32 | 2 | 1 | 0 |
| F0 single | dynamic fixed4 | 1,0 | yes | 4 | Stable | 4 | 4 | 64 | 2 | 1 | 0 |
| F0 single | no Delta-KV | 0,0 | no | 1 | Stable | 0 | 0 | 0 | 0 | 0 | 0 |
| F1 chain | frozen adaptive | 1,1 | yes | 2 | Stable | 1 | 2 | 32 | 1 | 0 | 0 |
| F1 chain | dynamic adaptive | 1,1 | yes | 2 | Stable | 2 | 2 | 32 | 2 | 1 | 0 |
| F1 chain | dynamic fixed4 | 1,1 | yes | 4 | Stable | 4 | 4 | 64 | 2 | 1 | 0 |
| F1 chain | no Delta-KV | 0,0 | no | 1 | Stable | 0 | 0 | 0 | 0 | 0 | 0 |
| F2 reversal | frozen adaptive | 0,1 | yes | 2 | Stable | 1 | 2 | 32 | 1 | 0 | 0 |
| F2 reversal | dynamic adaptive | 0,1 | yes | 2 | Stable | 2 | 2 | 32 | 1 | 0 | 0 |
| F2 reversal | dynamic fixed4 | 0,1 | yes | 4 | Stable | 4 | 4 | 64 | 1 | 0 | 0 |
| F2 reversal | no Delta-KV | 0,0 | no | 1 | Stable | 0 | 0 | 0 | 0 | 0 | 0 |
| F3 missing successor | frozen adaptive | 1,0 | n/a | 2 | Stable | 1 | 2 | 32 | 1 | 0 | 0 |
| F3 missing successor | dynamic adaptive | 1,0 | n/a | 2 | Stable | 2 | 2 | 32 | 2 | 1 | 0 |
| F3 missing successor | dynamic fixed4 | 1,0 | n/a | 4 | Stable | 4 | 4 | 64 | 2 | 1 | 0 |
| F3 missing successor | no Delta-KV | 0,0 | n/a | 1 | Stable | 0 | 0 | 0 | 0 | 0 | 0 |

Seed updates were one for F0/F3 and two for F1/F2. No fixture reported storage saturation.

## Interpretation

The dynamic mechanism is executable and its accounting boundary works:

- F0 and F1 visit two distinct query keys under dynamic adaptive recurrence;
- F1 therefore demonstrates that the evolving SBG state can cause a deterministic successor-key lookup without a second MOR invocation;
- dynamic projection work is explicit: two projections / 64 semantic-coordinate reads for the two-cycle adaptive path, versus one initial projection / 32 semantic-coordinate reads for frozen-key;
- matched fixed-depth dynamic recurrence doubles read MACs from 32 to 64 relative to the adaptive two-cycle runs.

However, the preregistered usefulness boundary is not satisfied.

### No observed final-state advantage

For every fixture with a declared target, the frozen-key DKV-2b arm already reaches the same final target at the same adaptive depth as dynamic re-keying.

In F1 the dynamic arm does visit a successor key, but the frozen arm reaches the same final state. The experiment therefore does not isolate a task-quality benefit attributable to dynamic re-keying.

In F2 the measured dynamic path does not change key at all, despite the fixture containing a distinct precomputed successor key. The seeded associative updates alter the effective read trajectory enough that the recurrent state reaches the final reversal target without visiting that successor.

### Missing-successor control fails the safety expectation

F3 is the decisive negative result.

The dynamic arm changes from the initial key to an unseeded successor key, yet:

- reports zero ambiguous values;
- halts `Stable` at depth 2;
- emits a confident V state.

The preregistration required an unseeded successor to surface explicit ambiguity/max-depth or another declared fail-closed state rather than silently look confident. That requirement fails.

This is consistent with an associative matrix: an unseeded key may still produce a non-zero response through overlap/generalization. For the intended SML memory semantics, that behavior cannot currently be interpreted as an explicit miss.

## Decision

**DKV-2c dynamic re-keying is NOT promoted into the DKV-3 context path.**

The implementation remains a research surface because it demonstrates deterministic state-dependent re-projection and may be useful in a later experiment with an independently qualified confidence/miss mechanism.

For DKV-3:

- use the merged DKV-2b frozen-key recurrent path as the qualified Delta-KV recurrence mechanism;
- retain direct `BoundedAssociativeMemory` independently;
- retain exact raw/source provenance outside Delta-KV;
- do not use dynamic re-keying as an authoritative retrieval mechanism;
- do not infer a memory miss from Delta-KV magnitude until a separate miss/confidence protocol is preregistered and qualified.

## Non-claims

This observation does not establish that frozen-key recurrence is optimal, nor that dynamic re-keying is generally harmful. It establishes only that this preregistered DKV-2c mechanism did not satisfy its own promotion boundary on the frozen fixtures.

No language-model quality, long-context, latency, throughput, RAM, energy or scaling claim follows.
