# Q5-178 vs native Q5_K on the *same* pretrained FP16 source, exactly matched matrices

Status: **active experiment, not qualified until exact-head CI succeeds**. This closes an important gap in the first Q5-178 real-model pilot: F16 was an informative control but cannot establish superiority against optimized native five-bit ggml.

## Design

Use the same pinned `SmolLM2-135M-Instruct-f16.gguf` source, SHA-256 `f535f83ec568d040f88ddc04a199fa6da90923bbb41d4dcaed02caa924d6ef57`, and llama.cpp exact SHA `f7591a6a01f9273de03331732bdc0b39721168be`. The nonstandard Q5-178 converter selects exactly those F16 matrices whose first dimension is a full multiple of 256; in this model the previous hosted run observed **30** `ffn_down.weight` matrices containing **103,680 blocks**.

The native comparator is produced by the upstream, completely unmodified `llama-quantize` CPU executable in the **same run**. Instead of indiscriminately converting other tensors, the invocation uses the upstream positional `Q5_K_S` quantizer plus `--pure` and first-match regex overrides:

```bash
./stock/build/bin/llama-quantize --pure \
  --tensor-type 'ffn_down\.weight$=q5_k' \
  --tensor-type '.*=f16' \
  source-f16.gguf native-q5-k.gguf Q5_K_S 2
```

A new Rust `audit-native-q5k` command **must pass** before any inference:

- Parse the original F16, native standard Q5_K, and sentinel Q5-178 GGUF v3 directories independently.
- Demand matching tensor names and all dimensions, original block count, file alignment, and no malformed duplicate entries.
- For each source tensor selected for Q5-178, demand *both* the native Q5_K type ID **13**, exact native Q5_K **176 bytes/256**, and the Q5-178 sentinel type `0xffff0005`, **178 bytes/256**.
- For every nonselected source tensor, demand original type and **byte-for-byte unchanged serialized payload** in both comparators. The audit fails if native quantization unexpectedly touches any different tensor.
- Demand exactly **30 selected matrices and 103,680 blocks** in the pinned source. Do not claim a matched benchmark if any mismatch occurs.

The baseline Q5_K and experimental Q5-178 have different quantization algorithms and scale layouts. Equal source and equal selected tensors **do not** mean their coefficients are identical; this experiment is about actual model quality and speed, not format equivalence.

## Frozen measurement protocol

- Native untouched stock llama.cpp CPU vs temporary source-pinned experimental Q5-178 patch, 2 threads, GPU layers 0.
- Exactly two eight-step, full-vocabulary model traces on short arithmetic and long prefill prompts. The independent reporter compares numeric logits only when all preceding greedy tokens are identical, retaining divergence and nonfinite failures. The Q5 Rust FFI *and* true fifth-bit tiled matrix execution markers must be present.
- Eight alternating AB/BA full-model paired sessions (four of each order), `llama-bench -ngl 0 -t 2 -p 64 -n 32 -r 2`, peak RSS from process telemetry for every arm. Preserve raw pairs and compute median **within-pair** ratios; no artificially positive speed threshold or post-hoc sample selection.
- Hash source, native output, Q5 sentinel, Q5 temporary model, and compiled binary. Retain conversion user/wall time and memory for both pipelines.
- No raw pretrained weights, modified model or full-vocabulary logits uploaded. Artifact holds only metrics and operational logs.

## Strict scope and next decisions

Passing unit tests or compiling llama.cpp is not a performance qualification. A true model result must include successful three-way tensor audit, live inference, all per-step observations, paired throughput and memory. Retain slower/less accurate outcomes as negative results.

Even a positive hosted result cannot be generalized to physical Xeon, ARM or another model. Dell/Thor/Mac physical experiments are permitted only through **Memorithm/RemoteOps** resource admission; do not alter production services, launch permanent models, or add NVIDIA SDKs.

The previous hosted F16 comparison at [run 43126478915](https://github.com/Memorithm/SML-GENIUS/actions/runs/43126478915) found preliminary lower Q5-178 prefill and higher decode throughput versus **F16**, but no native Q5_K control. This experiment closes that specific scientific comparison gap without changing the production implementation.
