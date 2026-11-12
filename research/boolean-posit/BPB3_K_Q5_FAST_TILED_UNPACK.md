# Q5-178 tiled: contiguous nibble SIMD + eight-byte fifth-bit swizzle

Status: **experimental optimization; no speed claim until the exact-head full-model canary finishes**.

## Measured problem

The first rigorously *source- and tensor-matched* native comparison at [run 43129338902](https://github.com/Memorithm/SML-GENIUS/actions/runs/43129338902) (artifact 13904442893) shows that Q5-178 is slower than standard `Q5_K` on both prefill and decode at 2 CPU threads. Both formats quantized the same 30 `ffn_down.weight` matrices (103,680 x 256-value blocks) of the pinned `SmolLM2-135M-Instruct-f16.gguf`. Nonselected payloads matched bit-for-bit. Median of eight within-pair Q5-178/native ratios:

| Operation | Q5-178 / Q5_K native | Relative result |
| --- | ---: | ---: |
| pp64 prompt | 0.871174 | 12.88% slower |
| tg32 decode | 0.963827 | 3.62% slower |
| Peak RSS KiB | 1.002948 | 0.295% higher |

Each of the eight pairs favored Q5_K on prefill and decode. This negative baseline must be retained. The native Q5_K process used approximately 176 bytes per quant block, versus the experimental Q5-178's 178 bytes. The previously positive result against native *FP16* does not establish a Q5_K advantage. Conversion costs are not directly matched: the exhaustive Rust Q5-178 converter ran 32.93 s on this host and upstream Q5_K two-thread conversion 1.27 s, with different algorithms and CPU utilization.

Fidelity remains a concern: on the arithmetic prompt the token streams diverged at generation step 5 (0-based), and the context-aware reporter correctly marked subsequent numeric logits as incomparable. On the long prefill prompt the eight greedy tokens matched. Do not claim equivalent logits.

## Actual code change

The existing pinned llama.cpp `ggml/src/ggml-cpu/tiled/tiled.cpp` Q5-178 surrogate unpack loop previously extracted the 5th bit using an independent shift and load **for each of 256 weights**, in addition to nibble unpacking. This PR changes the generated C++ tiled kernel on the *same SHA-pinned throwaway checkout*:

1. Uses SSE2 128-bit loads and masked shifts to expand each group's 16 packed nibble pairs directly into 32 contiguous unsigned codes (16 low then 16 high); falls back to the original scalar loop on non-SSE2 platforms.
2. Expands one packed fifth-bit byte into **eight separate byte positions** with a three-stage 64-bit bit spread using masks `0x0000000F0000000F`, `0x0003000300030003`, `0x0101010101010101` and shifts `28,14,7`. ORs four 64-bit words per 32-weight group into the nibble codes using `std::memcpy`, avoiding alignment and aliasing undefined behavior.
3. Preserves 256 Q5 byte values, sixteen signed subgroup scales, original FP16 global scale, `BIAS=16`, one Q8_K source and existing compute graph. Changes no model weights, file format, global GGUF loader, or production service.
4. Rust regression tests exhaust all 256 possible packed fifth-bit byte values, and verify all 256 decoded codes for 1024 deterministic packed 178-byte blocks against the original scalar bitplane oracle.

This optimization primarily targets *tiled prefill*, not the one-row Rust AVX2 decode path. It cannot be credited with a decoding speedup without whole-model measurement; the unchanged FFI remains subject to a separate optimization pass.

## Qualification

- Rust 1.89 formatting and test suite must pass on the PR's exact head.
- The dedicated pinned ggml workflow must build real `llama-cli` and `llama-bench` on hosted CPU with the modified source-pinned kernel.
- The same F16 source whole-model canary must execute the Q5 Rust dot and direct tiled path and satisfy its frozen logit/token admission.
- The independent native Q5_K experiment must show *same-matrix* three-way audit and eight paired CPU tests on the new exact head. Do not infer a speedup by comparing unrelated hosted CPU VMs. A real closed-loop old-vs-new paired candidate experiment would be needed for a strong claim of acceleration.
- No Dell, Thor or Mac run until admitted by **Memorithm/RemoteOps** and isolated from other workloads. No GPU, NVIDIA dependency or production enablement.

This is an optimization candidate, not proof of superiority. If the observed prefill ratio remains unfavorable, retain the result and work on the compute/repack bottleneck rather than simply adding more tests.
