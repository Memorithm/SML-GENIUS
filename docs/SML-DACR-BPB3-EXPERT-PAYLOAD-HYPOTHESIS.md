# DACR × Boolean Posit Block — optional expert-payload research bridge

**Status:** research hypothesis only; no SML runtime, default weight format, or DACR-1 source-index semantic change. **Date:** 2026-11-09.

## Separation of duties

DACR-E exact-source lookup addresses CONTEXT-0/1c raw data by byte-exact key, validated source ID and SHA-256. It is NOT a numeric dot-product workload. **Never** substitute BPB3 packed weights, Posit-like scale encoding, POPCOUNT similarity, approximate keys or floating-point math for exact-key/provenance verification.

DACR-L latent semantic lookup may eventually contain trained low-bit scoring/projection coefficients, but must first show held-out semantic recall and measured memory traffic at equal total/index budget. Direct access to a trained code is not equivalent to searching all historical tokens.

DACR-P (native SBG/MOR expert/page selection) is the **primary integration candidate**: store only qualified *selected* parameter pages in BPB3-family bitplane payloads, execute directly with bitwise/SIMD kernels where the coefficient/activation domain matches, count actual page transfers, and leave native ANF/Boolean circuits in their existing more compact representation where appropriate.

## Actual format and evidence boundaries

- BPB-3/v0: 26 serialized bytes per 64 weights = 3.25 bit/weight, bipolar activation dot; **not Posit Standard 2022**.
- BPB3-K: 110 bytes per 256 values, equal to official GGUF Q3_K's 3.4375 bits/weight, lossless *quantized reconstruction* transcode; **no parameter-byte saving over Q3_K**.
- BPB3-M: two 56-byte 128-weight microblocks (112 versus 110 bytes per 256, +1.818%); possible selective-half access but fair Q3_K half reference also skips unneeded bitplanes, so only real physical partial-work measurements can decide.
- Earlier BPB-3 Dell synthetic experiments reported 43.91–44.67% less iMC DRAM read-CAS traffic than selected INT8 kernels, **not** a proportional end-to-end generation gain. Matched scan-time INT8/BPB ratios were 1.217–1.286 on accepted cells, not model throughput.
- On *trained Pythia-14M weights*, BPB-3 with two residuals had 36.6–43.1% *worse* reconstruction RMSE than Q3_K at equal 110-byte/256 budget. BPB3-K fixes reconstruction parity by preserving Q3_K coefficients/scales; it does not beat Q3_K.
- BPB3-K fused AVX2 v2 `fused_v2_stock` achieved bounded full-model SmolLM2 numeric parity in the cited 2026-11-09 dossier; two physical Dell campaigns were category **E / contaminated**. Throughput superiority over optimized stock Q3_K is **not** established; the published contaminated sessions show a slower BPB3-K candidate.

The SML-0 target 25B physical mix is 21.25B × 1 bit, 2.5B × 2 bits, and 1.25B × 4 bits = 31.25 billion packed bits, **1.25 average bits/parameter** before metadata. Naively converting the entire proposed SML atlas to 3.25-bit BPB-3 would *increase*, not decrease, this payload. Use BPB only where higher-range signed weights, vector arithmetic or zero-dequant execution justify the real measured cost. Do not count deterministic connectivity as physical learned weights.

## Candidate experiment after DACR-1 / DACR-2, NOT a DACR-1 gate

Keep the same learned circuit, page IDs, SBG/MOR selected-page set and task data across matched arms:
1. native SML Boolean/ANF bits (baseline);
2. equal-quality quantized signed low-bit dense baseline;
3. BPB-3 or BPB3-K as appropriate to the **same** coefficient domain, plus a selective BPB3-M arm for actual half-page requests;
4. equivalent fixed routing and dense/all-page controls only as scientific comparators.

For each arm record test/held-out loss, task accuracy, materialization/conversion cost, true physical bytes read (including metadata/cacheline amplification), page faults, RAM/VRAM residency, latency p50/p95/p99, throughput, CPU instructions, active page count, training/storage cost and failure/abstention behavior. The source-exact lane's correctness must be unchanged. No unmeasured speedup or model-accuracy claims.

**Reuse ownership:** SML owns page/weight semantics, BPB research and quality promotion. SciRust may own reusable bitplane kernels/oracles; FLAT may qualify WGPU kernels only for independent compatible numerical domains; ElasticXxx may plan residency of **qualified** payload formats; NNIS may be an optional portable CPU/WGPU carrier; RemoteOps owns Dell/Thor/Mac physical test admission. No NVIDIA SDK required.

Sources: `research/boolean-posit/README.md`, `PAPER_BPB3_WORKING_v0.1.md`, `BPB3_K_EQUIVALENCE.md`, `BPB3_M_128.md`, `BPB3_K_FUSED_AVX2_V2.md`; PR #910 physical qualification dossier (E); `docs/SML-0-SPEC.md`; `docs/SML-DACR-0-DIRECT-ASSOCIATIVE-CIRCUIT-ROUTING.md`.
