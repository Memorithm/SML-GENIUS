# PCEQ-1 — Fixed-protocol producer/quantizer/consumer benchmark

Status: **research only**, source candidate; no hardware measurements included.
Owner: SML-GENIUS. Previous exact oracle: PCEQ-0, issue #829 / PR #830.
Do not infer any FP32 activation, ANF/PVP, KV or real-model quality improvement.

## Registered controls and candidate

The benchmark source is `crates/sml-core/src/bin/sml-pceq1-bench.rs`.
Inputs are deterministic signed i16 values from an index/seed generator.
`seed=0x517420261106f00d`. A separately generated signed i16
weighted-sum vector is reused by all implementations. Same complete
synthetic producer→quantizer→weighted-sum path is timed.

- A: construct a full i16 Vec, then two-pass per-block exact-absmax
  `quantize_materialized`, then use the same weighted-sum consumer;
  source materialization, allocation and reading are included.
- B: generate i16 values once into at most 128 stack values, compute
  an exact absmax, quantize and consume `quantize_fused` with no full
  unquantized input Vec.
- C: same fused bounded-source approach but use a conservative OR
  magnitude envelope to determine each power-of-two scale.

Fixed lengths: 4096, 65536, 262144. Fixed block sizes: 32, 64, 128.
Fixed input distributions: `balanced` (full i16), `sparse`
(dominantly zero), `outliers` (two extremes in every 64-value run).
Three warmups; nine timed repetitions per candidate. Round-robin timing
order rotates to mitigate first-candidate bias. Report median and
empirical order-statistic p95 of elapsed nanoseconds plus raw checksums.

The exact fused candidate must produce identical codes, metadata, and
weighted sum to the materialized control **before any timing**.
The OR path is permitted to incur greater numerical distortion, never
to silently saturate: each block's reconstructed error is independently
measured as squared error and maximum absolute deviation. Compare
error and speed together, never speed alone.

## Scope of measurement and interpretation

No counter records actual DRAM traffic in this first phase.
The `materialized_i16_bytes` field is the **logical size** of the control
input tensor, not actual DDR bytes read or written.
The `logical_bytes` field is the PCEQ code+metadata accounting,
not allocator capacity/RSS or physical serialisation.
Console output explicitly says `drambytes=UNMEASURED`.
In-memory caching and compiler effects may substantially alter
physical traffic; the evaluation **must not claim any DRAM reduction**
from timing or logical bytes alone.

Per host separately record source SHA, RemoteOps workflow SHA, runner
name, CPU, NUMA, RAM limits, rustc version, compiler flags, kernel,
clock/governor state when readable, and loadavg. Collect `/usr/bin/time -v`
(maximum resident set size, page faults, real time) if available and
retain stdout, stderr and SHA256 digests. Do not report energy without
a qualified meter. Probe `perf stat` only as an optional,
non-blocking, permission-qualified second pass with kernel event metadata;
`perf` failure is *not* proof of zero traffic.

**Do not mix Dell and Thor latency distributions or compare cross-host
speedups.** A and B are comparable only on the same host under matched
input, build mode and execution budget. Run protected, trusted exact
SHA on Dell/Thor via Memorithm/RemoteOps and verify code provenance,
runner identity, bounded wall time, non-root status, no secrets,
and minimum CPU/RAM admission. Keep CPU governor changes out of scope.

## Promotion gates

- Rust formatting, Clippy, tests and independent control comparisons pass.
- Positive speed claim requires paired native host data with variability,
  no saturation and acceptable reconstruction error.
- More substantive memory traffic claim requires qualified DRAM counters
  and an independent assessment that the counter model is valid.
- Real SML model quality and generalization claims require model-aligned
  numerical activations and real tasks not included in PCEQ-1.
- An exact ANF/PVP Boolean bank has distinct semantics: never substitute
  this lossy numerical quantizer without a full formal equivalence review.
- Do not merge a PR without green required checks on the exact PR head.
