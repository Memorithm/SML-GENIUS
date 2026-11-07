# Issue #853 — E1 paired original-weight reconstruction, E2 exploratory control

Status **research_only**. This is an **executable** model-independent study,
not an assertion that quantizer output from an existing trained model was
accessed. It uses *real official GGML Q3_K and Q4_K encoder functions* on
declared **synthetic original f32 weights** (or optionally an external, licensed
`.f32le` dataset supplied by an authorized study runner).

## Frozen upstream and boundary

- ggml-org/llama.cpp **`f7591a6a01f9273de03331732bdc0b39721168be`**
- Functions `ggml_quantize_chunk(GGML_TYPE_Q3_K/Q4_K)` from the pinned
  `ggml-base` library (original C implementation, importance matrix null),
  with `ggml_type_name`, `ggml_type_size`, `ggml_blck_size` checked in Rust
  to fail closed on ABI/layout changes. The official C dequantization functions
  are compared numerically to the existing Rust `gguf_k.rs` byte decoder.
- `GGUF Q3_K`: 110 bytes/256 weights = 3.4375 b/w; `Q4_K`: 144 bytes/256
  = 4.5 b/w, **including superblock scales and minima**. This code does not
  create a whole GGUF file or measure resident DRAM traffic.
- BPB-3: 4 × 26-byte serialized 64-weight blocks = 104 bytes/256 = 3.25 b/w;
  same `[-24,27]` SSE search as `quantization_panel.rs`, unpacked through
  the independently tested `Bpb3` implementation.
- E2 **fixed** candidate: `BPB-3 + 2×(u8 position, FP16 signed additive
  residual)` per 256-coefficient superblock = 104+6 = **110 bytes/256**,
  exactly the Q3_K serialized byte budget (3.4375 b/w). Positions and signed
  residuals are fully counted; no free outlier correction is assumed.
  The largest two reconstruction residuals are chosen per superblock as part
  of the quantization algorithm, and each is rounded to FP16 nearest/ties-even.
  The number of residuals is fixed **before inspecting the results**; it is not
  selected to optimize TEST. This is a **new experimental format**, not the
  original BPB-3 v0 or a final standardized encoder. It has no GPU kernel.

## Frozen deterministic synthetic splits

Four independently seeded families (`uniform`, `sparse`, `outlier`,
`logspread`) with 24 independent 256-element original-weight rows in each of
`DEV` and `TEST`. Seeds are stable, disjoint per family and split; each row
receives identical quantizer inputs and the same deterministic set of
16 bipolar dot-product masks across all formats. Do not merge the two sets
in candidate selection. All values must be finite. This is deliberately
**not** a real model and has **not** been validated on a licensed weight corpus.

A future authorized runner may provide an immutable, licensed original
f32le binary file by passing `--external-f32le /absolute/file.f32le`; the
program accepts only complete rows (256×4 bytes), requires at least two,
and deterministically separates its rows into DEV/TEST without retuning.
The external caller must independently supply license, source URL, exact
SHA256, tensor name/shape and leakage controls. The tool labels all such
outputs `EXTERNAL_UNVERIFIED` until this external provenance gate is complete.

## Evidence output and numeric definitions

CSV columns are paired by `split,family,codec`; each codec operates on
**the same original input row**. Error `e_i = reconstructed_i-original_i`.

- `RMSE = sqrt(mean(e_i²))`;
  `nRMSE = RMSE / sqrt(mean(original_i²))`, with zero-denominator
  status explicitly treated as 0 in this initial reference (zero norms must
  be excluded from any publication of normalized outcomes).
- `MAE=mean(abs(e_i))`, `signed_bias=mean(e_i)`,
  nearest-rank p95/p99/p99.9 absolute error, worst index and max absolute
  error. `zero_abs_mae` is conditional on exactly zero original weights.
- `cosine = (orig·recon)/(||orig||₂||recon||₂)` over all paired
  coefficients, with a zero-norm fallback value of 0; do not interpret
  zero-norm rows as having meaningful cosine similarity.
- `dot_rmse` is the RMSE of paired, original-versus-reconstructed
  256-element bipolar dot products over 16 fixed masks per row. It is
  **not** a model-inference accuracy or a GPU performance metric.
- The `wire_bytes_per_256` column includes only the exact codec payload
  and explicit scale, mask and residual fields; tensor padding, GGUF
  container header and higher-level shape/indexing metadata are excluded
  because they are identical or unspecified at this stage. Do not label
  these figures as physically measured memory reads.

CI pins Rust 1.89 and upstream GGML, compiles only `ggml-base`
without CUDA/Metal/Vulkan backends, verifies C and Rust reconstructions,
runs Rust E2 boundary tests, and retains inspectable CSV + SHA provenance.
The data families are used as **debug/feasibility controls only**.
Any performance claim on production weights still requires a licensed,
cryptographically frozen independent TEST corpus and model-layer analyses.

## Planned E2 ablations (not implemented/qualified by this E1 PR)

Scale quantiles vs SSE, outlier count 0/1/2/3 at *fully costed* bits,
block widths 32/64/128/256, mixed precision, and actual GGUF-native
remnant coding. Select these hyperparameters on DEV only, then report
independent TEST outcomes once. The paired `Q3_K` and
`BPB-3+2residual` 110-byte codecs are a directly interpretable first
feasibility comparison, **not** a conclusive superiority claim.
