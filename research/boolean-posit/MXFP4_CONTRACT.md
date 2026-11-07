# BPB-3 adversarial baseline: OCP MXFP4 E2M1 + E8M0

**Status**: standalone scientific research, **not** SML-model integrated, and **not** a trained-model quality result. The active code is `mxfp4.rs` + `quantization_panel.rs`. The existing BPB-3/INT2/INT3/INT4 comparisons remain unchanged.

## Primary specification

Open Compute Project, *OCP Microscaling Formats (MX) Specification*, version 1.0, September 2023, §§ 5.2, 5.3.3, 5.4.1:
https://www.opencompute.org/documents/ocp-microscaling-formats-mx-v1-0-spec-final-pdf

The **MXFP4** logical payload comprises 32 FP4 E2M1 coefficients (4 bits each) sharing one E8M0 8-bit scale. The `mxfp4.rs` reference stores one 32-element group in **17 bytes**, scale followed by 16 little-nibble-first packed bytes; 64 elements are two independent 17-byte records, **34 bytes**, equivalent to **4.25 bits/weight**, without extra tensor-level alignment. The array ordering is an explicit local wire convention, not a claim about device-specific tensor layouts or tensor-core hardware support.

E2M1 finite magnitudes for positive codes 0–7: `[0, 0.5, 1, 1.5, 2, 3, 4, 6]`; bit 3 is sign. Conversion rounds to nearest with ties-to-even and saturates to ±6. A signed zero is encoded faithfully. The E8M0 scale byte is an exponent with bias 127: finite bytes `0..=254` represent `2^(code−127)`; `0xFF` represents NaN scale and is explicitly rejected in this finite-only research evaluator. The scale does **not** represent zero.

## Policy-specific distinctions

- This implementation's **decoder**, E2M1 codebook, NaN rejection, and 32-value shared-scale grouping follow the OCP format. The local `[scale; packed_nibbles]` ordering is the specified experimental container.
- The **encoder** searches all integer power-of-two exponents `[-24, 27]`, separately for each 32-element block, selecting the candidate with minimum squared reconstruction error and reproducibly choosing the first smaller exponent on ties. That bounded optimal-SSE **experimental selection policy is not prescribed by the OCP specification**. We use the same exponent search limits as the original BPB-3/INT baselines, but MXFP4 receives two scales per 64 coefficients rather than one; the budget is consequently 34 bytes rather than 26.
- The panel contains frozen synthetic families (uniform, sparse, outlier, logspread) and fixed-seed bipolar inference masks. Quantization error compares **to original floating-point coefficients**, not already-quantized INT8. The numerical dot-product error is evaluated against the same real-weight scalar oracle; no GPU throughput is being measured here.
- The format-level budget does not include any tensor padding, alignment, row metadata, or hardware-specific block swizzling. Those must be counted separately in an end-to-end layout or deployed-model comparison. Nor are GGUF Q3_K/Q4_K implemented by this addition: those remain **NOT_TESTED**.

## Reproduction and rejection

```sh
rustc --edition=2021 --test research/boolean-posit/quantization_panel.rs -o /tmp/bpb3-quality-test
/tmp/bpb3-quality-test
rustc --edition=2021 -O research/boolean-posit/quantization_panel.rs -o /tmp/bpb3-quality
/tmp/bpb3-quality
```

The dedicated GitHub Actions workflow uses Rust 1.89 and archives the CSV. Format tests pin all 16 E2M1 codes, ties-to-even midpoint choices, both independent 32-element scales, precise nibble ordering, the complete finite E8M0 range, explicit NaN-scale rejection, and exact realized per-block bytes. Research/quality evidence is independent of any untrained SML-GENIUS model or FLAT GPU kernel.

## Work still required

Matched float-scale INT4 candidates with independent full-range quantization; real GGUF Q3_K/Q4_K byte contracts and reconstruction; disjoint frozen TEST weight corpora; full tail/cosine and dot-product distribution; calibrated cross-quantizer hyperparameters selected only on DEV; physical GPU kernels with comparable implementation effort; and complete memory accounting. No ranking from this synthetic panel alone is qualified as a general superiority claim.
