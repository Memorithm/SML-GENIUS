# BPB3-K: equivalent GGUF Q3_K numeric reconstruction in Boolean bitplanes

**Research only. This is not the established BPB-3/v0 format.**
This experiment addresses the negative result of issue #853's E1 trial:
on real trained Pythia-14M weights, a fixed BPB-3+two-FP16-residual
candidate was still 36–43% higher RMSE than official GGUF Q3_K at
the same 110 bytes/256-coefficient budget. No superiority claim is made.

## Exact byte budget, no free metadata

- Q3_K original: 32-byte hmask + 64-byte 2-bit codes + 12-byte
  sixteen 6-bit signed subgroup scales + 2-byte global FP16 scale
  = **110 bytes/256** = 3.4375 bits/weight.
- BPB3-K: four groups × (3 × 8-byte Boolean bitplanes) = 96 bytes
  + **the same 12-byte subgroup scales and 2-byte global FP16**
  = **110 bytes/256** = 3.4375 bits/weight.
- Sign plane 0 encodes positive 0..3 using the remaining two magnitude
  planes; sign plane 1 encodes negative -1..-4 using magnitude minus one.
  **Unlike standard BPB-3 v0**, this exposes the signed alphabet
  `[-4,3]` necessary for exact Q3_K numeric parity.
- Q3_K's 16 local signed 6-bit subgroup scales are preserved exactly:
  converting Q3_K to BPB3-K changes **layout only**, not scales,
  rounding, quantization coefficients, reconstruction or bit budget.
- A bitplane vector can represent each 64-coefficient group with three
  unsigned 64-bit values. The speed and physical memory traffic of this
  Boolean layout are **not** qualified and no GPU kernel is included.

## Scientific interpretation

The transcode `q3_to_bpb3k` consumes **actual Q3_K wire bytes**.
`decode_bpb3k` reconstructs each FP32 coefficient and is compared bitwise
against the validated `gguf_k::decode_q3_k` oracle over 25 independent,
deterministic raw byte fixtures (all bitplanes and all encoded subgroup
metadata). This qualifies a new wire decoder, not an independent
trained-model quantizer. If future real official Q3_K-encoded weights
are transcoded, their reconstructed values must remain exactly equal:
**this E2 branch solves the gap to Q3_K's reconstruction numerically by
inheriting its metadata scheme**, not by discovering a better quantizer.

No gains versus the official Q3_K encoder, FP32 training, a model's
downstream quality, or energy use follow from codec equivalence.
Matching Q3_K is a baseline for future optimized scale selection
and bitplane execution. When testing further mutations, require new
untouched checkpoints/layers for final TEST; the original Pythia-14M
layer 0/5 TEST from issue #853 is already observed.

## Primary pinned reference

ggml-org/llama.cpp `f7591a6a01f9273de03331732bdc0b39721168be`:
`ggml/src/ggml-common.h`, `ggml/src/ggml-quants.c`,
`gguf-py/gguf/quants.py`. Linked to [issue #853](https://github.com/Memorithm/SML-GENIUS/issues/853).
