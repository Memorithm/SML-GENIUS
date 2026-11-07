# BPB-3 competitive format qualification: GGUF Q3_K / Q4_K

**Status:** reference decoding qualification in progress. No quantizer,
reconstruction error measurement, real-model integration, SML training,
GPU speed, physical memory traffic, novelty or superiority assertion.

## Upstream source of truth (immutable for this qualification)

- ggml-org/llama.cpp commit
  `f7591a6a01f9273de03331732bdc0b39721168be`.
- `ggml/src/ggml-common.h` declares `block_q3_K` and `block_q4_K`.
- `ggml/src/ggml-quants.c` contains the native `dequantize_row_q3_K`
  and `dequantize_row_q4_K` paths.
- `gguf-py/gguf/quants.py` contains the independent published Python
  decoders `Q3_K.dequantize_blocks` and `Q4_K.dequantize_blocks`.

Reference URLs:
https://github.com/ggml-org/llama.cpp/blob/f7591a6a01f9273de03331732bdc0b39721168be/ggml/src/ggml-common.h
https://github.com/ggml-org/llama.cpp/blob/f7591a6a01f9273de03331732bdc0b39721168be/ggml/src/ggml-quants.c
https://github.com/ggml-org/llama.cpp/blob/f7591a6a01f9273de03331732bdc0b39721168be/gguf-py/gguf/quants.py

## Physical byte layout

Both variants dequantize a 256-weight **superblock**, not a 64-weight
BPB-3 block. Actual layouts include packed low-bit codes and **all** the
scale/min metadata, not merely the nominal quantizer bits:

| Layout | Fields / sizes | Total bytes | Effective bits per coefficient |
|---|---|---:|---:|
| Q3_K | hmask 32 + qs 64 + 16×6-bit scales 12 + global FP16 d 2 | **110** | **3.4375** |
| Q4_K | FP16 d 2 + FP16 dmin 2 + scales/mins 12 + qs 128 | **144** | **4.5** |

For Q3_K, each of sixteen 16-value groups gets a signed 6-bit
subscale, stored as an offset `scale - 32`. Each coefficient has two
low bits and a high-mask bit: the decoded integer is
`(low2 - (hmask_bit ? 0 : 4))`; reconstructed value is
`FP16(d) × (scale - 32) × integer`. The 32-bit hmask bytes and two-bit
bitplane order follow ggml's actual layouts.

For Q4_K, eight 32-value groups share a superblock FP16 `d`, an FP16
`dmin`, and individually packed 6-bit scale/min values. The two nibbles
of each quant byte correspond to **distinct groups of 32** (not to
adjacent weights). Reconstruction is
`(FP16(d) × group_scale) × nibble - FP16(dmin) × group_min`.
Six-bit scale/min unpacking differs for groups 0–3 and groups 4–7.

`gguf_k.rs` is a standalone Rust decoder with **exact length checks**.
It does not parse the GGUF container header, tensor dimensions,
alignment or per-tensor metadata. Any tensor-level storage accounting
must add those costs to these physical block byte counts.

## Verification gates

The GitHub-hosted workflow
`.github/workflows/boolean-posit-gguf-k.yml` runs under Rust 1.89
without NVIDIA software. It:

1. Compiles Rust scalar tests for byte sizes, FP16 signed zero/subnormal,
   indexed bitplanes and strict rejection of wrong block lengths.
2. Generates 5 deterministic, deliberately nontrivial raw byte fixtures
   for each format, including zero, negative, positive and subnormal
   FP16 scales. **This is synthetic format data**, not quantized model
   weights.
3. Fetches exactly the official upstream SHA, installs an isolated
   Python numpy dependency, then compares **all 2560 decoded f32
   bit-patterns** to the official `gguf-py` decoder.
4. Publishes the marker and complete parity vectors as CI artifacts
   only on successful reference verification.

The upstream code in this CI is fetched and imported for independent
correctness checking; it is *never* installed as a resident service or
used as a GPU backend on a local host.

Passing this gate qualifies **decoder agreement on these fixtures**,
not a ggml/GGUF-native quantizer, nor fidelity on real weights.
Randomized future fixtures and real GGUF files remain separate gates.

## Next milestone: reconstruction errors (after format acceptance)

The reconstruction programme must use a genuine `original float weights`
versus `quantize → bytes → decode → reconstructed float weights`
comparison, not the exactness of an already-quantized vector.

For Q3_K and Q4_K the initial acceptance criteria are:

- Compare GGUF outputs from the **upstream ggml reference encoders**
  to both official and Rust decoders, on licensed, SHA-pinned,
  disjoint TRAIN/DEV/TEST weights. A handcrafted bitstream alone cannot
  qualify quantization quality; **no real GGUF reconstruction score is
  reported until actual encoder output exists**.
- Measure RMSE, normalized RMSE (explicit denominator), mean absolute
  error, p95/p99/p99.9 absolute error, maximum absolute error,
  saturation/outlier error, signed bias, cosine similarity and
  bipolar/floating-input dot-product error distributions.
- Use the same *original* weights and held-out inputs for BPB-3,
  INT3, INT4, MXFP4, Q3_K and Q4_K, with a frozen deterministic
  stratification by scale, outliers, sparsity and row shape.
- Compare group 256 honestly against group 64: include headers,
  scales, alignment, padding and metadata; report actual bytes and
  bits/weight for every candidate. Report matched-byte comparisons
  separately from unconstrained quality comparisons.
- Choose candidates and thresholds on DEV; keep final TEST untouched,
  report paired per-block uncertainty and negative controls; never
  optimize on the final TEST or attribute model/token quality without
  a trained-model evaluation.

All model data access and physical machine execution are separate
RemoteOps-admitted tasks; this change performs no local remote dispatch.
