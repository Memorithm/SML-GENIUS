# ENG-1b — exact reverse-suffix trie: first hosted diagnostic and limitations

**Measurement date:** 2026-10-12 UTC, not independent replicated evidence.
**Reference preregistration:** [ENG-1B-TRIE-PREREGISTRATION.md](ENG-1B-TRIE-PREREGISTRATION.md),
written in first ENG-1b commit `01d7ac430a240d48b7c4754cc570776333e09fde`
**before** the new implementation, synthetic fixtures, tests and any measurements.

## Exact provenance

- Repository: `Memorithm/SML-GENIUS`.
- Measured source head: `51a00699bd96f00f4b375a8d8005f9c2b1fc298a`.
- Trie Rust blob SHA: `e058f6a7078b84e8e6d5eb344dc19d715daad646`.
- Measurement workflow: [GitHub Actions run 38311439857](https://github.com/Memorithm/SML-GENIUS/actions/runs/38311439857);
  dedicated job `115017866813` concluded **success** (format, check, warnings-as-errors
  Clippy, exact oracle tests, three physical resource process executions).
- Unmodified benchmark logs and environment: [artifact 11774254337](https://github.com/Memorithm/SML-GENIUS/actions/runs/38311439857/artifacts/11774254337)
  (`eng1b-suffix-trie-physical-diagnostic`, 1,489 compressed bytes; ZIP
  byte checksum **not** independently retrieved/recomputed).
- Runner: GitHub-hosted `ubuntu-24.04` x86_64, `rustc 1.89.0`,
  optimised `--release` bin, `RUSTFLAGS=-D warnings`.
- Frozen input: 64 lanes × 96 bytes using the preregistered public generator;
  6080 TRAIN target events. Same independent deterministic offset-137
  heldout query stream for all three arms. 256 untimed warmups then 2048
  timed 256-class predictions per process. The arithmetic checksum for
  TRIE and LONG matches across those identical query inputs.
- Report is *documentation of the measured source*. A later report-only
  final HEAD must pass its own complete exact-head CI before merge;
  no performance requalification is claimed from documentation edits.

## Correctness

The ENG-1b Rust tests passed on the recorded HEAD, including:
- Exact equality of every `f64::to_bits()` class probability to the
  complete-suffix ENG-1a oracle at all depths 0..32 on declared synthetic
  observed/unseen probes; independent legacy ENG-0/NgramModel at 0..7.
- Exact heldout 64-lane score/accuracy/denominator agreement at 0/4/7/12/16/24/32.
- Twelve-byte suffix witness separating two histories that have identical
  most recent seven bytes, plus lane permutation and immutable prediction
  tests. Budget overshoots fail closed.
- The production default route and original reference source were **not**
  modified. This is functional parity, not next-byte natural language quality.

## Three-process hosted diagnostic

Lower is better; **TRIE** and **LONG** are comparable at depth 32.
The **LEGACY** arm uses depth 7 and is a contextual reference **not**
a depth-matched memory or speed competitor.

| Frozen metric | TRIE, depth 32 | LONG, depth 32 | LEGACY, depth 7 |
|---|---:|---:|---:|
| TRAIN events | 6,080 | 6,080 | 6,080 |
| Distinct contexts | 96,733 | 96,733 | 8,946 |
| Distinct context/next-byte pairs | 101,212 | 101,212 | 11,346 |
| Logical non-counter keys/labels/edges (bytes)* | 1,068,533 | 2,010,371 | 82,914 |
| Logical u32 counter payload (bytes)* | 791,780 | 791,780 | 81,168 |
| Process VmRSS after training (KiB) | 15,672 | 22,012 | 3,760 |
| **Process VmHWM peak (KiB)** | **15,804** | **22,148** | 3,892 |
| TRAIN wall time (µs) | 19,065 | 51,700 | 4,061 |
| Lookup median/p50 (ns) | 3,907 | 9,478 | 1,563 |
| **Lookup p95 (ns)** | **4,569** | **10,800** | 1,663 |
| Probability rolling digest | `5c4ad424180a35ae` | `5c4ad424180a35ae` | `d2cca7b68d5a0784` |

*These *logical fields* have different layout components by arm:
TRIE includes logical parent/child u32 edges, byte labels and depth tags;
LONG includes full context byte sequences and follower byte labels.
Neither captures `BTreeMap` nodes, allocator fragmentation/capacity,
metadata alignment or RSS.

Observed hosted-process VmHWM reduction: **6,344 KiB (28.64%)**,
TRIE vs LONG. Observed p95 lookup reduction: **6,231 ns (57.69%)**.
These are measurements of one predeclared synthetic fixture at one
point in time, *not* inferable improvements on arbitrary text, larger
corpora, Jetson Thor, other architectures or true per-object allocations.

## Frozen admission test and verdict

Preregistered two-part diagnostic gate:
`VmHWM_trie <= 0.90*VmHWM_long` AND
`p95_trie <= 1.10*p95_long`, with matching full-prediction digest.

Observed: both numerical inequalities hold, and the same-query 256-class
rolling digests match. The workflow printed:

`ENG1B_PROMOTION_GATE=PASS,kind=hosted_diagnostic_only`

**Consequence:** the opt-in trie is a promising exact reference and
qualifies for the **next validation campaign**, NOT automatic promotion
to SML default inference. The scientific gate requires independent
same-hardware repeated runs and a real model-aligned corpus. Physical
on-device actuation remains blocked pending model-owned qualification
and the ElasticXxx generic resource transaction boundary.

## Explicit limitations / next obligations

1. Ubuntu-hosted VmRSS/HWM includes process runtime/allocator and
   is not measured at granularity of allocations or mmap/page cache.
2. No independent replicate of the corpus or cross-device native Thor
   measurement was executed; one hosted runner and one fixture per arm.
   Scheduler/DVFS/thermal/allocator state may affect lookup timings.
3. Full 256-way equality was checked on predefined synthetic fixtures,
   not exhaustively for every possible 32-byte history.
4. CPU cycles, cache misses, actual memory traffic, disk bytes, major
   faults, energy and GPU memory are **unmeasured**.
5. CTW/PPM, conditional-depth routing and long-distance language
   benefits remain **unimplemented or unproven** here. A depth change
   may change the probability distribution and requires independent
   SML model-quality authorization; this report cannot authorize it.
6. The old A6-E4/E5 protected TEST data were not used to train or
   optimize this representation. Refer to [ENG-1b issue #526](https://github.com/Memorithm/SML-GENIUS/issues/526)
   and [ElasticXxx #244](https://github.com/Memorithm/ElasticXxx/issues/244)
   for the staged next experimental/resource gates.
