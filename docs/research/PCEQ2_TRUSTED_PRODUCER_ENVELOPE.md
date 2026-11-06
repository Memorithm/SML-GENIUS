# PCEQ-2 — Zero unquantized rereads by a producer-proven envelope

Status: bounded research candidate, not a production feature and not a performance result.
Extends PCEQ-0 (#829, #830), separated from PCEQ-1 native timing (#831).

## The missing mechanism

PCEQ-0 fused producer avoids materializing an entire i16 tensor, but
**still re-reads a small local scratch block** after discovering its exact
maximum. Eliminating this last second read requires a scale to be known
before the first numerical value is produced. There is no generic exact
algorithm for doing so for arbitrary unknown data without a prior
guarantee. PCEQ-2 therefore introduces a **trusted producer contract**:

- `ProducerBound::MaxAbs(m)`: producer commits that all absolute i16
  magnitudes in a block are at most `m <= 32768`. The scale can be
  computed from m before production. Every emitted value is checked
  against its bound, then immediately quantized directly into the final
  i8 payload. **No unquantized local scratch is stored or revisited.**
  Underestimation aborts with block index, sample index and observation,
  and no partially built snapshot is returned.
- `ProducerBound::CertifiedZero`: a *trusted immutable producer* knows
  the entire block is zero from its own semantics (e.g. previous
  validated block-activity representation). In this case the source
  callback is **not invoked for any member** of the block. This is not
  a self-verifying certificate: a false or stale assertion silently
  changes the result, so consumer-facing use requires producer identity,
  bound origin and version coherence. No untrusted external metadata.

For every block, one source-bound callback occurs before generating any
element. `MaxAbs` values are verified while streaming. For `CertifiedZero`,
zeroes are written to the i8 output (not sparse-elided in this prototype),
and inactive metadata is attached.

## Mathematical contract

For each block with promised upper bound M, choose the smallest integer
`e >= 0` such that `M <= 127·2^e`; quantize `q = sign(x) ·
floor((abs(x) + 2^(e-1)) / 2^e)` for e>0 and q=x for e=0.

The bound is safe but an overestimate can increase error. At exact
bound, the codes and sidecar must equal the two-pass reference. At a
broader bound, absolute reconstruction error is at most half the
quantization step (for the stated rounding rule), without saturation.

The i16 input is a deliberate fixed-point experiment. These claims
**do not** extend to arbitrary FP32, NaNs/Inf, ANF/PVP exactness or
real-model quality.

## Correctness, resource and evidence gates

1. Exact bound derived from an independent fixture must give
   byte-for-byte equivalent snapshot to `quantize_materialized`
   on empty, tail, full, zero, positive and negative blocks.
2. No input callback for certified zero; no second callbacks for the
   remaining blocks; one bound callback per block.
3. Bound >32768 and underestimated bounds fail closed with no published
   snapshot. Invalid block size aborts before any callback.
4. Coarse and exact bounds both preserve code range and error limits.
5. Test a stale/false certificate in a *separate adversarial contract
   investigation* before permitting any live model path. Because zero
   blocks are not read, **the quantizer cannot independently detect a
   false zero claim**; responsibility must stay with the trusted producer.
6. No added dependencies, unsafe code, new persistent service, runtime
   promotion, or changed SML model semantics.
7. CI exact PR-head: Rustfmt, Clippy and workspace tests.
8. Physical benchmark only after PCEQ-1 native paired baseline and
   proper same-host measurement plan, with metadata production costs
   included and no unqualified DRAM/quality claim.

## Cross-project boundary

- SML-GENIUS: numerical experiment and model-scientific decision.
- SciRust: potential generic trusted-bound type/bitpacking primitives,
  after shared equivalence and licensing checks.
- TurboQuant: can evaluate producer-carried block-scale hints for
  codec format only through its own versioned contract.
- FLAT-ATTENTION: future portable fused producer-consumer kernels.
- RemoteOps: only host enforcement and evidence execution.

The potential benefit is avoidance of **a second local scratch read**
and even producer work on guaranteed zero blocks. It is not proven
faster on Dell/Thor and makes no claim about unrecoverable information.
