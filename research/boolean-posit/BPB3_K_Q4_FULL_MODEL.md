# BPB3-K Q4-146: actual entire trained SmolLM2 inference versus native Q3_K

Status: hosted-CPU full-model experiment IMPLEMENTED AND MEASURED on
2026-11-10; model-level superiority was NOT observed. This remains
research-only, NOT a production promotion or a physical Dell result.
SML-GENIUS owns the scientific conclusion; Memorithm/RemoteOps alone
admits Dell/Thor physical host access.

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
Its signed integer reconstruction preserves all eight AVX2 integer
accumulators; the live FFI now applies independent per-lane FP32 FMA
over all superblocks and ggml's original horizontal reduction order.
A scalar per-block reduction previously changed the real model output
(from "4." to "two"); it was corrected without relaxing the frozen
logit/token acceptance criteria. Actual logits and tokens, not algebra
alone, remain the definitive numerical qualification.

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

## Hosted-CPU full-model result — 2026-11-10

Successful reproducible full-model workflow:
[run 42895070407](https://github.com/Memorithm/SML-GENIUS/actions/runs/42895070407),
[raw evidence artifact 13799179745](https://github.com/Memorithm/SML-GENIUS/actions/runs/42895070407/artifacts/13799179745).
This is the **pre-main-integration code head** (commit
`e55cce91d90b1be15d61ad96a72c90f9948dcdfe`); the same code
was later merged with main without tree changes to experimental files.
For final PR acceptance, require the new exact-head CI independently.

- Actual pre-trained SmolLM2-135M-Instruct, source GGUF SHA-256
  `7add77b8d3736d6b2fd21dc96e69be026f19bde843e9988b43cef1315fc5eebe`,
  against pinned original upstream ggml source. The converter rewrote
  **30 Q3_K tensors, 103680 blocks**, leaving all other tensor data intact.
  Original 82.41 MiB, experimental model 85.97 MiB.
- Actual Rust Q4-146 FFI executed in generation, and the original and
  surrogate output the same arithmetic-answer canary.
- Both exact-token sequences match across two distinct fixed prompts,
  eight output-token steps per prompt. **786432 vocabulary logits total
  compared** (393216 per prompt); max absolute discrepancy **0.0**,
  RMSE **0.0**, zero tolerance violations. This is limited to the
  measured traces; not a universal numerical-equivalence proof.
- Twelve paired AB/BA whole-model CPU runs, `-ngl 0 -t 2 -p 64
  -n 32 -r 2`, executed on the same GitHub-hosted CPU runner.
  Medians below are computed over the twelve run-level throughputs.
  The relative effect is the median of twelve within-pair Q4/stock
  throughput ratios, NOT a ratio of the displayed medians.

| Metric | Native Q3_K stock median | Rust Q4-146 median | Paired Q4/stock median |
| --- | ---: | ---: | ---: |
| Prompt processing pp64 (tokens/s) | 756.025 | 546.540 | 0.7228 (**-27.72%**) |
| Generation tg32 (tokens/s) | 240.495 | 225.470 | 0.9417 (**-5.83%**) |
| Maximum RSS (KiB) | 170018 | 173558 | 1.0206 (**+2.06%**) |

All twelve prompt pairs and all twelve generation pairs favored native
Q3_K on this hosted CPU in this configuration. Model-level speed
superiority is therefore **NOT demonstrated** here; the measured
candidate is slower despite its isolated-kernel microbenchmark wins.
The increased weight footprint, reuse and calling overhead are
plausible investigation targets but are NOT measured causal explanations.

No change to production inference defaults is authorized by this
research result. No Dell Xeon or NUMA/PMU conclusions are implied.
A separate strictly admitted physical paired campaign through
Memorithm/RemoteOps is still required to evaluate that hardware.

