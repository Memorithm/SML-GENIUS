# BPB-3 adaptive bitplanes — preregistered correctness and trained-weight gate (2026-11-12)

**Research-only candidate, not Posit Standard 2022, not model-integrated, no superiority claim.** The prior BPB-3/v0 has 26 bytes/64 coefficients with 3 fixed sign/magnitude planes, 2-byte regime-like shared exponent, and maximum signed code 3. Earlier *trained* Pythia-14M E1 comparisons showed BPB-3 plus two residuals remained worse than standard GGML Q3_K at identical 110-byte-per-256 budgets. We retain that negative finding.

## Actual new BPB-3-A1 format and numeric contract

Each 64-coefficient source block is encoded as:

`u8 mode (1, 2 or 3 magnitude planes) || u16 scale (canonical BPB-3 regime k, LE) || u64 negative sign plane (LE) || mode × u64 magnitude planes (LE)`.

- Serialized **19 / 27 / 35 bytes per block**, or 2.375 / 3.375 / 4.375 effective bits per coefficient including the mode byte and shared exponent. All-zero negative signs are canonical; negative zero is rejected. Each magnitude bit plane contributes `2^j` to a nonnegative magnitude. The sign plane negates the resulting magnitude. Signed domains are `[-1,1]`, `[-3,3]`, `[-7,7]`.
- `k ∈ [-24,27]`, inherited from BPB-3/v0, scale exactly `2^k`. This is *not* a scalar posit and has no Posit Standard arithmetic behavior. At mode 2, bytes after the leading mode are exactly the v0 BPB-3 26-byte wire for identical quantized codes and exponent.
- Every admitted finite 64-element source block is independently quantized for all three modes; each mode enumerates all 52 shared integer exponents, uses deterministic nearest-integer/ties-away-from-zero and clamps to its signed domain. Objective: unweighted source **SSE** in f64; ties resolve to earliest exponent.
- Exact multiple-choice dynamic programming then chooses one mode per block under a **frozen total wire budget**. Each block costs 19 + 8×(mode−1) bytes. The frozen default evaluation budget is **27 bytes per 64 coefficients = 108 bytes per 256 coefficients**, including all codec metadata; this is *below* reference Q3_K's 110 bytes per 256. The same tagged fixed mode-2 reference occupies exactly 27 bytes/block; native original BPB-3/v0 occupies 26 bytes/block and is separately acknowledged, never treated as equal cost to the tagged candidate. The DP computes globally minimum source SSE for the declared codebooks and budget, not the global optimum of every possible quantizer.
- The serialized direct bipolar dot uses signed mask equality and POPCOUNT for each magnitude plane, exactly matching the independently evaluated dense-code signed integer dot. For multi-block accumulation, a checked **i128** sum uses a fixed `2^-24` scale; this is arithmetically exact on admitted blocks, not a promise of ggml FP32/FMA equality.
- Payload bytes exclude container headers, alignment, model/tokenizer, activations and memory allocator overhead, which are not defined here. Do not infer RAM, DRAM or CPU latency from the serialized size.

## Source, prospective splits and fixed comparators

The actual trained-weight trial is run on **a newly chosen set of matrices** from the open Apache-2.0 EleutherAI Pythia-14M model: layer 2 `attention.dense` and `attention.query_key_value`, layer 3 `mlp.dense_h_to_4h` and `mlp.dense_4h_to_h`. None of those four tensor names was used in prior E1's layer-0/layer-5 strata. Pin `EleutherAI/pythia-14m` revision `94f7c35d5e9f2e9bac8ca839329f505b4d007d5d`, file `model.safetensors` expected SHA-256 `116a02532db461f91386a5b20f942ff2c8d4de7341e21b55caafc3d7b25f49a1`. Extract disjoint **24 DEV and 24 TEST** original source blocks of 256 f32-upcast coefficients *per tensor*, deterministic `bpb3-adaptive-v1-nov12-independent-layer2-3` shuffle; source indices and derived f32le hashes retained. This is independent *sample selection*, not an independent model or independent architecture.

All hyperparameters, mode domains, budget, corpus choice, selection order, tie handling and report metrics are fixed here before new TEST results. Candidate selection can inspect the weights it is quantizing (as must any post-training quantizer), but must **never** use TEST outcome metrics to change codec parameters. DEV and TEST are evaluated and reported separately; no reranking or omission of unfavorable TEST matrices.

References on identical original FP32-upcast source rows:

1. Fixed tagged mode-2 BPB-3-A1, 108 B per 256 (27 B per 64); invariant: adaptive DP SSE <= fixed tagged SSE at equal or lower wire bytes. This is a mathematical property of exact DP and nested candidate choices, **not an empirical superiority result**.
2. Original fixed BPB-3/v0, 104 B per 256 (un-tagged), using the identical mode-2 numeric codebook. Report metadata overhead honestly; never claim 108 == 104.
3. Official ggml `Q3_K` (110 B per 256) and `Q4_K` (144 B per 256), pinned upstream `f7591a6a01f9273de03331732bdc0b39721168be`, compiled CPU-only. Reuse the independent `reconstruction_e1.rs` official C encoder and Rust/C parity diagnostics, with the **same new original source files**, no alternative checkpoints.

Measure, per corpus and split: exact codec wire bytes, realized bit/weight, per-mode frequencies, RMSE of decoded weight reconstruction and bipolar dot RMSE. Record Q3_K and Q4_K original-weight RMSE and dot RMSE on the identical splits; their native dot protocol is separately implemented and does not have to use the candidate's mask seeds. Do not rank unmatched dot estimates across distinct mask protocols without recomputing with identical masks. Report negative results.

## Admission, outcomes and limits

**A1 correctness gate**: Rust 1.89 -D warnings unit tests for all 3 modes, canonical decoder and malformed inputs, 64-lane signed dense oracle on deterministic bit patterns, fixed BPB-3 mode-2 byte identity, exact DP vs complete enumeration on a small corpus, precise budget accounting, heterogeneous example improvement, and exact inter-block dyadic integer arithmetic. Unit tests are correctness evidence only.

**A2 trained-weight gate**: Hosted-only CPU workflow must validate upstream model hash, matrix provenance and distinct DEV/TEST indices, pass all Rust tests and formatting, run all four tensors with frozen budget and official ggml Q3_K/Q4_K matched baselines, preserve CSV and manifest without uploading weights or model. Fail closed on missing cells, nonfinite values, inaccurate bytes or incomplete data. Real-model quality remains unqualified; reconstruction quality can be favorable or unfavorable and must not be treated as a CI failure.

**A3 (not covered by A1/A2)**: independent second model, held-out perplexity/task quality, complete native SML runtime integration, physical inference latency/RSS/DRAM counters on isolated hardware via Memorithm/RemoteOps, and uncertainty intervals. Even if A1 and A2 pass, no general superiority, original algorithm novelty or production activation is established.

## Context and prior art

- BPB-3/v0 source and existing negative Pythia E1: `research/boolean-posit/README.md`, `RECONSTRUCTION_E1.md`, and [issue #853](https://github.com/Memorithm/SML-GENIUS/issues/853).
- Posit Working Group, *Posit Standard*, 2022: https://posithub.org/docs/posit_standard-2.pdf
- Open Compute Project, *OCP Microscaling Formats (MX) 1.0*, September 2023: https://www.opencompute.org/documents/ocp-microscaling-formats-mx-v1-0-spec-final-pdf
- Li Ding et al., "AMPA: Adaptive Mixed Precision Allocation for Low-Bit Integer Training," ICML 2024: https://proceedings.mlr.press/v235/ding24b.html
- Zhiyong Huang et al., "Hessian-based mixed-precision quantization with transition aware training for neural networks," *Neural Networks* 182 (Feb 2025), 106910: https://pubmed.ncbi.nlm.nih.gov/39579751/