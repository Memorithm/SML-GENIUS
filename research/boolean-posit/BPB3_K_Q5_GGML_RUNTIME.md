# Q5-178 temporary ggml runtime adapter (research, 2026-11-12)

Status: **implemented code; exact-head compilation and real-model inference qualification separate**.

Three independent Rust components:

- `bpb3k_q5_gguf_transcode.rs`: source-precision F16/BF16/F32 GGUF v3 -> **deliberately invalid** type `0xffff0005` Q5-178 blocks. Added command `prepare-patched-runtime`, which strictly validates payload extents and refuses any native Q3_K, then atomically makes a distinct *temporary* GGUF copy mapping only the sentinel type to 11 for a matching patched engine. The safe sentinel source is preserved. **Do not load the prepared copy in stock ggml**.
- `bpb3k_q5_ggml_ffi.rs`: live Rust AVX2 `mem_q5_178_vec_dot`, one ggml signed Q8_K activation and one 178-byte Q5 block per 256 coefficients; 8 integer lanes and eight separate FP32 FMA chains across all superblocks, ggml AVX2 horizontal reduction order; abort on invalid ABI, unsupported CPU, mis-sized n, or nrc!=1, never falling back to stock.
- `bpb3k_q5_ggml_patch.rs`: rejects all source SHA drift; patches **only a throwaway checkout** of llama.cpp at `f7591a6a01f9273de03331732bdc0b39721168be`. The Q3_K surrogate is widened from 110 to 178 bytes and wired to the Q5 decoder, Rust Q5 AVX2 vector dot and distinct direct 5-bit tiled repacker, with tiled `BIAS=16`. The original Q3_K source unpacker is not called on Q5 data, and the original Q4-146 path is not modified. Staticlib must be explicitly supplied at CMake configure time.

This source-pinned adapter does **not** make Q5-178 a standard GGUF quantization type. It requires the two-stage converter/preparer and a separately patched CPU backend. A source model mixing native Q3_K is rejected by the preparer. The runtime is not installed or resident anywhere; hosted CI compilation is isolated and has no Dell/Thor/Mac side effects.

### Acceptance gates

1. Exact-head Rust 1.89 `-D warnings` tests of source converter, preparation, FFI and patch generator.
2. Rebuild of pinned original llama.cpp, replacing only Q3 surrogate internals and linking the real Rust AVX2 staticlib, including `llama-cli` and `llama-bench` binaries with hashes. **This is compilation evidence, not a live model result.**
3. **Still missing:** independent full trained FP16/BF16 model, real entire Q5-178 inference, unchanged FP16 control and standard Q5_K/Q4-146 reference, full-vocabulary logit and token output, reproducible paired throughput, conversion time/RSS and physical Dell RemoteOps qualification. The checksum and model itself must be pinned, and a candidate that does not execute the Q5 tiled path must fail admission.
4. No default production enablement or speed claim until all criteria pass. Candidate model quality is not expected to be bit-exact with FP16: report task metrics, deviation distributions and token differences without relaxing a frozen acceptance policy after the fact.

Reproducible Rust commands (source checkout only):

```bash
rustc --edition 2021 -D warnings --test research/boolean-posit/bpb3k_q5_ggml_ffi.rs -o /tmp/q5-ffi-tests
/tmp/q5-ffi-tests
rustc --edition 2021 -D warnings --test research/boolean-posit/bpb3k_q5_ggml_patch.rs -o /tmp/q5-patch-tests
/tmp/q5-patch-tests
```
