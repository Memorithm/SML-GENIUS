# BPB-3-A4: preregister local16 dyadic scales at a fully costed storage budget

Frozen on 2026-11-12, before any held-out A4 data are inspected. Research-only.
A1 and A3 both showed improved reconstruction relative to tagged fixed BPB-3,
but worse reconstruction and bipolar-dot error than unmodified Q3_K. This A4
mechanism is a new independent **hypothesis**, not a demonstrated improvement.

## Wire format, no hidden bits

One block reconstructs 64 original FP32 coefficients. A4 has **one common
magnitude precision for the 64 lanes**, and four **independent** power-of-two
scales for the four contiguous 16-lane subgroups. Serialization:

- 1 byte mode, accepted values 1, 2, 3 for 1/2/3 unsigned magnitude planes;
- 4 signed i8 subgroup exponents (in original source order), -24..27,
  reconstruction scale = 2^exponent;
- 8 bytes little-endian negative sign bitmask;
- 8 bytes little-endian per magnitude plane, up to 3 planes, in order.
- Reject all other mode/exponent combinations, lengths and negative zeros.

Total = 13 + 8*mode bytes = **21 / 29 / 37 bytes per 64 coefficients**,
or 2 extra bytes per block versus tagged A1 19 / 27 / 35.
All scale bytes and mode bytes count. One 256-weight row uses four blocks.
For A4 **budget is 108 bytes/256 coefficients**, exactly identical to
A1 tagged fixed 3-bit and A1 adaptive 108 B, smaller than stock Q3_K
110 B and stock Q4_K 144 B. A4's minimum is 84 B/256, so **only three
8-byte precision increments** can fit into a 256-weight row; no silent
fourth increment. DP allocation is allowed across each complete split's
96 64-weight blocks with total budget 24*108 = **2,592 bytes**. No nested
tensor metadata or uncounted corrections.

## Frozen algorithm and expected controls

For each 64-block/mode, quantize **each** of four 16-value groups
independently by exhaustive shared exponent k in [-24,27], source-value
rounding/clamping to signed magnitude representable by mode, and SSE
minimization. Deterministic lower-exponent tie order. The combined block
SSE is the sum of the four SSEs. Globally optimize the 96-block precision
choice using an exact bounded multiple-choice knapsack in the same
8-byte increments as A1. Compare with independent brute-force allocations
on short fixtures.

Validate serialize/deserialize, fixed format, invalid wire rejection,
no heap allocation in signed Boolean dot calculation, per-subgroup POPCOUNT
with independent dense oracle, and exact checked dyadic i128 inter-block
accumulation. SIMD and whole-model inference are *not* claims in this PR.

A4 is **not guaranteed to improve source SSE** over A1 because its
local-scale metadata costs 2B per 64 and reduces available magnitude
planes; record negative results if any.

## Held-out trained source, no reused A3 TEST

For a frozen-source evaluation, use the independently pinned public
HuggingFaceTB/SmolLM2-135M pretrained safetensors model (Apache-2.0)
revision `93efa2f097d58c2a74874c7e644dbc9b0cee75a2`,
file SHA256 `80521b40281d6ce74e35c9282c22539e75aa0ac8578892b2a59955ef78d55da1`.
Do not reuse the exposed A3 layer2/3 TEST. Instead preselect four fresh
rank-two matrices: layers **6 and 7** attention output and query projection,
MLP up and down projection, specifically:

- `model.layers.6.self_attn.o_proj.weight`
- `model.layers.6.self_attn.q_proj.weight`
- `model.layers.7.mlp.up_proj.weight`
- `model.layers.7.mlp.down_proj.weight`

Same deterministic selection rule as A3, but new SHA-seeded shuffled
split label `bpb3-a4-local16-independent-layers6-7-v1`. For each matrix:
24 DEV and 24 disjoint TEST original contiguous 256-value rows. Original
BF16/F16/F32 values widen exactly to FP32; hash every selected source
file. Stop on nonfinite weights, missing tensor, wrong SHA, bad dtype or
shape. These are new **within-model** strata, NOT another independent
architecture and NOT a language quality/perplexity test.

Every codec operates on identical original values:
A1 adaptive 108 B/256, A1 tagged-fixed 108 B/256, A4 local16
108 B/256, official unmodified ggml Q3_K 110 B/256 and Q4_K
144 B/256 at pinned upstream `f7591a6a01f9273de03331732bdc0b39721168be`.
Compare per-coefficient reconstruction RMSE and matched 256-weight
bipolar-dot RMSE using **identical** 16 masks per row generated from
A3 `0xa3b0_114d_5c15_2026` with its xorshift64* stream and split
seed variation, not different masks per codec. For pooled metrics use
sqrt(sum(squared_error)/count), not arithmetic averages of RMSE.

Report all TEST matrices without selection. No post-TEST retuning,
regardless of positive or negative results. Publication gate: functional
CI, trained-source provenance, and full local16 vs A1/Q3/Q4 metrics;
no superiority claim absent matched evidence. Model-integrated
perplexity, token accuracy, physical memory traffic and native CPU
speed require subsequent work.

No Dell/Thor/Mac workloads for these tests; use GitHub-hosted CPUs,
Rust 1.89, no NVIDIA tools.
