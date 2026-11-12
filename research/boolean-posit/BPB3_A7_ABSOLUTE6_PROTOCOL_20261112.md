# BPB-3-A7 — four absolute 6-bit local exponent codes

Prospective preregistration on **2026-11-12**, before reading any A7 held-out weights. This document is committed before implementing or evaluating the A7 candidate.

## Motivation and evidence limits

A6 signed two-bit base-relative exponent differences improved held-out reconstruction on SmolLM2-135M layer18/19 versus the original BPB-3-A1 at **108 B/256 original weights**. The independently verified native Q3_K remained more accurate at **110 B/256**. A6 four-bit differences were less effective than its two-bit form. Do not use exposed A3 layer2/3, A4 layer6/7, A5 layer10/11, or A6 layer18/19 TEST samples to tune A7.

This candidate tests one alternative compression mechanism: **four independent biased 6-bit exponent symbols** with a packed fixed-size 24-bit field per 64 weights. Four independent exponents lie within [-24,27] (52 valid symbols) and therefore fit in 24 bits rather than the 4 bytes used in A5. A7 costs the same as A6's four-bit signed-delta mode, but allows every independent combination of the 52 exponents (a mathematical superset of A6-delta4). Unlike A6-delta2 it consumes **one additional byte per 64 weights**.

## Exact wire specification

The block encodes 64 successive original coefficients in 4 consecutive 16-coefficient groups.

- Byte 0: four two-bit fields in order, `mode_minus_one` for each group; 0/1/2 mean **one/two/three** unsigned magnitude bitplanes and value 3 is forbidden.
- Bytes 1..3: **24-bit little-endian bit field**, 4 × 6-bit `exponent+24` codes; group 0 occupies bits0..5, group 1 bits6..11, group 2 bits12..17, group 3 bits18..23. Values 0..51 encode exponents -24..27. Values52..63 are invalid and must be rejected for every group.
- Bytes 4..11: 8-byte LE sign mask, one bit per coefficient, with a negative-zero encoding forbidden.
- Remaining: for group 0..3, exactly `mode[group]` successive 2-byte LE magnitude planes in ascending significance, each plane containing 16 per-weight bit positions. No padding or external codebook.

**Actual storage = 12 + 2×Σmode[group] B per 64 coefficients**, min 20B and max 36B. For 256 weights, 4 blocks; per-24-row TEST budget **24 × 108 = 2,592B**, all metadata included. 96-block base with one plane per subgroup is 1,920B. The 672B remaining may fund at most 336 optional two-byte plane increments. For source comparison use A7 108 B/256, A6-delta2/mixed/A1/A5 each 108 B/256, official native Q3_K 110 B/256, Q4_K 144 B/256. Never compare a 108B candidate to a 104B untagged A1 as if equal-budget.

## Frozen quantization and exact budget optimization

Per original 16-value group and each 1/2/3 magnitude mode, exhaustively evaluate all **52 dyadic scales** 2^k (k=-24..27). Each weight quantizes to `round(w/2^k)` clamped to signed magnitude representable by mode. Choose group SSE minimum; on exact ties choose smallest exponent encountered, deterministically. Refuse any nonfinite original source or nonfinite sum of squares.

The four independent group choices form a block. Allocate precision using an **exact multiple-choice DP** across all 16-value groups of the split, with two-byte increments and the exact block base cost. No headers or scale symbols outside the accounted wire. Prove on synthetic brute-force fixtures and against independently decoded source SSE.

The format contains all A6-delta4 configurations at equal byte cost: for identical local precision modes and exponents, its packed codes and signed source values are identical. Thus, an exact A7 minimum-SSE optimizer must not exceed the A6-delta4 optimum on **the same original source and total budget**. **No analogous guarantee versus A6-delta2**, whose metadata is one byte smaller per 64.

Each signed Boolean dot uses `POPCOUNT` on 16-bit planes per subgroup, with an independent dense integer oracle. Accumulate four group results in checked `i128` aligned to the smallest scale 2^-24. Reject forbidden bitfields, invalid wire sizes, negative zero, and reserved symbols. No uncharged fields, source output or dynamic entropy decoder.

## Frozen fresh trained-weight test

Use the original public pretrained Apache-2.0 [HuggingFaceTB/SmolLM2-135M](https://huggingface.co/HuggingFaceTB/SmolLM2-135M), HF revision `93efa2f097d58c2a74874c7e644dbc9b0cee75a2`, safetensors exact SHA256 `80521b40281d6ce74e35c9282c22539e75aa0ac8578892b2a59955ef78d55da1`. The model has 30 layers. Freeze four **previously untested** original parameter matrices:

- `model.layers.24.self_attn.o_proj.weight`
- `model.layers.24.self_attn.q_proj.weight`
- `model.layers.25.mlp.up_proj.weight`
- `model.layers.25.mlp.down_proj.weight`.

Take exactly 24 DEV plus 24 disjoint TEST contiguous 256-weight blocks per matrix, from a SHA256-seeded shuffle using `bpb3-a7-absolute6-unseen-layers24-25-v1` plus the full tensor name. Exact widening F16/BF16/F32 to FP32; hard-stop on SHA mismatch, shape, nonfinite data or overlapping original source rows. Preserve selected source indexes and hash manifest only, never distribute trained weights. This is a **new tensor holdout within the same model**, not an independent model.

Compare on identical original source, and the same 16 predetermined signed-bipolar input masks per 256-weight row from the A3 xorshift64* generator `0xa3b0114d5c152026`, with TEST split xor `0x9e3779b97f4a7c15`. Compute source-coefficient RMSE and paired bipolar dot RMSE for all four matrices and correctly pooled sqrt(sum SSE / total count). Include A7, A6-delta2, A6-delta4, A6-mixed, A5, A4, A1 fixed/adaptive, and pinned original GGML Q3_K/Q4_K (ggml SHA `f7591a6a01f9273de03331732bdc0b39721168be`). Retain all outcomes including no gains and regressions. No post-holdout tuning.

## Exit gates

GitHub-hosted Rust 1.89 CPU-only tests on exact PR head: format, canonical serialization, exhaustive DP, signed Boolean dot oracle, cross-block dyadic accumulation, and subset relation to A6-delta4. Reproducible source weights and comparative CSV. Full repository CI green before merge. Do not use Dell, Thor, Mac, GPU SDKs or productive services, or claim full LLM perplexity, token accuracy, end-to-end throughput, energy or standard Posit compliance from component-level source RMSE.
