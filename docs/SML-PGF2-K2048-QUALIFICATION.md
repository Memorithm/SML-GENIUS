# PGF2 K=2048 qualification

Status: implementation/correctness qualification under the PGF2 research track
merged in PR #193. This document does not promote PGF2 into the SML model path.

## Scope

Here `K` denotes the logical PGF2 Boolean state width. It is not a Delta-KV
key-width claim and it does not modify any frozen DKV protocol.

The width ladder is:

`K = {64, 128, 256, 512, 1024, 2048}`.

The explicit K=2048 panel uses 32 packed `u64` words and 16 deterministic
fixtures.

## Frozen correctness contract

For every tested width:

1. the packed in-place transform must exactly match the direct Boolean oracle
   `y[n] = XOR{x[k] | k & n == k}`;
2. applying the packed transform twice must reproduce the original packed state
   bit-for-bit;
3. no floating-point operation is part of the PGF2 transform;
4. the reference implementation performs no algorithmic scratch-word
   allocation beyond the caller-provided packed state.

For K=2048 specifically:

- packed words: 32;
- state bytes: 256;
- butterfly stages: 11;
- logical destination-bit XOR updates per transform:
  `2048 / 2 * log2(2048) = 11,264`;
- packed destination-word updates in the current reference:
  `32 * 6 + 16 * 5 = 272`;
- K=2048 deterministic fixtures: 16.

## Execution

CI runs the release example twice and requires byte-identical stdout:

`cargo run --locked --release -p sml-core --example pgf2_k2048`

The output reports the complete width ladder and a dedicated K=2048 aggregate
line.

## Non-claims

This qualification does not measure wall-clock latency, does not test semantic
quality, does not compare PGF2 with the matched reversible controls required by
PGF2-3, and does not authorize MOR, Delta-KV, context or model promotion.

A later semantic ablation must remain separately preregistered and must retain
negative/null results.
