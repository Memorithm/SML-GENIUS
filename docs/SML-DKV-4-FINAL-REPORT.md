# DKV-4 — development completion report

Status: ten-slice engineering/development programme; scientific/model promotion remains BLOCKED.

This report closes the owner-requested DKV-4 implementation batch. It records
what the current evidence supports and what it does not support. It does not
overwrite DKV-3 evidence or promote Delta-KV into the authoritative context path.

## Implemented slices

The first nine DKV-4 slices were merged through:

1. PR #132 — conservative DKV-4a typed compute controller;
2. PR #133 — executed DKV-2b/SBG/MOR signal adapter;
3. PR #134 — bounded adaptive recurrent runner;
4. PR #135 — prospective paired K=64/K=256 protocol;
5. PR #137 — deterministic paired dimension panel;
6. PR #138 — immutable retained K64/K256 evidence;
7. PR #139 — dimension observation without retuning;
8. PR #143 — preregistered DKV-4c semantic-impact controller;
9. PR #144 — retained DKV-4c comparison and observation;
10. PR #149 — final report plus bootstrap/roadmap synchronization.

PR #149 is the tenth slice. Its merge identity is intentionally not embedded in
the document before the required exact-head CI and merge occur.

## Stable software contract

DKV-4 controls recurrent **compute**, not truth or answer authorization.

Its public decision vocabulary is:

- `Refine`;
- `StopCandidate`;
- `Abstain`.

There is deliberately no `Accept` action.

Provenance has precedence over numeric memory signals:

- `Missing` or `Conflict` -> abstain before a Delta-KV matrix read;
- `Unverified` -> never StopCandidate; bounded refinement ends in abstention;
- only independently `VerifiedPresent` evidence can reach StopCandidate, and
  StopCandidate still requires downstream decision/action gates where applicable.

Key dimension, projected key bytes/hash, gold answer and evaluator truth are not
controller policy inputs.

## DKV-4a result: fail-closed but over-conservative

The first controller used absolute Q12 correction magnitude as one independent
reason to keep refining.

On the frozen paired panel:

| metric | K=64 | K=256 |
|---|---:|---:|
| written correctness | 125/128 | 128/128 |
| StopCandidate | 11/128 | 10/128 |
| Abstain | 117/128 | 118/128 |
| recurrent cycles | 490 | 491 |
| read MACs | 62,720 | 251,392 |

It did not improve correctness relative to fixed-depth or SBG stability controls.
It consumed about 1.95x the recurrent cycles of stability-only. The absolute
correction-magnitude refinement rule is therefore rejected as the default DKV-4
compute policy.

DKV-4a remains retained as a negative/reference arm.

## DKV-4c result: remove analog residual as an unconditional refine signal

DKV-4c keeps correction magnitude as telemetry but lets refinement be driven by
observable semantic impact and MOR control evidence:

- ambiguity in memory outputs;
- actual V/M/A/R state change;
- MOR precision escalation;
- MOR structural disagreement above the frozen threshold;
- provenance precedence and hard depth bound.

Measured result:

| metric | K=64 | K=256 |
|---|---:|---:|
| written correctness DKV-4c | 125/128 | 128/128 |
| StopCandidate | 127/128 | 128/128 |
| Abstain | 1/128 | 0/128 |
| recurrent cycles | 251 | 250 |
| read MACs | 32,128 | 128,000 |
| stability-only cycles | 251 | 250 |

Against DKV-4a, DKV-4c removes 48.78% of recurrent cycles at K=64 and
49.08% at K=256 while preserving every retained written-query correctness
outcome. No retained written row is deeper under DKV-4c.

However, DKV-4c exactly matches the simpler SBG stability-only control in
aggregate written correctness and cycle count on this fixed one-page,
zero-disagreement MOR panel. It therefore does **not** establish an adaptive
compute advantage over the simpler control.

DKV-4c is retained as the cleaner development reference because it keeps
provenance and future MOR escalation explicit without the measured DKV-4a
over-refinement. That is a software/control-plane preference, not a scientific
architecture promotion.

## Unknown-state result

Production-style unknown handling comes from provenance, not from the Delta-KV
matrix.

For all 896 Missing-provenance queries per K in the dimension panel:

- matrix reads: 0;
- StopCandidate: 0.

The separate DKV-4b Unverified latent diagnostic showed at least one resolved
output coordinate on 896/896 unknown queries at both K. Complete latent
resolution was 871/896 at K=64 and 875/896 at K=256.

Thus the current Delta-KV magnitude/ambiguity path is not an unknown-state
detector on this panel. Numeric resolution must not be relabeled as presence,
confidence or verified provenance.

## K=64 versus K=256

On the frozen synthetic written fixtures K=256 produced 128/128 correct final
two-bit outputs versus 125/128 at K=64. That three-query difference is not an
optimal-width result.

With value width fixed at two, K=256 uses four times as many key-coordinate MAC
terms per read/update as K=64. Per seed the frozen ingest required:

| metric | K=64 | K=256 |
|---|---:|---:|
| prediction MACs | 2,048 | 8,192 |
| weight updates | 2,048 | 8,192 |
| logical matrix + metadata bits | 2,240 | 8,384 |

No wall-clock, RSS or energy conclusion follows from these logical counters.

## Evidence identities

DKV-4b dimension evidence:

- protocol: `71fc497affd9ae54d11a43cc4c07844078bc6ead`;
- executed code: `1cfb395485765fdb30a8e447554aab71213ac9a1`;
- retained under `research/dkv-4b/`;
- 2,048 rows, two byte-identical runs.

DKV-4c semantic-impact evidence:

- protocol: `286b4b9d847cc08bc1a445a0dcb7b3e623437fce`;
- executed code: `0efd2287965c6a8039b07dfe2b8603f5e5c3d879`;
- retained evidence head: `6bdcbdce81fd3af1fdb1e31d8dbad6627baaffa9`;
- RemoteOps run: `37198054863`, host `debian`, x86_64, Rust 1.89.0;
- 2,048 rows, two byte-identical runs;
- output SHA-256:
  `a57076feb598c6923981d3e79c871278093ea5ce4eea4ba506224e476e27cf5f`;
- retained under `research/dkv-4c/`.

## Development decision

The DKV-4 **engineering/development milestone is complete**:

- bounded compute controller exists;
- provenance precedence is fail-closed;
- actual DKV-2b/SBG/MOR evidence is wired;
- fixed-depth and simpler stability controls are retained;
- K64/K256 costs and behavior are measured;
- DKV-4a's over-refinement is preserved as negative evidence;
- DKV-4c removes that defect without rewriting prior results.

The following remain open before any stronger claim:

1. a separately preregistered panel with non-trivial MOR
   disagreement/escalation where DKV-4c can be distinguished from stability-only;
2. representative learned SBG states rather than random synthetic states;
3. physical wall-clock/RSS/traffic/energy measurements;
4. end-to-end context integration only after the controlling CONTEXT/DKV
   provenance gates are satisfied;
5. independent reruns before any model-level promotion claim.

No automatic propagation to other Memorithm repositories follows from this
development result.

**Scientific/model promotion remains BLOCKED.**
