# DKV-4b — paired K=64/K=256 observation

Status: Development observation under the frozen protocol in
`SML-DKV-4B-DIMENSION-PREREGISTRATION.md`.

Protocol commit: `71fc497affd9ae54d11a43cc4c07844078bc6ead`.
Executed code: `1cfb395485765fdb30a8e447554aab71213ac9a1`.
RemoteOps run: `37190108162`.
Host: `tarek`, aarch64. Rust: 1.89.0.
Rows: 2,048. Two explicit runs were byte-identical.
Output SHA-256: `16ea5242386a5fd4218abef6636ae8c2a9d617a8b28ee5fd576869814629b1d5`.

The retained evidence is in `research/dkv-4b/`. No threshold, seed, query
family, write order or classification rule was changed after observation.

## 1. Unknown-state safety is supplied by provenance, not by Delta-KV

There are 896 unknown-state queries per K:
six NEAR families plus INDEPENDENT, 128 queries per family.

With provenance explicitly marked `Missing`, DKV-4 short-circuited before the
matrix on every one of those queries:

| K | unknown queries | matrix reads | StopCandidate |
|---:|---:|---:|---:|
| 64 | 896 | 0 | 0 |
| 256 | 896 | 0 | 0 |

This is a controller/provenance invariant. It is not evidence that the matrix
recognizes an unknown state.

The paired research-only `Unverified` arm shows the opposite. It never permits
`StopCandidate`, but it deliberately reads the matrix to expose latent behavior.
At least one of the two output coordinates was resolved on **896/896 unknown
queries at both K=64 and K=256**.

Complete two-coordinate latent resolution was:

| query family | K=64 | K=256 |
|---|---:|---:|
| independent | 118/128 | 108/128 |
| near-1 | 128/128 | 128/128 |
| near-2 | 125/128 | 128/128 |
| near-4 | 127/128 | 128/128 |
| near-8 | 126/128 | 128/128 |
| near-16 | 125/128 | 128/128 |
| near-32 | 122/128 | 127/128 |
| **all unknowns** | **871/896 (97.21%)** | **875/896 (97.66%)** |

A resolved latent value is not an accepted answer. These results show that the
existing DKV-2b magnitude/ambiguity mechanism is not an unknown-state detector
on this panel.

This experiment therefore does not reproduce a 1% versus 13% false-acceptance
result. Its production-style acceptance path gives zero StopCandidate decisions
for Missing provenance at both dimensions, while its unverified latent diagnostic
resolves at least one output coordinate in every unknown query at both dimensions.
Those are different definitions and must not be conflated.

## 2. DKV-4a is over-conservative on known states

There are 128 written-state queries at each K.

| metric | K=64 | K=256 |
|---|---:|---:|
| final bits correct, DKV-4a | 125/128 | 128/128 |
| final bits correct, fixed depth 1 | 125/128 | 128/128 |
| final bits correct, fixed depth 4 | 125/128 | 128/128 |
| final bits correct, SBG stability-only | 125/128 | 128/128 |
| DKV-4a StopCandidate | 11/128 | 10/128 |
| DKV-4a Abstain | 117/128 | 118/128 |
| DKV-4a total recurrent cycles | 490 | 491 |
| SBG stability-only total cycles | 251 | 250 |

DKV-4a therefore produced no correctness improvement over any listed baseline
on these written-state fixtures. Its mean depth was 3.83 cycles at both K,
versus about 1.95 cycles for the existing stability-only control. In logical
cycle count, DKV-4a used about 1.95x the recurrent cycles of that control.

The low StopCandidate coverage is not evidence of safer semantic reasoning by
itself. Most written-state final bit pairs are already correct, but the controller
continues to refine and commonly reaches its hard budget before abstaining.

The likely mechanism is visible in the frozen controller definition: absolute
Q12 distance between the matrix proposal and the current Boolean encoding can
remain above 128 even when the Boolean state no longer changes. This is an
interpretation of the implemented signal path, not a retuned result. The current
absolute-correction rule should not be promoted from this panel.

## 3. K=256 buys written accuracy here at substantially greater logical cost

For this exact synthetic panel, K=256 had 128/128 correct written outputs versus
125/128 at K=64 across all compared recurrent controls. This is a three-query
difference on 128 repeated synthetic fixtures and is not a general scaling law.

Per seed, 16 full-rate writes required:

| metric | K=64 | K=256 |
|---|---:|---:|
| ingest prediction MACs | 2,048 | 8,192 |
| weight updates | 2,048 | 8,192 |
| logical matrix + metadata bits | 2,240 | 8,384 |

The query MAC count also scales fourfold per recurrent read because value width
is fixed at two while K grows fourfold. The controller itself has no K input;
its nearly identical cycle counts at the two dimensions do not compensate for
the per-cycle increase.

No wall-clock latency, RSS or energy conclusion follows from logical MAC counts.

## 4. Decision

DKV-4a is retained as a useful fail-closed control-plane reference, especially
for the invariant that Missing/Conflict provenance abstains before any matrix
read. It is **not promoted** as an efficient adaptive-compute policy.

The next candidate should be separately preregistered and should remove absolute
analog residual magnitude as an unconditional reason to continue when the
observable semantic state is already stable. A reasonable next arm is a
**semantic-impact controller**:

- provenance remains the first gate;
- ambiguity, actual V/M/A/R state change and MOR escalation remain refinement signals;
- correction magnitude is reported, but cannot by itself force another cycle
  after the memory proposal leaves the discrete semantic state unchanged;
- compare against unchanged DKV-4a, stability-only, fixed-depth 1 and fixed-depth 4;
- do not alter DKV-4a retrospectively.

Unknown-state latent resolution remains a separate diagnostic. It must not be
converted into acceptance, confidence or provenance.

## Non-claims

The panel uses deterministic synthetic states and a fixed one-page MOR control.
It does not establish learned-language behavior, optimal K, latency, RSS, long
context quality or 25B scaling.

**Scientific/model promotion remains BLOCKED.**
