# BPB3-X/v0: Elastic coefficient count per block (64/128/256)

**Scope:** research-only, reversible representation prototype. This is a
provisional interpretation of *coefficient par bloc Elastic* as a **variable
number of coefficients in a stored/executed block**, not elastic 3-bit
quantization coefficients or independently adjusted FP16 scales.

## Existing scientific baseline

- BPB3-K: a 110-byte exact bitplane rearrangement of a 256-coefficient
  ggml Q3_K superblock: 96B three-plane weights, 12B sixteen signed 6-bit
  subgroup scales, and 2B original global FP16.
- BPB3-M: proposed two 128-weight halves, each 56B (48+6+2), with FP16
  repeated. It is an experiment and not a general GGUF 128-value format.
- BPB-3/v0: a separate -3..+3 and single power-of-two-scale quantization
  scheme; not interchangeable with Q3_K-equivalent [-4..+3] layouts.

## BPB3-X v0 partition rules

Partition **one and only one** 256-weight BPB3-K source superblock into
contiguous segments of 64, 128, or 256 coefficients. Six possible patterns:

| v0 marker | Block lengths (coefficients) | Segment count | Payload B | With 1-byte directory | Total bits/weight |
| --- | --- | ---: | ---: | ---: | ---: |
| 0 | 256 | 1 | 110 | 111 | 3.46875 |
| 1 | 128 + 128 | 2 | 112 | 113 | 3.53125 |
| 2 | 128 + 64 + 64 | 3 | 114 | 115 | 3.59375 |
| 3 | 64 + 128 + 64 | 3 | 114 | 115 | 3.59375 |
| 4 | 64 + 64 + 128 | 3 | 114 | 115 | 3.59375 |
| 5 | 64 + 64 + 64 + 64 | 4 | 116 | 117 | 3.65625 |

**Formula:** per-segment bytes = 27 × number-of-64-groups + 2.
The fixed one-byte directory records one of six eligible patterns:
per 256 source weights, 1 + 108 + 2 × number-of-segments.

Each 64-group carries 24B of unchanged K bitplanes and three densely
packed bytes for its four 6-bit signed subscales (each 16 values).
Every segment duplicates the same source global FP16 bits; all segments
must match that value. The format has **no cross-source-superblock
fusion**. It reconstructs the exact original 110-byte BPB3-K wire and
thereby retains the original Q3_K numeric reconstruction, not improved
quantization accuracy.

The one-byte directory is explicit; an alternative 3-bit six-way code
could be researched but is **not** counted as free. A whole tensor needs
a directory/index and offsets to allow random source-block lookup.
Memory padding and cache alignment are additional measured costs,
not evidence for compression.

## Division of authority with ElasticXxx

SML-GENIUS owns all BPB3-X domain semantics, code, correctness tests,
source pin, quantization fidelity and full-model gates.

ElasticXxx should eventually select a **qualified layout ID** from a
versioned capability/evidence contract, if and only if measurements
show a benefit. Its generic runtime owns the typed lifecycle
OBSERVE -> FORECAST -> PLAN -> VALIDATE -> ACT -> VERIFY -> COMMIT/ROLLBACK.
It must not learn BPB3-X's bitplane/scale arithmetic and SML-GENIUS
must not embed a second generic adaptive resource manager.

The current code accepts the caller's layout ID and **does not use
ElasticXxx or actuate live model state**. A later adapter must use only
the public 'elastic' facade, a pinned exact source and eligibility
conditions (valid model/weight identity, version, metrics freshness,
NUMA/cache budget, read/write epochs, global-scale invariants, stable
weights, and rollback of the previous wire and index). Unknown or
stale evidence must fail closed.

No live transition during inference, model format migration, or
runtime promotion is authorized by this PR. Physical tests must be
admitted through Memorithm/RemoteOps. No NVIDIA SDK dependency.

## Measurements that could falsify the hypothesis

- Full 256-coefficient dot: compare X layouts with K whole and
  competent optimized Q3_K SIMD. X may be slower and larger.
- Partial 64/128: compare with **optimized selective K** extraction,
  not a naively forced 110-byte full read; include index overhead and
  actual cache-line / DRAM traffic, not only logical byte counts.
- Measure transcoding, directory/prefix-offset building, memory
  residency and repeated queries, cache-/NUMA-bound kernels, energy
  where available, random ordering and confidence intervals. Freeze
  data, source SHAs, activations and compile settings; include
  sham comparisons and predeclared rejection criteria.
- Selection value: compare Elastic selection against the **best
  fixed candidate**, not only against a deliberately bad static
  baseline. If choosing a layout costs more than it saves, reject X.
- General real-valued activations, training-quality improvements and
  model end-to-end accuracy are **unqualified** in this v0 experiment.

## Invariants qualified in standalone Rust tests

- All six 256-coefficient partitions and exact encoded byte counts.
- Exact byte-for-byte BPB3-K roundtrip across all six layouts and
  64 deterministic source fixtures, including non-finite FP16 metadata
  bit preservation; this is distinct from permitting non-finite values
  in actual inference.
- Direct BPB3-X 256-lane bipolar integer dot matches independent
  BPB3-K direct dot across layouts and inputs, without dense f32 arrays.
- Rejection of malformed lengths, unknown marker, and mismatched global
  FP16 values across segments.

Reproduce under Rust 1.89, from the SML-GENIUS repository root:

    rustfmt --edition 2021 --check research/boolean-posit/bpb3x.rs
    rustc --edition=2021 -D warnings --test research/boolean-posit/bpb3x.rs -o /tmp/bpb3x-tests
    /tmp/bpb3x-tests
    rustc --edition=2021 -D warnings -O --crate-type lib research/boolean-posit/bpb3x.rs -o /tmp/libbpb3x.rlib

Sources: original ggml Q3_K decoder and BPB3-K codec in
research/boolean-posit/gguf_k.rs, bpb3k.rs and bpb3k_direct.rs;
upstream ggml-org/llama.cpp
f7591a6a01f9273de03331732bdc0b39721168be.
Tracking issue: #878. Scientific roadmap: #858.
