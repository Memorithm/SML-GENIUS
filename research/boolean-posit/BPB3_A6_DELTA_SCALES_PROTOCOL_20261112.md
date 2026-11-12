# BPB-3-A6 — frozen compact local exponent coding experiment

Date of preregistration: 2026-11-12, before any unseen A6 holdout source is read.
A1 at 108 B / 256 is better than A4 and A5 at the same budget; official
Q3_K at 110 B / 256 is better than all three in source and paired-dot RMSE.
Do **not** infer improvements from the representational degrees of freedom.

## Research motivation, baselines and compression algorithm triage

Official GGML Q3_K encodes 256 weights, 16 local 16-weight group scales
quantized in 6-bit fields plus an amortized 16-bit shared scale; see
https://github.com/ggml-org/llama.cpp/wiki/Tensor-Encoding-Schemes and
https://github.com/ggml-org/llama.cpp/blob/master/ggml/src/ggml-common.h.
This does NOT establish that our dyadic delta format is competitive.

Explore two **fixed-width, direct-decode** exponent delta schemes, and a
mixed per-block selector. Huffman, Golomb-Rice, RLE and ANS are not embedded
in this trial: a 4-exponent block is too small to assume entropy gains
after the mandatory headers; no entropy or latency benefit is presumed.
Subsequent entropy coding needs an actual histogram and amortization
analysis plus block-level random access constraints.

## Exact on-wire format

One 64-coefficient block is 4 consecutive groups of 16. Mode and sign
semantics equal BPB-3-A5, but replace the FOUR exponent i8 bytes with
one common base exponent and fixed-width deltas.

- Byte 0: four 2-bit `mode_minus_one` fields, modes 1/2/3 permitted;
  encoded field `11` invalid. Modes independently choose one, two
  or three 16-coefficient unsigned magnitude planes.
- Byte 1: **base/variant** canonical symbol:
  values 0..51 mean 2-bit delta mode with base exponent = symbol − 24;
  values 52..103 mean 4-bit delta mode with base = symbol − 52 − 24;
  values >=104 forbidden. All actual exponents must fall in [-24,27].
- Two-bit offsets: ONE byte with four 2-bit codes. Code 0/1/2/3
  denotes delta -1/0/+1/+2. This is the only accepted 2-bit mapping.
- Four-bit offsets: TWO bytes with four signed 4-bit codes, low nibble
  then high nibble, group order 0,1,2,3. Code 0..15 denotes delta -8..+7.
- Next 8 bytes: shared 64-lane sign plane, little endian; negative zero
  must be rejected.
- Then for group 0..3, its 1..3 magnitude planes, each a u16 LE in
  ascending significance. No padding or extra lookup metadata.

Total cost **11 + 2×sum_group_modes** for 2-bit offsets (19..35B per
64 coefficients) and **12 + 2×sum_group_modes** for 4-bit offsets
(20..36B per 64). A5's 4 exponent bytes cost
**13 + 2×sum_group_modes** (21..37B per 64). The 2/4-bit variant flag
is included *within* the already counted base symbol: no hidden flag.

Three A6 arms must be measured separately at a FIXED total
**108 bytes per 256 weights** (3.375 bpw including scale/mode/sign
and bitplanes): `A6_delta2` only, `A6_delta4` only, `A6_mixed`
(each block independently selects 2 or 4 bits). Four 64-value blocks
per 256 source values; across 24 256-value rows exactly 2,592 bytes
are available. Count every byte actually written; variable length
is carried by the block's canonical base/variant byte.

## Frozen optimization

For each original 16-value group × each magnitude plane count 1..3 ×
each exponent -24..27, calculate FP64 source SSE using exact widened
FP32 source and quantize = round(value / 2^k).clamp(±(2^mode−1)).
Under each common base k ∈ [-24,27], enumerate every valid local
delta in the declared fixed-width range, selecting the minimum
per-mode SSE and deterministic first-encountered ties. Build exact
per-block 4-group multiple-choice DP over 0..8 extra 2-byte planes,
holding base/variant fixed. Retain the best candidates by **actual
byte cost** (19..36 bytes), with stable deterministic ties.
Finally an exact global DP over the blocks of the source split chooses
a candidate for each 64-value block under the **2,592-byte ceiling**
and reconstructs its packed wire.

At each point enforce valid scale exponent, mode, byte accounting,
source finiteness and no negative zero. Validate with brute-force
oracles for small source and direct signed POPCOUNT group-dot against
a separate scalar signed integer oracle. Cross-block dot sum aligns
2^(-24) in checked signed i128. A6 may be less accurate than A5 if
restricted deltas make local optimal scales unreachable.

## NEW held-out source

Pinned pretrained Apache-2.0 checkpoint
HuggingFaceTB/SmolLM2-135M, revision
`93efa2f097d58c2a74874c7e644dbc9b0cee75a2` and exact
model.safetensors SHA256
`80521b40281d6ce74e35c9282c22539e75aa0ac8578892b2a59955ef78d55da1`.

Do NOT reuse already revealed A3 layers2/3, A4 6/7, A5 10/11.
Preselect four never tested matrices:
`model.layers.18.self_attn.o_proj.weight`,
`model.layers.18.self_attn.q_proj.weight`,
`model.layers.19.mlp.up_proj.weight`,
`model.layers.19.mlp.down_proj.weight`.
Model config specifies 30 hidden layers. Use deterministic SHA-seeded
shuffle of complete original nonoverlapping 256-weight blocks with seed
`bpb3-a6-delta-local16-layers18-19-v1` + tensor name.
Take 24 disjoint DEV and 24 TEST rows per tensor and preserve selected
indices and SHA of widened source bytes. Reject mismatch, overlaps,
nonfinite source or unsupported dtype. Same model, **not independent
out-of-architecture evidence**.

Compare 3 A6 arms to A5 mixed16 and A1 adaptive/fixed at 108B/256 and
official untouched GGML Q3_K 110B/256, Q4_K 144B/256, pinned
`f7591a6a01f9273de03331732bdc0b39721168be`.
Every arm must consume identical original FP32-widened source and
the **same** 16 deterministic signed bipolar masks per row (A3 mask
generator seed 0xa3b0114d5c152026, TEST variation unchanged).
Report every four TEST matrix, pooled sqrt(total squared error/count)
on weights and paired dots, code size, 2/4-bit usage counts and
negative outcomes. Do not adapt algorithm/code to revealed TEST.

GitHub-hosted CPU-only Rust 1.89. Do not run physical Dell/Thor/Mac,
start production inference, introduce NVIDIA SDKs, or claim real model
perplexity/token fidelity, throughput/energy, memory traffic, or
compliance with Posit Standard.
