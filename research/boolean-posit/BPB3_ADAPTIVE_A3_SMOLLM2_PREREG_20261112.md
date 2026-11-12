# BPB-3-A3: independent-model source quantization and same-mask signed-dot preregistration

Date frozen: 2026-11-12. This contract is committed **before looking at A3 held-out model weights or metrics**. It does not change A1 tuning, codebooks, 2/3/4-bit modes, or gate criteria.

## Independent architecture and immutable provenance

- Model: [HuggingFaceTB/SmolLM2-135M](https://huggingface.co/HuggingFaceTB/SmolLM2-135M), **pretrained** rather than instruct, Llama-family architecture; distinct from the earlier EleutherAI Pythia-14M GPT-NeoX architecture. License: Apache-2.0, according to the published model card.
- Repository snapshot: `93efa2f097d58c2a74874c7e644dbc9b0cee75a2`.
- Exact `model.safetensors` full-file SHA-256: `80521b40281d6ce74e35c9282c22539e75aa0ac8578892b2a59955ef78d55da1`, from the [model-file pointer](https://huggingface.co/HuggingFaceTB/SmolLM2-135M/blob/main/model.safetensors). Abort on any different bytes. No pickle.
- Tensor strata frozen now, all from **one independent model**:
  `model.layers.2.self_attn.o_proj.weight`,
  `model.layers.2.self_attn.q_proj.weight`,
  `model.layers.3.mlp.up_proj.weight`,
  `model.layers.3.mlp.down_proj.weight`.
  The file reader must verify each declared tensor is rank-2, finite, and its contiguous flattened width divisible by 256. Supported stored float dtypes: F16/BF16/F32 with exact widening to FP32. Missing tensor or incompatible dtype: abort, never silently substitute a layer.
- Per tensor: take 24 disjoint DEV and 24 disjoint TEST blocks of 256 weights after deterministic shuffle of all nonoverlapping contiguous blocks, seeded by SHA-256 of `bpb3-a3-smollm2-135m-fresh-v1-20261112` plus tensor name. Freeze the manifest with selected block indices and SHA-256 of each transcribed source file. This reblocking is **not** a claim of correct GGUF tensor layout.

## Exact algorithm, budget and paired measurement

- Read-only use of SML-GENIUS BPB-3-A1 `bpb3_adaptive.rs` **without changing codebooks or scales**: 64-coefficient mode-1/2/3 bitplanes, 19/27/35 B per block, scale exponent -24..27; optimize precision per tensor and split with the existing bounded exact dynamic program under **108 B / 256 original weights**. Tagged fixed mode-2 BPB is **also 108 B / 256**; the untagged historical 104 B format is not an equal-budget control.
- Official unmodified GGML `ggml_quantize_chunk` encoder at `f7591a6a01f9273de03331732bdc0b39721168be`, Q3_K = 110 B / 256 weights; Q4_K = 144 B / 256. C/Rust dequantizer agreement verified. Every codec sees the same original FP32-widened weights, before quantization.
- Exactly 24 rows × 256 coefficients per tensor per split. Two tiers of metrics:
  1. reconstructed-source per-coefficient RMSE; `pooled_RMSE=sqrt(sum_sse/sum_weight_count)` across all four predefined matrices, not arithmetic mean of RMSE.
  2. signed bipolar 256-coefficient dot RMSE with **16 predetermined identical input masks per row**, generated once from `0xa3b0_114d_5c15_2026u64` and a stable xorshift64* generator, reused for each codec. Dot computation is in f64 using original widened FP32 values and reconstructed values. Quantized Boolean `dot` must match independent dense signed-integer dot *exactly* for each block before any error measurement. Native Q3/Q4 uses independent official C encoding, Rust decoding, and C decoder checks. Output includes sample counts and serialized budget per method.
- DEV is for analysis; **no hyperparameter selection, model/tensor exclusion, or TEST-adaptive threshold** is permitted after inspecting data. Report all four TEST matrices and pooled results, including negatives; no confidence interval is implied by deterministic paired sample rows.
- Explicit outcomes: A3 source reconstruction and matched-dot evidence may qualify, but only independently replicated second-model **components**, not language-model loss/perplexity, downstream accuracy, end-to-end speed, DRAM traffic or Posit Standard 2022 compliance. If A3 does not beat Q3_K, retain the negative outcome.

## Execution and safety

GitHub-hosted CPU-only job, pinned Rust 1.89, NumPy + Hugging Face client, no model uploads to artifacts, only fixed manifest/CSV/SHA and source revision. No unattended workloads on physical Dell, Thor, Mac or production services. Tests required on exact PR head before merge; record hashes and partial/incomplete outcomes without cherry-picking.
