# BPB-3/v0 — Boolean Posit Block

Status: **experimental research**. Not Posit Standard (2022) compliant; not integrated into SML-GENIUS runtime. Do not claim numeric or model superiority without physical and held-out evidence.

## Format contract

Each block represents exactly 64 quantized weights `w_i = q_i * 2^k`, with `q_i ∈ [-3,3]` and `k ∈ [-24,27]`. The on-disk encoding has 26 bytes: a little-endian 16-bit shared scale, followed by three little-endian 64-bit planes (`negative`, `lo`, `hi`). Negative zero is invalid; zero uses an unset sign bit.

For the shared scale, `k = 4r+e` with Euclidean division, `0<=e<4`. Positive/zero `r` encodes `r+1` ones then a zero; negative `r` encodes `-r` zeros then a one. Follow with two exponent bits, high bit first, in a left-justified u16 with mandatory zero padding. This is a block-level regime-inspired exponent, **not** a scalar posit.

For signed bipolar inputs `x_i in {-1,+1}` (input bit 1 denotes -1), let `E = NOT(negative XOR x_negative)` and `C(M)=2*popcount(E AND M)-popcount(M)`. The **exact integer** dot product of quantized weights is `D=C(lo)+2*C(hi)`. The represented numeric result is the pair `(D,k)` meaning `D*2^k`. The output is **not** exact relative to original unquantized weights. Inter-block accumulation needs scale alignment and bounded overflow handling; not defined in this v0.

## Capacity and limitations

Serialized weight payload: 26/64=3.25 bits per weight, compared to 8 for INT8 and 32 for FP32. Payload-only ratios are 64/26=2.4615 and 256/26=9.8462. Cacheline fetches, RAM residency, alignment, activations, indices, metadata, quantization error and any conversion must be measured; no conclusion on actual memory traffic follows.

The implementation includes exactness tests versus a dense oracle, serialization checks and an actual labeled-data learner that learns a bounded synthetic linear classification task. That is **not** demonstration of a trained SML model. `--bench` is a warm microbenchmark, not a DRAM-bandwidth or end-to-end test.

## Reproduce

From the repository root (Rust 1.89, no dependencies):

```bash
rustfmt --edition 2021 --check research/boolean-posit/prototype.rs
rustc --edition=2021 --test research/boolean-posit/prototype.rs -o /tmp/bpb-tests
/tmp/bpb-tests
rustc --edition=2021 -O research/boolean-posit/prototype.rs -o /tmp/bpb-demo
/tmp/bpb-demo
/tmp/bpb-demo --bench
```

## Next scientific gates

BP-1: exact bitwise parity, canonical decode and supervised learning on disjoint holdout.
BP-2: quantize real weights and compare held-out quality with INT8, BNN, MXFP4/MXINT8, and equivalent 3-bit block schemes at matched budgets.
BP-3: freeze paired physical CPU experiments: RSS, cold/warm latency, parameter/page traffic, cache events, training cost and uncertainty.
BP-4: adaptive bitplane count, sparsity and block-size studies including metadata.
BP-5: separately preregistered SML model-level investigation, with no promotion without evidence.

**Prior art caution:** Shared block scale is established in Open Compute Project MX (2023); XNOR/popcount is established in BNN. Novelty of this particular combination, including unary-scale encoding, is **not established**.

SML-GENIUS retains scientific and model authority; SciRust/FLAT-ATTENTION/NNIS are optional validation partners, and RemoteOps is the exclusive self-hosted execution control plane. No NVIDIA SDK.

## Sources

- Posit Working Group (J. Gustafson, chair), *Posit Standard*, 2022: https://posithub.org/docs/posit_standard-2.pdf
- OCP Microscaling Formats (MX), version 1.0, September 2023: https://www.opencompute.org/documents/ocp-microscaling-formats-mx-v1-0-spec-final-pdf
- *Optimizing Data Flow in Binary Neural Networks*, 2024: https://pmc.ncbi.nlm.nih.gov/articles/PMC11314678/
- IEEE P3109 (ongoing arithmetic formats project): https://standards.ieee.org/ieee/3109/11165/
- Repository: `AGENTS.md`, off-main `agent/ecosystem-roadmap`, `docs/ROADMAP.md`, PVP hardware-alignment bootstrap.
