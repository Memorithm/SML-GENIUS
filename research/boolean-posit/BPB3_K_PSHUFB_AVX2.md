# BPB3-K PSHUFB-AVX2 — Haswell/Broadwell kernel experiment

Status: **research-only and unqualified for model inference or physical speed**.
SML-GENIUS remains the model; RemoteOps is the only remote physical control plane.

## Motivation / external provenance

The original ggml Q3_K vs signed Q8_K x86 kernel is pinned to
[llama.cpp f7591a6](https://github.com/ggml-org/llama.cpp/blob/f7591a6a01f9273de03331732bdc0b39721168be/ggml/src/ggml-cpu/arch/x86/quants.c)
and uses packed integer AVX2 VPMADDUBSW and VPMADDWD with FP32 FMA.
The pinned upstream ggml_vec_dot_iq3_s_q8_K is a related but **different
quantization codebook**, using indexed iq3s_grid scalar lookups,
PSHUFB for sign expansion and integer MADD. IQ3_S's speed or model
quality is not comparable without a separate same-model/same-budget study.
No upstream source is copied into this experimental implementation.
Intel documentation places AVX-512 VPOPCNTDQ after Skylake-X,
on Ice Lake server, not on Broadwell or first-generation Xeon Scalable.

## Candidate and exact input domain

Research code: **bpb3k_pshufb_avx2.rs**; 110-byte BPB3-K weight block
(256 Q3_K-equivalent signed codes), 292-byte original signed Q8_K.
Both raw source scale bit patterns are returned without FP32 reduction.

AVX2 PSHUFB expands two 16-bit sign/low/high bitplane segments into one
32-lane vector of {0,1} integers. For sign=s and magnitude bitplanes
b0,b1, each Q3_K-equivalent integer is:

    q=(b0 XOR s)+2*(b1 XOR s)-4*s

This is in [-4,+3]. Because VPMADDUBSW requires unsigned weights,
the code uses unsigned_q=q+4 in [0,7], computes two adjacent
unsigned×signed products in SIMD, then subtracts the contribution
of four times the signed Q8 activation codes. No i16 saturating
limit is reached: 2*7*128=1792, less than 32768.
A subsequent AVX2 VPMADDWD applies the exact 16 signed Q3_K subgroup
scales; the integer result is bounded in magnitude by 4,194,304.

This **combines Boolean bitplane extraction with native integer SIMD
multiplication** instead of emulating vector popcount.
It has no dynamic bitplane transpose, no activation LUT, no allocation
per dot and does not trust Q8_K's cached bsums. A safe Rust entry point
checks exact lengths and runtime AVX2 support before the unsafe
core::arch::x86_64 implementation. Other platforms return a typed
unsupported-CPU error. No ggml runtime default is changed.

## Predeclared gates and benchmark

1. Independently read original, untranscoded Q3_K wire bytes in a scalar
   signed-integer oracle, compare against the packed BPB3-K SIMD result
   on 96 deterministic weight cases × 16 signed-Q8 patterns, extremes,
   nonfinite raw-scale metadata bit preservation, malformed lengths.
2. Extract the **unchanged original** ggml AVX2 Q3_K C function from
   pinned upstream via existing run_bpb3k_stock_oracle.sh. This
   also validates 896 original AVX2 and 112 original tiled comparisons.
3. For the 256 original signed Q8 codes, one of each, verify bit-exact
   single-block FP32 outputs for 1024 different weight rows, with both
   FP16/f32 block scales **frozen at exactly 1.0**. This unit-scale
   domain is not arbitrary-scale/multiblock FMA parity.
4. On the same hosted runner compare original Q3_K AVX2 C vs the new
   PSHUFB Rust kernel, 1/32/256/1024 rows, 3 warmups, 16 alternated
   AB/BA paired rounds and 4 repeats per timed invocation. Retain all
   64 paired raw observations and descriptive medians in ns/dot.
   Transcode Q3_K -> BPB3-K once **outside** timing as in previous
   single-block experiments. Full-model speed is not implied.
5. Compile optimized Rust for exact target-cpu=haswell and inspect
   generated assembler for VPSHUFB, VPMADDUBSW, VPMADDWD and no
   VPOPCNT. This does **not** establish absence of register spilling:
   function-scoped disassembly and native physical counters are needed.

No favorable speed threshold gates a green CI. The benchmark may
refute the speed hypothesis. Never promote this kernel solely on CI
or microbenchmarks. Any full-model phase must preserve stock
vector/tiled FP32 order, all retained logit/token fidelity constraints
and **RemoteOps** Dell admission with the existing frozen CPU
contamination policy. An AVX2 native baseline is required.

Potential library transfer to SciRust is conditional on validated
semantics, verified license/provenance and requalification in the
destination. No new GPU vendor dependencies are introduced.
