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


## Gigascale resident-corpus programme (2026-11-06)

**Target host:** Dell T430 (user-reported 125 GB installed RAM). Installed physical RAM is **not** a claim of currently available budget. No unadmitted host experiment is authorized. The initial gate uses GitHub-hosted runners; future Dell execution must be admitted via Memorithm/RemoteOps, with resource/active-job inspection and no disruption.

`large_corpus.rs` generates deterministic **synthetic** BPB blocks and matching INT8 coefficients in actual resident vectors, plus one bipolar 64-bit input per block. Every pass scans the *full* resident corpus through both the bitplane and scalar INT8 kernels, alternates kernel order across passes, and requires exact equality of aggregate integer outputs. The standalone generator uses fixed seed, records sizes and runtimes, does not call a GPU, and never claims real data or measured DRAM bytes.

Physical-resident allocation estimate is computed using Rust `size_of::<Bpb3>() + 64 + 8` per block, **not** the serialized 26-byte payload. For an alignment-32-byte `Bpb3` Rust structure, the expected footprint is approximately 104 bytes/block, excluding Vec headers, allocator overhead, process/tooling and OS caches. The program prints the actual structure size indirectly via the estimate; its output is evidence, not this assumption.

| Blocks (64 coefficients each) | Coefficients | Serialized BPB weights | Estimated resident BPB + INT8 + inputs (if Bpb3 = 32 B) |
|---:|---:|---:|---:|
| 1,048,576 | 67,108,864 | 26 MiB | 104 MiB |
| 8,388,608 | 536,870,912 | 208 MiB | 832 MiB |
| 32,000,000 | 2,048,000,000 | 832 MB | 3.328 GB |
| 128,000,000 | 8,192,000,000 | 3.328 GB | 13.312 GB |
| 256,000,000 | 16,384,000,000 | 6.656 GB | 26.624 GB |
| 512,000,000 | 32,768,000,000 | 13.312 GB | 53.248 GB |

**Allocation safety:** before any corpus allocation, refuse requests exceeding the smaller of (a) 50% of observed available RAM and (b) available RAM minus a default 16 GiB reserve. Read Linux `MemAvailable`, and if available the effective process cgroup-v2 `memory.max - memory.current`; a missing cgroup limit is *unknown*, not proof of unlimited resources. `--max-ram-percent` is bounded to at most 70% and `--reserve-mib` to at least 256 MiB. An additional RemoteOps admission check is required before invoking the large Dell workload. The allocator uses fallible reserve, but OS OOM from concurrent workloads remains possible, so no host execution should proceed without external isolation/budget enforcement.

Suggested *commands for an already approved RemoteOps task*, **not** instructions for manually running unadmitted work on a busy host:

```bash
rustc --edition=2021 -O research/boolean-posit/large_corpus.rs -o /tmp/bpb-corpus
/tmp/bpb-corpus --blocks 8388608 --repeats 2 --reserve-mib 512
# Dell admitted, isolated 32M-block phase:
/tmp/bpb-corpus --blocks 32000000 --repeats 2
# Dell admitted, isolated 256M-block phase; only if resource guard accepts:
/tmp/bpb-corpus --blocks 256000000 --repeats 2
```

The initial 2026-11-06 physical campaign has now been completed on the Dell via RemoteOps: smoke [42298328750](https://github.com/Memorithm/RemoteOps/actions/runs/42298328750), 32m [42298780704](https://github.com/Memorithm/RemoteOps/actions/runs/42298780704), 128m [42299259414](https://github.com/Memorithm/RemoteOps/actions/runs/42299259414), and 256m [42299615528](https://github.com/Memorithm/RemoteOps/actions/runs/42299615528). All four accepted their resource gates and produced matching **aggregate sums** across two paired bitplane/INT8 passes. These aggregate checks alone do **not** certify individual block correctness; cancellation of distinct block errors can conceal faults. No physical DRAM traffic, real-data quality or model-runtime inference claim follows.

### Independent per-block integer oracle (next gate)

The optional standalone flag `--verify-each-block` scans **every** resident block after the paired benchmark, computing its BPB-3 bitplane result and independently evaluating the corresponding dense INT8 vector with the scalar reference. It aborts immediately with `FAIL_BLOCK_EXACTNESS index=...` when a block differs and prints `PASS_BLOCK_ORACLE blocks=... mismatches=0` only after the complete comparison. A deterministic ordered digest is a reproducibility aid, **not** a cryptographic fingerprint. Unit tests inject two opposite per-block changes whose total sum cancels, showing why prior aggregate-only equality is insufficient. The hosted 8,388,608-block/536,870,912-coefficient gate always enables the per-block comparison; **the Dell 256m run above did not run this new flag**. A separate exact-source-pinned RemoteOps admission/run is required to claim per-block validation at 16.384 billion coefficients.

The oracle checks quantized signed integers, not the accuracy of original floating-point weights or learned model parameters. The same corpus generator provides the paired INT8 and BPB representations; the *dot-product algorithms* are independent, not the provenance of original weights.

To reproduce on a bounded research computer, add `--verify-each-block` to the existing `large_corpus.rs` invocation after its host-specific memory admission.

Large synthetic checks alone cannot establish neural-network capability, practical numeric accuracy, or storage-traffic elimination. Matched real-corpus quality, multi-seed controls, hardware counters and end-to-end model trials remain separate preregistered gates.

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
