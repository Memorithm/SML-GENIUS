# Q5-178: pinned actual SmolLM2-135M-Instruct F16 experimental canary

Status: **implemented candidate workflow, not yet accepted as a real-model demonstration**. Keep native FP16 inference, Q4-146 research and original Q3_K unchanged.

## Original pretrained source and checksums

- Exact original source model: `bartowski/SmolLM2-135M-Instruct-GGUF`, `SmolLM2-135M-Instruct-f16.gguf`, repository revision `c6f2d396a5faa22c9722908d47e8da81a5d8cebc`.
- Source model SHA-256 expected: `f535f83ec568d040f88ddc04a199fa6da90923bbb41d4dcaed02caa924d6ef57` (Hugging Face Xet pointer metadata), verified by sha256sum before any quantization.
- Exact unmodified upstream llama.cpp: `f7591a6a01f9273de03331732bdc0b39721168be`, separate stock and temporary Q5 patched checkouts.
- F16 model is substantially different from the previously used 82 MiB Q3_K_S GGUF; **do not pool scores or treat Q3-to-Q5 transcoding as independent precision gain**.

## Work performed by the hosted-CPU canary

1. Pin and hash source F16 GGUF; no credentials and no uploaded model.
2. Run the actual Rust GGUF converter from **F16 source tensors**, not the old Q3_K corpus, then strict source validation and sentinel-to-temporary type-11 preparation (reject any unpatched stock Q3 mixtures).
3. Build both unmodified and patched CPU llama-cli, llama-debug, llama-bench binaries from the same SHA. Rust Q5 FFI must actually execute, and long prompt processing must hit the true fifth-bit tiled microkernel.
4. Compare full-vocabulary logits on two identical prompts and eight greedy decode steps. **Do not compare numeric logits after a token divergence** because input contexts have diverged. The new Rust reporter emits `NA` for incomparable steps while retaining all token IDs, max-absolute and RMSE for comparable steps. Reject nonfinite/missing dumps, malformed prompts or invalid greedy choices.
5. Run a bounded four-pair alternating CPU throughput pilot and record conversion time, command runtime and peak RSS. These four pairs are exploratory and cannot establish general superiority.
6. Upload only operational logs, hashes and reported metrics; never upload pretrained model, temporary Q5 GGUF, weights or full raw logits. No production deployment and no physical Dell use.

**Potential blocker:** exhaustive signed i8 subgroup scale optimization has nontrivial conversion cost for 135M source weights. A 1,800-second timeout deliberately rejects an impractical conversion; do not silently weaken this limit or infer model performance from partial conversion. Later optimizations must demonstrate a bounded quality-cost tradeoff on held-out source weights and compare full model against stock F16 and standard Q5_K.

## Admission

Success requires model SHA, exact-host builds, actual Q5 dot and Q5 tiled markers, finite full-vocabulary observations, and completed paired pilot. A negative quality or throughput result must be retained in the published report. Passing this workflow is **not** a production promotion nor a guarantee of speedup/quality. The next physical tests require Memorithm/RemoteOps scheduling, workload isolation and original/binary SHA preservation.
