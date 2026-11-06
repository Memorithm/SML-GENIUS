# PCEQ-3 — Sparse i8 payload elision under trusted producer bounds

Status: **research-only** Rust i16→i8 prototype. No runtime promotion, no measured
DDR savings and no model-level quality inference. Builds on PCEQ-0 and PCEQ-2,
with independent reference comparisons.

## Hypothesis and contract

PCEQ-2 can skip reading all source values of `CertifiedZero` blocks but
still writes explicit zero i8 payload to its dense output. PCEQ-3
`quantize_sparse_bounded` omits **all published i8 payload** for two cases:

1. A producer-certified all-zero block. Neither source callback nor any i8
   payload append occurs.
2. A sourced block whose numerical i8 codes all round to zero. Every i16
   source element *is still read exactly once* and checked against its
   producer-provided `MaxAbs` envelope; the quantized zeros are held in
   ≤128-byte local scratch and simply not appended to the published output.

Any active block retains all of its i8 codes, in block order. In contrast to
PCEQ-2 the active candidate temporarily revisits **quantized i8 scratch**
to append codes, not unquantized i16 scratch. This tradeoff reduces
zero-block writes and subsequent payload reads; it may increase CPU time.

Only numerical quantization is represented. Exact Boolean ANF/PVP operations
are not changed; a false producer `CertifiedZero` remains **undetectable**
without reading the source, so such metadata must only come from trusted,
version-coherent and immutable producer semantics.

## Representation and accounting

`SparsePceqI16` has private fields:

- `logical_len` and `block_size` (32, 64 or 128).
- `codes: Vec<i8>`: concatenated full i8 blocks **only when active**.
- `exponents: Vec<u8>`: one binary shift per logical block.
- `active_words: Vec<u64>`: bitmap, block index maps to word `index / 64`
  and bit `index % 64`; word boundary at blocks 63 and 64 is tested.

No per-block pointer index is needed for a sequential consumer; it advances
one payload cursor for each active block. Both `reconstruct` and
`weighted_sum` produce outputs comparable to dense PCEQ-2
`quantize_streaming_bounded` for identical callbacks/bounds.
Inactive blocks skip the entire weight-slice and payload during weighted sum.

`logical_bytes = active_payload_codes + block_count + 8×ceil(block_count/64)`.
This explicitly excludes Rust Vec headers and reserved capacity, peak RSS,
temporary scratch, alignment, memory allocator metadata, and actual DRAM
traffic. For a 64-value block, dense i8 payload costs 64 logical bytes;
a certified-zero block contributes **zero i8 payload bytes**, but still one
exponent byte and an amortized 1/8 bitmap byte. In highly dense data the
extra bitmap is overhead and may reduce performance.

All published fields are immutable through the public API. A client cannot
mutate codes independently from metadata. Atomic replacement of an entire
snapshot is required after any producer revision; no claims of general
lifetime/epoch correctness are made.

## Correctness gates

1. Empty, tail, single and many-block differential parity with the dense
   PCEQ-2 producer-bound implementation.
2. Exact producer callback count for certified-zero and numerical blocks;
   invalid block-size/bound values abort before source production.
3. Bounds underestimated in midstream fail closed and do not publish a
   partial snapshot. Oversized envelopes (>32768) are rejected.
4. Valid numeric input can be nonzero but quantize to an inactive zero block;
   compare reconstructed values, not original i16 values.
5. Validate multiword bitmap and correct sequential payload cursor across
   block indexes 63/64 with a partially filled tail.
6. Validate both reconstructed codes and signed weighted sum against dense
   oracle; weight-count mismatch is rejected.
7. Rustfmt, Check, Clippy and all required workspace tests on exact PR head;
   no merge before gates pass.

## Performance experiment next

Paired same-host Dell/Thor via Memorithm/RemoteOps, CPU only, deterministic
trusted `BlockRecipe` generator with certified zeros, and fixed mixture of
0%, 50%, 90% and 100% eligible zero blocks. Compare:
- Dense PCEQ-2 producer-bound, including the exact same envelope callback.
- Sparse PCEQ-3 producer-bound, including bitmap, local i8 scratch,
  and identical envelope callback.
- Same weighted-sum consumer for both, equal reconstruction semantics.

Record end-to-end time, active-code counts, full output logical bytes,
peak process RSS (where measured), absolute error and checksums. Preserve
fixed seeds, sizes and budgets before first host run. Do **not** infer a
physical DRAM read/write reduction from smaller logical payload alone.
Use PMU counters only after validating hardware counter availability and
the memory-controller event semantics for each host.

This layout is not yet a TurboQuant codec format: publication requires an
independent serialization version, endian and alignment contract,
cross-project ownership review, real model data and scientific promotion.
