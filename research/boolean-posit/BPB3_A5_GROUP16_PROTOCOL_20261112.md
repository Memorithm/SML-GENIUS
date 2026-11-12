# BPB-3-A5: preregister mixed precision at 16-coefficient granularity

Frozen 2026-11-12 before extracting unseen A5 TEST source weights.

## Hypothesis, NOT a verified performance result

BPB-3-A4 local16 scale-only has verified Boolean bitplane arithmetic but
**lost** versus A1 and Q3_K at a matched 108B/256 in the first heldout
SmolLM2 layers6/7. Additional exponent metadata reduced available
precision; see [A4 negative results](BPB3_A4_LOCAL16_RESULTS_20261112.md).

A5 proposes local16 **magnitude-plane precision** on top of the four
local16 exponents. It is an expressiveness superset of A4: every A4
uniform-mode block is representable by A5 for **exactly the same number
of bytes**. Therefore the exact global source-SSE optimizer may select
A4 configurations or better, and its optimized SSE cannot exceed A4
SSE on the same source and budget (modulo explicitly bounded rounding).
This inclusion is mathematical, **not** empirical superiority over A1
or stock Q3_K.

## Frozen on-wire specification and full byte accounting

One A5 block is **64 coefficients**, four contiguous groups of 16.
Every group independently chooses 1, 2, or 3 unsigned magnitude planes
and a dyadic signed exponent k in [-24,27]. Signed coefficient is
sign × magnitude, zero cannot have negative sign.

- 1 byte containing 4 × 2-bit group modes in ascending group order:
  `00` = one magnitude plane, `01` = two, `10` = three,
  `11` is RESERVED and must be rejected;
- 4 signed i8 local exponent bytes, one per 16-coefficient subgroup;
- 8 bytes little-endian shared 64-lane sign plane;
- for each subgroup g in increasing order, exactly mode[g] × 2 bytes
  of little-endian, 16-lane magnitude planes, low plane first.

Block size = **13 + 2 × sum_g mode[g] bytes**, from 21 B (4×one plane)
to 37 B (4×three planes), without hidden headers, lookups, or alignment.
When all four modes equal, the size equals A4 21/29/37 B. For 256
weights (4 blocks), fully charged bound is **108 B**, identical to the
prior A1/A4 experiments, and strictly below standard Q3_K's 110B.
For 24×256-weight rows, 96 A5 blocks = 384 groups, mandatory byte
cost 96×13 + 384×2 = 2,016 B. At a total budget of 2,592 B
(24×108), **exactly 288 optional two-byte plane additions can fit**.
Do not treat subgroup modes, local exponents, padding or correction
positions as free.

## Frozen source quantizer and Boolean dot rule

For each subgroup and each possible magnitude precision 1..3,
choose scale 2^k (k in [-24,27], exhaustive 52 candidates) minimizing
FP64 reconstruction SSE against **original widened FP32 source weights**.
Tie picks earlier exponent. Independent local mode candidates are
composed by exact global multiple-choice DP under the 2,592 B budget.
The A4 group candidates must agree in weights/scales when forced to a
uniform mode. Verify on independent small brute-force cases and test
non-increasing A5 source SSE versus A4 at identical actual bytes.

Bitwise POPCOUNT signed bipolar dot returns four subgroup integer
accumulators and four exponents. Compare each against a separate dense
signed oracle. Checked i128 dyadic accumulation at 2^-24 is exact for
admitted source/code ranges. Reconstructed f64 dots are not model
perplexity or GPU kernel time.

## Fresh held-out source and controls

New SmolLM2-135M pretrained **layers10/11** (different from A3 2/3
and A4 6/7) in the same pinned, Apache-2.0 checkpoint, revision
`93efa2f097d58c2a74874c7e644dbc9b0cee75a2`, safetensors SHA256
`80521b40281d6ce74e35c9282c22539e75aa0ac8578892b2a59955ef78d55da1`.

All four tensor names frozen:

- `model.layers.10.self_attn.o_proj.weight`;
- `model.layers.10.self_attn.q_proj.weight`;
- `model.layers.11.mlp.up_proj.weight`;
- `model.layers.11.mlp.down_proj.weight`.

Each tensor has 24 disjoint DEV and 24 disjoint TEST contiguous,
256-element original-weight blocks, seeded with
`bpb3-a5-group16-mixed-independent-layers10-11-v1` plus tensor name.
Fail closed on SHA, license, dtype, shape, missing tensor, nonfinite
values or overlapping indices. This is **a new within-model holdout**,
not a new independent model architecture.

Same source, same 16 predetermined bipolar input masks per source row,
same FP32 widening and same source quantizers:
A5 mixed local16, A4 local16, A1 adaptive (each 108 B/256),
tagged fixed A1 108 B/256, official stock GGML Q3_K (110B/256)
and Q4_K (144B/256), pinned ggml SHA
`f7591a6a01f9273de03331732bdc0b39721168be`.
Report per-tensor and pooled sqrt(weighted mean square) errors for
TEST and DEV, including any regressions, zero benefit or native K
comparisons. Never tune based on exposed TEST values.

Only GitHub-hosted Rust 1.89 CPU execution. No Dell/Thor/Mac,
NVIDIA SDK, production activation, automatic release, model-quality,
token/s, latency, DRAM, RSS or standard Posit claims.
