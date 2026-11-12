# BPB-3-A8: exact signed INT8 activation path, 2026-11-12

**Status:** implementation candidate requiring hosted Rust CI and independent quality evaluation. It uses the original BPB-3-A8 packed bitplanes and exact source quantization; it does not claim Posit Standard compliance, full-model quality, end-to-end inference, memory-traffic reduction or throughput superiority.

## Motivation

The original A8 tensor kernel only accepts one-bit signed **bipolar** activations, `x_i ∈ {-1,+1}`. That is insufficient to directly represent ordinary low-bit learned activations. This change adds an **exact signed INT8 activation dot** over the same 256 stored A8 coefficients per row without materializing an FP32 weight vector.

The calculation is based on two's-complement INT8 decomposition:

```
x = Σ_{b=0..6} 2^b x_b − 128 x_7,   x_b ∈ {0,1}
s_b = 2x_b − 1,                      s_b ∈ {-1,+1}
D_b = Σ_i w_i s_(b,i)
S   = Σ_i w_i

2 Σ_i w_i x_i = Σ_{b=0..6} 2^b D_b − 128 D_7 − S
```

Every D_b and S uses the existing **signed POPCOUNT** Boolean A8 kernel and checked i128 fixed-scale arithmetic in exact multiples of 2^-24. The standalone `dot_i8_exact` reference uses **nine** A8 bipolar dot evaluations (eight input planes and one constant correction), rather than a single 1-bit dot. The separate **fused** A8 implementation `dot_i8_prepared` parses the packed weight row **once**, reuses the eight local INT8 activation bitplane masks across all matrix rows and computes signed contributions using bitwise AND, POPCOUNT and checked integer arithmetic. We deliberately **do not claim faster execution** without paired native timing. The advantage is functional generality with exact correspondence to the **quantized** A8 weights and **all 256 signed INT8 activations**. The algorithm must not be presented as exact for the original floating-point model weights.

`dot_i8_exact(wire, input)` remains the independent nine-pass bitwise reference. `a8::dot_i8_fused` and `a8::dot_i8_prepared` implement a separate single-parse signed POPCOUNT kernel in the original A8 module. `matvec_i8(view, input)` and `matvec_i8_into(view, input, output)` use the fused prepared bitplanes for all original tensor rows, and support reusing the output buffer during repeated evaluations. All outputs remain exact 2^-24 fixed-scale integers.

## Safety and qualification

- Fail closed for any malformed original A8 wire; never substitute a scalar approximation.
- Check arithmetic overflow and integrality of the exact Boolean reconstruction.
- Validate the whole two-level A8 tensor directory once using its existing canonical parser and charge the same page plus tensor-directory overhead.
- Reject wrong-sized caller-owned outputs **before** mutation.
- Test three-way equality of the **independently decoded dense signed INT8 scalar oracle**, the nine-pass Boolean reference and the fused single-parse POPCOUNT kernel, with several scale exponents, 1/2/3-bit local magnitudes and eight input fixtures, including `-128`, `127`, alternating extremes and seeded byte patterns.
- Qualify a 257-row/two-page tensor matvec and reusable outputs versus the independent dense integer oracle.

The standalone Rust 1.89 source is `bpb3_a8_int8.rs`; the GitHub-hosted workflow `bpb3-a8-int8.yml` tests it at the exact PR head without any Dell/Thor/Mac runner or vendor-specific SDK.

## Next engineering gate

After exact three-way correctness is accepted, benchmark the new fused single-parse path against the nine-pass reference and native Q3_K/Q8_K at equal resident storage, then add full-model logits, held-out text loss, latency and physical memory traffic. The nine-pass path remains a correctness reference. **Neither** path is yet a production kernel or validated faster.
