# SmolLM2-135M-Instruct — BPB3-K first real trained-model gate (2026-11-08)

Status: **preregistered, not yet measured when authored**. Research only.
Owner of scientific validity: SML-GENIUS. Physical runner admission: RemoteOps.

## Why this model

- Original trained model: HuggingFaceTB/SmolLM2-135M-Instruct (134.5M parameters, Apache-2.0; transformer Llama architecture). This is an **external test subject**, not an SML architectural dependency.
- Public quantized control: bartowski/SmolLM2-135M-Instruct-GGUF, **Q3_K_S**, 88.2 MB, exact published SHA-256
  `7add77b8d3736d6b2fd21dc96e69be026f19bde843e9988b43cef1315fc5eebe`.
- Planned competitor baseline: same model's optimized llama.cpp Q3_K_S, Q4_K_M, Q8_0 in real complete decoding with paired prompt sets, with exact source hashes, CPU threading, GPU backend, prompt/eval context, tokens per second and perplexity.
- Q4_K_M published SHA-256:
  `2e8040ceae7815abe0dcb3540b9995eaa1fa0d2ca9e797d0a635ae4433c68c2d`.
  Q8_0 published SHA-256:
  `5a1395716f7913741cc51d98581b9b1228d80987a9f7d3664106742eb06bba83`.
- Sources (8 November 2026):
  https://huggingface.co/HuggingFaceTB/SmolLM2-135M-Instruct
  https://huggingface.co/bartowski/SmolLM2-135M-Instruct-GGUF

## Gate A — already implementable from published SML kernels

SML's Rust program `research/boolean-posit/smollm2_real_gguf.rs` parses
v3 GGUF conservatively, locates the first three physical Q3_K tensors
in file order, and samples up to 512 uniformly indexed 256-weight
superblocks per tensor. It checks the original Q3_K -> BPB3-K mapping
against independent Q3_K bipolar integer dots on eight fixed input masks,
and f32 reconstruction parity on the first 32 sampled blocks.

Twelve alternated Q3_K byte-addressed-vs-BPB3-K POPCOUNT kernel scans
are a **diagnostic only**; they are not a valid competitive inference
comparison. The experimental arm transcodes weights once before scans
and reports conversion cost separately; no hidden dense expansion.
Both arms consume identical original pre-quantized model coefficients.

Strict evidence marker is `PASS_REAL_MODEL_ORACLE`. Any missing Q3_K
tensor, unsupported GGUF metadata, model SHA mismatch, bad length,
numerical mismatch, bad source provenance, or resource gate fails closed.
All absent evidence is `NOT_MEASURED`, not a pass.

## Gate B — competitive real LLM evaluation, NOT YET IMPLEMENTED

The user-requested claim requires **real end-to-end inference using
BPB3-K as the live parameter representation**, without silently routing
to Q3_K or the ggml kernel. Because BPB3-K preserves Q3_K reconstruction
exactly but changes its physical layout, the quality target is strict
Q3_K-equivalence, subject to explicit floating reduction policy.
Measure with predeclared text/test corpus: perplexity or log-probability,
context-length dependent prompt processing, generated tokens/s, RSS,
physical bandwidth/energy where available, and all transcode/storage
overheads. At least 12 paired counterbalanced repetitions in a quiescent
window, strongest optimized Q3_K and Q4_K/INT8 baselines, pinned compiler
flags and upstream ggml commit, bootstrap paired confidence interval,
sham drift rejection and all negative arms retained. GPU kernel-only
timing and CPU access measurements are distinct evidence. The method
must not assume that bipolar microkernels apply to ordinary nonbipolar
activations. A Q3_K baseline run *without a BPB3-K full inference path*
cannot establish a winner.

Gate A may proceed on GitHub-hosted infrastructure. All Dell/Mac/Thor
experiments require RemoteOps admission, no interruption of active
services, and no NVIDIA vendor SDK. Do not merge or promote until CI
required on the exact source head is green. Do not present a token/s
claim before Gate B is actually implemented and measured.

References: SML-GENIUS issues #858 and #878; existing
`research/boolean-posit/BPB3_K_DIRECT2_PAIRED_CONTROL.md` and
`PAPER_BPB3_WORKING_v0.1.md`.
