# BPB-3-A8 — amortized 256-weight common dyadic base + 16 packed 2-bit local offsets

Prospective protocol frozen on **2026-11-12**, before reading any A8 TEST values. A7's negative A6-delta2 comparison on unexposed SmolLM2 layers24/25 is now known and **must not be used for parameter tuning**.

## Hypothesis and fully costed superblock

A6-delta2 encodes four 64-coefficient blocks per 256 weights; each 64 block has its own one-byte dyadic base plus a one-byte four-group 2-bit delta mask. Thus scale metadata is **8 B/256 weights**. A8 instead shares one base among all 16 contiguous local 16-weight groups in the **entire 256-weight superblock**, and stores 16 2-bit offsets in 4B. Total scale metadata becomes **1 + 4 = 5B/256**. Three metadata bytes are saved, *but every local scale is now constrained by the same common base*. It is not guaranteed to improve source SSE; compare strictly at identical total source weights and wire budget.

Canonical A8 256-weight wire:
- Byte 0: shared dyadic base+24, accepted only 0..51; all values 52..255 reserved.
- Bytes 1..4: LE 32-bit integer containing 16 × two-bit **mode_minus_one** fields for consecutive 16-weight groups. Value0=one, value1=two, value2=three unsigned magnitude planes; value3 is forbidden.
- Bytes 5..8: LE 32-bit integer containing 16 × two-bit local delta symbols, each 0/1/2/3 representing signed dyadic exponent delta −1/0/+1/+2. Each actual exponent must remain in [-24,27] or abort.
- Bytes 9..40: 32 bytes containing 256 negative sign bits in four consecutive LE u64 words, one per source weight. Reject negative zero in each local magnitude position.
- Following: for group0..15 in order, exactly mode[g] independent u16 LE magnitude bitplanes, low magnitude bitplane first. No extra padding, no per64 block header and no implicit alignment.

**Size = 41 + 2 × sum(mode[g]) bytes per 256 weights**, ranging from 73 to 137 bytes. Since one 256-weight source row is now *one independently decodable superblock*, individual superblocks may cost more than 108B if other superblocks cost less; globally enforce **24×108 = 2,592 bytes** across each 24-row source split. Byte accounting must use real packed lengths; A6-delta2 remains 108B/256 on that split. Minimum total 24×73=1,752B, leaving 840B for a maximum 420 optional two-byte subgroup magnitude planes. No free base/index/offset/sign fields. Superblock random access is at 256-weight boundaries, and may cost more to decode than A6.

## Frozen exhaustive quantizer and two-stage exact optimization

For each local original source group of sixteen FP32-widened trained weights and each magnitude mode m∈1,2,3 and each candidate exponent k∈[-24,27], compute SSE of clamped signed integer coefficients `round(weight/2^k)`, magnitude ≤ 2^m−1 and dyadic reconstructed values. Nonfinite original values or SSE must fail closed; exact ties choose earlier exponent encountered.

For each possible **single shared base k**∈[-24,27], each 16-weight group × mode selects the best of the four valid local scales k−1,k,k+1,k+2; no post-TEST tuning. An **exact inner knapsack** distributes 0..32 optional two-byte planes over the 16 groups for each base; retain the minimum SSE candidate at each superblock byte cost (73..137B), including the chosen global base.

A second **exact global multiple-choice DP** across the 24 independent 256-weight rows selects one packed superblock per row respecting the 2,592B total. Prove both DPs against bounded brute-force independent oracles. Serialize, parse, reject invalid mode/base/local exponent, and verify signed Boolean 16-bit POPCOUNT dot equals an independent scalar signed integer oracle for each group and the full 256-weight row. Checked i128 dyadic sum aligns 2^-24. IEEE f64 reconstructed dot and source RMSE on original weights are **not full-model quality**.

## Previously unused TEST weights and comparators

SmolLM2-135M pretrained Apache-2.0, immutable HF commit `93efa2f097d58c2a74874c7e644dbc9b0cee75a2` and exact original `model.safetensors` SHA256 `80521b40281d6ce74e35c9282c22539e75aa0ac8578892b2a59955ef78d55da1`. Preselect the **last unseen layers28/29** (SmolLM2-135M has 30 transformer blocks):
- `model.layers.28.self_attn.o_proj.weight`
- `model.layers.28.self_attn.q_proj.weight`
- `model.layers.29.mlp.up_proj.weight`
- `model.layers.29.mlp.down_proj.weight`

For each original matrix, take 24 DEV and 24 independent TEST 256-weight source rows from SHA256-shuffled nonoverlapping source blocks using seed `bpb3-a8-one-base-256-layers28-29-v1` + exact tensor name. Never reuse A3 layers2/3, A4 6/7, A5 10/11, A6 18/19, A7 24/25 TEST. Check exact pinned original trained safetensors SHA, shape, finiteness and widening; no original source weight bytes in CI artifacts. This is a **new within-model tensor holdout**, not an independent architecture.

Every codec sees the exact same FP32-widened original TEST source and the same 16 seeded bipolar masks per original 256-weight row, with A3 seed `0xa3b0114d5c152026` and TEST xor `0x9e3779b97f4a7c15`. Compare fully accounted A8 and A6-delta2/mixed, A1 tagged fixed/adaptive at exactly 108B/256, and stock unmodified GGML Q3_K 110B/256, Q4_K 144B/256 from upstream commit `f7591a6a01f9273de03331732bdc0b39721168be`. Report every matrix and properly pooled sqrt(SSE/count) source coefficient RMSE and paired bipolar-dot RMSE (not mean RMSE). Keep adverse results and DEV statistics. Do not modify the candidate or selection after TEST.

## Qualification and next decision

Rust 1.89 strictly formatted and warning-free host tests: full canonical wire, independent bitwise/dense Boolean dot oracle, exact checked i128, budget and brute-force DP oracles. Hosted GitHub Actions CPUs only, no physical Dell/Thor/Mac workloads, no NVIDIA SDK or production inference. Research PR merges require full exact-head CI green.

A8 may fail because a single global base is too restrictive. Even if it improves source RMSE over A6-delta2, no claim of model-level perplexity, generated token quality, speed/DRAM traffic, energy or standard Posit compliance without separate direct evidence. Honest negative results remain part of the scientific record.
