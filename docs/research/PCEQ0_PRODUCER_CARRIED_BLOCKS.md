# PCEQ-0: producer-carried block quantization (pre-registered research)

Status: implementation candidate and tests, **not** an SML runtime feature. Issue: #829.

## Hypothesis

Fuse blockwise scale-statistic collection with the numerical producer, retain only
a bounded local scratch block, and publish quantized i8 payload and a compact
scale/active sidecar. This can avoid materializing and rereading a complete
unquantized activation tensor. It does **not** eliminate the necessary initial
production of arbitrary data; it does not prove a reduction in physical DRAM
reads when the control is cache-resident.

PCEQ-0 first implements a **signed fixed-point i16 -> i8** control, with
power-of-two exponents, to make edge-case correctness testable without float
nonfinite/rounding ambiguity. It is not an FP32 model quantizer. Future FP32
variants require a separately reviewed contract handling NaN/Inf, subnormal,
rounding and clipping. **Never** treat this experimental lossy conversion as
a substitute for exact SML ANF/PVP Boolean semantics.

## Frozen candidate/control contract (before hardware measurement)

Supported block sizes: 32, 64, 128. The producer is a callback
`FnMut(usize) -> i16`; the fused variant calls it **once per index** and holds
at most 128 source values in a fixed local array. The control is
`quantize_materialized(&[i16], block_size)`, which reads each block to find
maximal absolute value, then reads it again to quantize.

For each block with `m = max(abs(x_i))` (unsigned abs, so `i16::MIN` maps
to 32768), exact scale is `2^e` with smallest integer `e >= 0` satisfying
`m <= 127 * 2^e`. Quantize each value by integer division with
round-half-away-from-zero. The reconstructed value is `q_i * 2^e`.
The quantized code is always in [-127, 127] and zero maps to zero.

OR-envelope candidate: accumulate `h = OR_i unsigned_abs(x_i)`.
Let `w = bit_length(h)` (zero has width zero). Choose `e=0` if
`w <= 7`, otherwise `e=w-6`. This is a **conservative upper bound**
to prevent saturation, not the exact maximal amplitude, and can cause larger
reconstruction error. It may be slower than the maximum comparison. Neither
candidate has established performance superiority.

Each immutable snapshot contains i8 codes and one `#[repr(C)]` two-byte
metadata record per block: `u8 exponent`, `bool active`. The active
flag denotes whether **any quantized code** in the block is nonzero.
A read-only weighted-sum consumer skips inactive blocks. No API provides
mutable access to codes or metadata, avoiding silently stale summary hints.

`logical_bytes = values + 2 * block_count` is only an accounting
formula for logical codes + metadata (no format specification or actual
serializing API). Real allocated bytes include Vec headers/capacities,
alignment and temporary scratch. A zero block still occupies its payload
in this first prototype; a later sparse materialization needs new accounting.

## Correctness gates

1. Exact fused output must equal the two-pass control (codes and metadata),
   across empty, tails, 32/64/128 blocks, random-like and adversarial inputs.
2. Callback invoked once per produced input; invalid block sizes are rejected
   before calling it.
3. OR exponent is never smaller than exact exponent; no saturation; absolute
   reconstruction error <= 2^(e-1) for exponent > 0, and exactly zero for e=0.
4. Cover `i16::MIN`, `i16::MAX`, zeros and all representable i16 values.
5. Consumers with and without inactive-block skipping return the same weighted
   sum; immutable metadata must not be invalidated by external mutation.
6. CI: `cargo fmt --all -- --check`, `cargo clippy --locked --workspace
   --all-targets -- -D warnings`, `cargo test --locked --workspace`.

## Benchmark protocol (not yet executed)

For any latency/DRAM/memory statement, pin repository+benchmark SHA,
runner+CPU+NUMA/OS, compiler flags, input generator, sample count, block size,
warmups, iterations, and statistical interval **before collecting timing**.
Compare the full path including producer, materialization, metadata and
downstream consumer. The baseline must perform the same computation on the
same input. Report end-to-end time, throughput, peak RSS, total allocations,
actual DRAM read/write counters when accessible and valid, and quantitative
error; distinguish cache traffic from DRAM. Replicate on Dell/Thor via
Memorithm/RemoteOps subject to runner/budget availability; no cross-host
latency pooling. Keep zero and outlier cases distinct.

A no-fusion pre-calibrated-scale control can quantify the cost of adaptation,
but it is not accuracy-equivalent on unknown inputs. In the second phase,
compare a truly bitplane-native Boolean producer/consumer under the *same*
declared semantics; do not compare exact Boolean outputs to lossy numeric
outputs as if they were equal.

## Ownership and promotion boundaries

- SML-GENIUS: scientific model authority and research-only reference.
- SciRust: neutral reusable packed layout, SIMD and differential oracle.
- FLAT-ATTENTION: optional portable fused/tiled WGSL kernels.
- TurboQuant: low-bit codec/format semantics; SLHAv2: KV cache policy.
- RemoteOps: protected host execution and immutable evidence.
- No changes to production inference, PVP model semantics, or shared codec.
- No portability, performance, memory or quality claim without matched
  model workload evidence and exact-head required green CI.

Prior art: Dao et al. (2022), FlashAttention IO-aware algorithm;
OCP Microscaling Formats v1.0 (2023); vLLM fusions design documentation.
These references motivate avoiding intermediate materialization, not a claim
of a new result.
