# BPB3-K Q5-178: source-precision GGUF v3 converter

Status: **experimental converter implementation, not a llama.cpp runtime integration**.
This is a separate Rust program `bpb3k_q5_gguf_transcode.rs`, building on the source FP16/BF16/FP32 quantizer and AVX2 tiled microkernel.

## Effective functionality

- Read bounded GGUF **version 3** metadata and tensor directory; validate entry counts, finite file length, general.alignment, UTF-8 names, offsets, tensor dimensions and source payload extent.
- Select tensors with rank 2–4, `shape[0] % 256 == 0`, `elements % 256 == 0` and **source storage type F16 (1), BF16 (30), or F32 (0)**. This avoids repacking 1-D normalization/bias vectors or matrices with incompatible row widths. At least one eligible tensor must be present.
- Quantize each contiguous source-precision 256-value block using the native Q5-178 block encoder; no intermediate Q3_K or Q4-146 weights. Preserve all nonselected tensor bytes exactly, including source padding.
- Preserve tensor names, shapes, source metadata and remaining fields. Rewrite **only** the converted matrix type and all tensor offset header fields; append aligned Q5-178 payloads and relocate remaining tensors.
- Use experimental type `0xffff0005` deliberately **not Q3_K (11) or Q5_K (13)**. This intentionally prevents a stock GGUF loader from silently interpreting nonstandard 178-byte blocks. A separate source-pinned ggml adapter is required before full-model inference.
- Reject malformed inputs, nonfinite values, invalid alignment/source ranges, zero/misaligned dimensions, preexisting destination, oversized source/output, and unknown eligibility. Remove any incomplete destination on transcode failure; leave the original GGUF untouched.

The converter does not claim restored precision for prequantized Q3_K models and does not rewrite standard GGUF `general.file_type` to pretend the experimental format is compatible.

## Source tests and evidence

The dedicated workflow compiles and executes the actual Rust program on GitHub-hosted CPU against a deterministic **GGUF v3 envelope containing 5 tensors**, of which F16, BF16 and F32 matrices are converted. It verifies independent source quantization, output byte ranges, correct relocated offsets and type codes, byte-identical untouched Q3_K payload/vector, immutable input, and fail-closed truncated/nonfinite/corrupted metadata. Every candidate full-model runtime must separately verify all-vocabulary logits/token IDs and real prefill/decode speed.

```bash
rustc --edition=2021 -D warnings --test research/boolean-posit/bpb3k_q5_gguf_transcode.rs -o /tmp/q5-gguf-tests
/tmp/q5-gguf-tests --nocapture
rustc --edition=2021 -D warnings -O research/boolean-posit/bpb3k_q5_gguf_transcode.rs -o /tmp/q5-gguf-convert
/tmp/q5-gguf-convert transcode original-F16-BF16-F32.gguf EXPERIMENTAL-Q5-178.gguf
```

**Do not run the converter on operational models or deploy its output**: it is a research-only, explicitly nonstandard format. The same-source full-model Q5_K, Q4-146 and FP16 baselines remain to be implemented, as does the pinned ggml model runtime adapter and physical Dell/RemoteOps qualification. No existing Q4 or production code path is changed.
