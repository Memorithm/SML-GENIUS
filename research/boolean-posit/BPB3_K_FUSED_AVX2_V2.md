# BPB3-K fused AVX2 v2 — prospectively frozen qualification

Status: experimental candidate; E (incomplete qualification) until the exact
merged source has passed fidelity and paired Dell measurements. Targets are
not results. No format, model weights, quantization, or default mode change.

## Audit and hypotheses

Audited SML main `a45459259e6165a50a35d416fa7ea149fd5f4eac`, ancestor
`e4652003701ef2ca604e60259340cb0e723fb448` (#887). RemoteOps main at initial
inspection: `839ec1397c2bafed0b480884cccce702d0de2f67`.
Roadmaps read from off-main `agent/ecosystem-roadmap`: SML
`d54b7722f82015f374e61922dc8211ff688c4abb`, RemoteOps
`34f13675c34fcc559c86c0d947cb42f317a4590d`. No roadmap copied into main.

V1: direct 110-byte weights + 292-byte Q8_K activations; 16 subgroups each
decode two 8-element halves into i32, multiply coefficient × activation ×
subscale, keep eight i32 accumulators, spill per superblock, widen and scale
to f32, accumulate blocks in order, reduce eight lanes in order. No dense
weight materialization. FFI allocates row/result Vecs on each call. The ggml
adapter uses a safe 2×2 tile (nrc=2), not four rows on tmp[32].

Measurable hypotheses: i32 decoding/multiplication instruction pressure,
per-superblock boundaries, per-call allocator overhead, dependency chains.
Do not infer memory-bandwidth limitation from thread scaling alone.

V2: share three bitplane broadcasts over each 64-value bank; decode signed
bytes in 32-value groups, interleave in (0,8,1,9,...,7,15) order, widen to i16, reconstruct
negative values with XOR -1, multiply by subscale in i16, pair signed Q8_K
activations with VPMADDWD. Two integer accumulators break dependencies. Each
scaled coefficient fits [-128,128], each lane fits [-524288,524288]. Keep the
eight f32 lanes across blocks and store only at row end. No FMA, reassociation,
temporary dense weights, or per-call Vec. AVX2 runtime guard; scalar fallback.

The intermediate `fused_v2_i16` variant broadcasts bitfields per 16 values;
it shares the v2 allocation and accumulation path, isolating the 32-value
byte-decoding change. It is retained as a diagnostic ablation, not an eighth
primary full-model benchmark arm. Kernel diagnostics at 3/9/36 superblocks are
reproducible via bpb3k_kernel_bench.rs; they never establish model speed.
Further unrolling, FMA, persistent relayout and NUMA policies remain separate
experiments; no presumed winner.

## Frozen fidelity criteria (before timing)

- Every FP16 encoding: 61440 normal, 2 signed zeros, 2046 subnormals,
  2 infinities, 2046 NaNs. Exact widening including NaN sign, signaling/quiet
  bit and payload; arithmetic NaN payload propagation is not promised.
- Exact eight integer lanes against independent original-Q3_K byte indexing,
  all 64 subscale codes in all positions, 4096 deterministic cases, signed
  activation extremes. Exact ordered f32 bits across block counts
  0/1/2/3/7/31/257, unaligned buffers, extreme finite scales. Partial buffers
  rejected before SIMD access. ABI nrc=1/2/4 with output sentinels.
- Full trained SHA-verified SmolLM2-135M original Q3_K and lossless relayout:
  three prompts, context limits 128/512 (including genuinely longer input),
  2/8 threads, two fresh processes per cell, eight deterministic greedy tokens.
  Observe every vocabulary logit at every generated step, preserve raw dumps.
- V1/v2: exact tokens and every f32 bit. Repeat processes: exact tokens/logits.
  Stock/v2: exact tokens; every finite logit must satisfy
  `abs(candidate-reference) <= 2e-4 + 2e-5*abs(reference)`. Nonfinite values,
  malformed/truncated dumps, wrong argmax or shapes reject qualification.
  No threshold tuning after observation. The observer changes only upstream's
  debug example, outside timed inference; kernel and model code stay unchanged.

This bounded corpus does not establish universal model equivalence.

## Historical evidence re-audit

RemoteOps run 42510895356 completed successfully under CHECKUPAUTO; controller
`0e697ba4438c6dcb242f09dc0c41031a40c8714e`, SML source e4652003, pinned
llama.cpp f7591a6. Artifact 13629688328, ZIP SHA256
`ac41818d55c9b6724788e46b7ec0636281d99d3fade89373f51e45ec64a82b3c`.
Raw log confirms the historical tg32 means:

|mode|2 threads tok/s|8 threads tok/s|
|---|---:|---:|
|stock|102.32 ± 0.19|140.33 ± 6.90|
|scalar|23.00 ± 0.20|83.28 ± 2.53|
|simd|17.33 ± 0.14|66.07 ± 0.48|
|tiled|11.79 ± 0.06|46.49 ± 1.36|
|hybrid|17.42 ± 0.15|61.62 ± 0.27|
|fused|48.30 ± 0.66|120.65 ± 0.92|

Artifact contains only aggregate output: no per-repeat samples, affinity,
NUMA placement, binary hash or peak RSS. Sequential fixed mode order. These
are descriptive historical observations, not a paired confirmatory study.

## Dell campaign contract

RemoteOps alone; trusted merged SML SHA and RemoteOps main; CHECKUPAUTO;
runner online/available, cooperative lock, explicit RAM/disk/CPU admission,
no service changes, no GPU SDK. Record lscpu flags/topology, effective CPU set,
NUMA policy, compiler, model/source/binary hashes and peak RSS. Check real
AVX2 capability at execution, never assume a CPU brand guarantees a flag.

Historical qualification `-t2 -p64 -n32 -r3`; main campaign 2/8 threads,
`-p64 -n32 -r5`. Seven modes. Seven forward and seven reverse rotations yield
14 balanced paired rounds; each mode occupies each ordinal twice. Retain
llama-bench JSON containing all five per-process sample durations and all
failed attempts. Treat round as statistical unit, not 70 independent hosts.
Compute paired tg32 ratios v2/v1 and v2/stock, log-ratio mean and 95% Student-t
interval with df=13; report prompt throughput, ms/token, RSS, raw sample CV
and all regressions separately. Superiority requires fidelity and lower ratio
bound >1 at both thread counts; ambitious +10% requires lower bound >1.10.
No-regression at eight threads requires lower v2/v1 ratio bound >=1.00.
If evidence is missing or contaminated, E; if inconclusive, C; material
fidelity/resource regression, D. B only with confirmed v1 improvement without
stock superiority. No pooling of machines or retrospective sample exclusions.

Reusable lessons for SciRust/FLAT: preserve integer lane ownership, exploit
range-proven packed products, guard CPU capabilities, retain oracle/reduction
order and test ABI sentinels. Transfer requires destination qualification;
no automatic downstream speed or model-quality claim.
