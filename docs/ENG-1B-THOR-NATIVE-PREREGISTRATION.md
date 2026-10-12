# ENG-1b/N1 — native Jetson Thor CPU study, frozen protocol

**Preregistered:** 2026-10-12, **before** introducing native workflow and
qualification script or executing a native measurement.

This follow-up is a *hardware diagnostic* of the already qualified,
self-sufficient SML-GENIUS **ENG-1b** representation. It is NOT a new
predictive model, trained-language evaluation, proof of GPU optimization,
or an automatic authorization to switch default model resources.

## Source and scientific boundaries

- Source: `Memorithm/SML-GENIUS`, trusted `main` only.
- Frozen code blob `crates/sml-core/src/elastic_ngram_trie.rs`:
  `e058f6a7078b84e8e6d5eb344dc19d715daad646`.
- Frozen native binary source blob
  `crates/sml-core/src/bin/sml-eng1b-resource-bench.rs`:
  `dd9e59374e4ccaf137ff2e209df8820afc92c825`.
- The existing ENG-1b synthetic 64×96 byte generator is unchanged;
  6,080 TRAIN next-byte events and the fixed offset-137 heldout query
  stream, 256 warmups/2,048 measured inference calls per process.
- Reference Witten-Bell source and ENG-0/ENG-1a comparisons unchanged.
- Prior hosted measurements are exposed at
  `docs/RESULTS-ENG-1B-TRIE-HOSTED-DIAGNOSTIC.md`; this is not a blind
  prediction-quality test and may be replicated without exhausting a
  language quality TEST set.
- Proposed runtime: **Jetson Thor CPU**, aarch64, Rust **1.89.0**,
  SML-GENIUS integrated and immutable `main` tree; GPU is not measured.
  No CUDA, NVML, TensorRT, NVRTC or proprietary NVIDIA toolkit.
  A machine name check is an identity hint and not cryptographic
  hardware attestation.
- No candidate PR code on a persistent self-hosted runner. The new
  native workflow has **only `push` to `main` and `workflow_dispatch`
  on `main`** as triggers, never `pull_request` or `pull_request_target`.
  The PR itself must pass GitHub-hosted structural/CI checks first.
  Do not reuse the PVP6 correctness workflow or start background services.

## Fixed native workload

The native action, after its trusted merge, verifies:
- repository, integrated `GITHUB_SHA`, aarch64, host `tarek`,
  pinned Rust version, and **the two frozen SML code blob IDs above**.
  Unexpected source/host/architecture means **NO MEASUREMENT**.
- Build the pre-existing `sml-eng1b-resource-bench` from main
  `--locked --release` with `CARGO_BUILD_JOBS=2`,
  `CARGO_INCREMENTAL=0`, no external model or data download.
- Run on the actual ARM CPU and retain five sequential **triples** of
  fresh OS processes. Alternate per-triple order TRIE→LONG→LEGACY for
  rounds 1/3/5 and LONG→TRIE→LEGACY for rounds 2/4.
  Legacy has model depth 7 and is a historical non-matched control,
  never included in the depth-32 relative promotion gate.
- No hidden corpus selection, environment tuning, program branch
  recompile between rounds, silent retry of failed measurements or
  reduction of the preregistered sample count.
- Record complete unmodified per-process log lines (TRAIN durations,
  Linux `VmRSS` after training and `VmHWM` lifetime peak in KiB,
  median and p95 lookup in nanoseconds, counts and prediction digest),
  exact Git commit/object IDs, Rust compiler version, architecture,
  observed hostname, number of logical CPUs and a **before/after**
  Linux loadavg + MemAvailable sample to identify contention.
- A successful measurement must have real nonzero `VmRSS`, `VmHWM`,
  train and p95 timings. This is process-level memory, **not**
  per-object allocation, GPU RAM, bandwidth or cgroup-peak evidence.
  A missing field does not become zero or a synthetic estimate.
  Any inability to run on the named host is a **DEFERRED/FAILED**
  observation, not an inferred measurement.

## Mandatory functional parity and falsifiers

For *every* round, compare TRIE and LONG at depth **32**:
- same 6,080 TRAIN events, **96,733** exact contexts and
  **101,212** distinct context/next-byte pairs;
- complete prediction query digest equal between arms, and equal
  to previously frozen hosted `5c4ad424180a35ae`;
- all 2,048 timed predictions with 256 untimed warmups;
- native Rust unit parity tests pass `elastic_ngram_trie` and
  `elastic_ngram_long` unchanged before any timing.
- A checksum mismatch, unexpected context count, invalid/nonpositive
  metric, missing artifact, invalid identity/branch or unsupported
  runtime is a **correctness failure**. Stop, do not claim parity.

## Physical promotion threshold (diagnostic only)

For each paired native round, evaluate unchanged earlier ENG-1b gate:

`VmHWM_TRIE <= 0.90 * VmHWM_LONG` **and**
`p95_TRIE <= 1.10 * p95_LONG`.

- A positive five-round native diagnostic requires **all five**
  comparisons PASS; report per-round results and the median physical
  ratios. If any fails, report `ENG1B_THOR_RESOURCE_VERDICT=STOP`.
  The scientific negative result does NOT fail engineering CI provided
  all measurements and checksums are complete.
- Even five positive runs on a single Thor host and a synthetic corpus
  are correlated observations. Neither they nor the x86 result prove
  general performance or justify default route promotion. A real
  language workload, model-quality evaluation, independent repeated
  sessions, GPU-inclusive traffic/memory checks when relevant and
  model/runtime rollback contract are **separate future gates**.
- No inexact context-depth switching may be described as lossless or
  authorized on the strength of this campaign.

## Execution and evidence

The PR introduces a **hosted-only PR contract test** checking the
shell syntax/trusted `main` native triggering; this cannot execute
the native workload. On merged main, the new native workflow runs
once via its `push` path trigger. It may also be retried explicitly
using `workflow_dispatch` on trusted main, without changing frozen
criteria. Raw logs plus manifest and decision are uploaded as one
readable artifact; no credential or host-private files are included.

Native job has a bounded timeout and build parallelism. The runner
may still contend with other host tasks: a real low-load/exclusivity
claim is deliberately **not** made.

Ownership: SML owns Ngram model/digest/equivalence;
RemoteOps owns physical host/runtime; ElasticXxx owns generic
resource transition planning and must not be considered connected
by this observational run. Existing issue
https://github.com/Memorithm/ElasticXxx/issues/244 remains open.
