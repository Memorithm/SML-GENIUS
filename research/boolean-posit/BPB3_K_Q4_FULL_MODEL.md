# BPB3-K Q4-146: actual entire trained SmolLM2 inference versus native Q3_K

Status: prospectively frozen full-model research experiment, NOT a result or
production promotion. SML-GENIUS owns the scientific conclusion.
Memorithm/RemoteOps alone admits Dell/Thor physical host access.

## Precise qualification question

Does a **real complete SmolLM2-135M-Instruct** run with **all its original
Q3_K quantized tensors losslessly converted** into the 146-byte Q4
representation and consumed by a **live Rust AVX2 dot implementation**
preserve model outputs at the original Q3_K numerical quality while
improving prompt processing and autoregressive token throughput?
Nothing short of the actual model-level comparison establishes this.

Public Q3_K_S control is pinned by full GGUF SHA-256
7add77b8d3736d6b2fd21dc96e69be026f19bde843e9988b43cef1315fc5eebe
(bartowski/SmolLM2-135M-Instruct-GGUF). Original upstream llama.cpp,
with genuine native optimized Q3_K, is pinned at
f7591a6a01f9273de03331732bdc0b39721168be.

## Code: no silent stock-Q3 fallback

The Rust adapter bpb3k_q4_full_runtime.rs parses GGUF v3, checks all
original tensor sizes and offsets, rewrites only Q3_K payloads to 146
bytes per 256 weights, relocates EVERY tensor's GGUF header offset with
the original alignment, copies all non-Q3 tensors byte-for-byte and
preserves all Q3 signed quantization coefficients, 16 subgroup scales
and global raw FP16 bits. Synthetic fixture tests exercise relocation
and ensure a following unmodified F32 tensor remains identical.

The 146-byte payload is NOT a standard original GGUF Q3_K block.
It is an ephemeral, local research surrogate type ID 11, consumed
only by a transient source-pinned patched ggml runtime. Patches:
- ggml-common.h: original Q3 C struct fields retained for compilation
  plus 36 experimental tail bytes, type_size=146;
- ggml.c: Q3 to_float routes to a 146B signed-code decompressor;
- ggml-cpu.c: Q3 vec_dot routes exclusively to Rust
  mem_q4_146_vec_dot, nrows=1;
- tiled.cpp: forbid the stock Q3_K tiled weight repacker on surrogate
  bytes. An unexpected Rust FFI ABI nrc, size or null pointer aborts;
  there is no switch that silently falls back to stock Q3_K;
- CMake: CPU backend explicitly links the research Rust staticlib.
No patched runtime, original GGUF, or model data is committed or
uploaded as a GitHub artifact.

The candidate Rust bpb3k_q4_ggml_ffi.rs calls the independently
qualified bpb3k_q4_pair228_avx2.rs 146B AVX2 kernel on actual Q8_K
activation blocks inside the real model's matrix multiplies.
Its signed integer reconstruction is exact; its scalar FP32 block
sum is not assumed to reproduce the native eight-lane FP32 FMA
accumulation order for arbitrary multi-superblocks.
Model logits and tokens must settle that question.

## Frozen all-model evidence, same native source/build target/CPU

1. Rust 1.89 self-tests of converter and Rust live FFI, including a
   synthetic two-tensor GGUF payload with real Q3 and untouched F32
   bytes; abort on mismatch. Pin source SHA and GGUF SHA.
2. Build and retain binary hashes of genuine original optimized
   llama-cli, llama-debug and llama-bench, plus separately patched
   live Rust Q4-146 CPU-only equivalents. No GPU offload or SDK.
   Verify the Rust FFI actually executes in full generation via
   BPB3K_Q4_146_REAL_DOT_EXECUTED and a deterministic arithmetic
   prompt canary against original Q3_K.
3. Before timing evaluate both real LLMs on two prompts, with
   llama-debug capturing every vocabulary logit for eight generated
   steps. Compare original Q3_K and Q4-146 with the *existing
   separately frozen SML stock tolerance* abs <= 2e-4 +
   rel <= 2e-5 * abs(reference), and demand identical token IDs.
   Reject any nonfinite, missing, malformed or mismatched dumps.
   If this fails, retain RED fidelity status even if Q4 runs fast.
4. Measure both actual whole-model llama-bench inference paths
   with identical original training weights, CPU-only, threads=2,
   prompt length p=64, generation n=32, repetitions r=2, 12
   same-host paired counterbalanced runs (AB,BA alternating).
   Retain unaggregated bench tables and peak RSS per process.
   No performance success threshold gates this research workflow.
   A final quality gate requires the frozen model logits and
   token criteria, after all the bench evidence has been retained.
5. GitHub-hosted CPU does NOT qualify real Dell Xeon, NUMA,
   power, host load or physical DRAM counters. No extrapolation
   to other models; no 1.0 release or inference default changes.
   Real physical research must use the RemoteOps control plane,
   a strict admission envelope and the exact tested model/source
   hashes, with no interruption of active services.

## Current evidence boundary

In the earlier one-block synthetic tests, Q4-146 achieved
13.16 ns/row versus stock 18.60 ns/row at 1024 rows, and
11.81 versus 17.27 ns/row over 262144 contiguous weight rows,
while adding 32.73% per-weight storage (see BPB3_K_Q4_PAIR228.md).
Those observations are NOT any model-generation token throughput
or fidelity result.

Until this entire full-model experiment actually succeeds,
the claim "Q4-146 is faster at model level" remains **UNQUALIFIED**.
If the full-model attempt fails or hardware is unavailable,
report the exact missing gate and make no release claim.
