# GB-4 native x86_64 observation — 2026-10-23, user-provided terminal transcript

**Status:** qualified as a *reported benchmark observation*, not independently archived native-host evidence or a model-speed qualification. Origin of numerical rows: console output pasted by the owner, 2026-10-23. Raw CSV and host metadata have **not** been downloaded into this repository or inspected independently.

## Reproduction identity

- SML source SHA: `c8613098ec6cf3cf59b784f1d425e12f8c710283` (GB-4 PR #667).
- Architecture reported by the terminal: `x86_64`; OS: Debian, not further pinned in the pasted evidence.
- Rust toolchain selected and confirmed in transcript: `rustc 1.89.0 (29483883e 2025-08-04)`. The *host default* before the run was Rust 1.98.1 and was not used by the pinned-worktree command.
- Command: `cargo test --locked --release -p sml-core --bin sml-geometric-anf-cost`, then `cargo build --locked --release -p sml-core --bin sml-geometric-anf-cost`, then both `deterministic` and `benchmark` CLI modes.
- Local only, **not** a network artifact: `/root/gb4-evidence/20261023T070224Z-3132633`; expected files `environment.txt`, `rust-version.txt`, `binary.sha256`, `tests.log`, `build.log`, `deterministic.csv`, `benchmark.csv`.
- User-reported outputs: `SUCCÈS`; CSV line counts 37 for deterministic and 973 for benchmark. Accounting matches 36 deterministic cells (4 levels × 3 predicates × 3 sample lengths) and 972 timed rows (36 cells × 3 paths × 9 measurements). This supports command-level completion only; distributional diagnostics require raw CSV inspection.
- The terminal transcript does **not** provide `lscpu`, governor, frequency, thermal readings, external CPU contention, CPU affinity, checksum, full CSV or binary host architecture attestation. Do not identify the specific CPU from this output alone.

## Medians from user output (ns per point, full corpus only)

| Sectors | Family | SCAN+LUT | BINARY+LUT | BINARY+ANF | ANF/LUT |
| ---: | --- | ---: | ---: | ---: | ---: |
| 8 | sparse | 31.9 | 36.3 | 44.2 | 1.22 |
| 8 | alternating | 31.9 | 36.3 | 44.1 | 1.22 |
| 8 | pi | 31.9 | 36.4 | 44.2 | 1.22 |
| 16 | sparse | 46.0 | 43.4 | 52.9 | 1.22 |
| 16 | alternating | 46.0 | 43.4 | 52.9 | 1.22 |
| 16 | pi | 46.0 | 43.4 | 52.9 | 1.22 |
| 32 | sparse | 72.9 | 51.3 | 66.4 | 1.29 |
| 32 | alternating | 73.0 | 51.4 | 66.9 | 1.30 |
| 32 | pi | 72.9 | 51.5 | 66.5 | 1.29 |
| 64 | sparse | 126.8 | 59.8 | 86.7 | 1.45 |
| 64 | alternating | 127.0 | 59.6 | 86.7 | 1.46 |
| 64 | pi | 126.4 | 59.6 | 86.9 | 1.46 |

## Falsifiable conclusions, confined to this observation

1. At 8 cells, **scan + LUT** is faster by observed median. From 16 cells onward, **binary + LUT** is faster than scan in all three recorded predicate families.
2. At 64 cells with `pi`, binary+LUT reduces measured time relative to scan+LUT from 126.4 to 59.6 ns/point, a `(126.4 - 59.6) / 126.4 ≈ 52.8%` *latency decrease*, or `126.4/59.6 ≈ 2.12` times the instantaneous throughput if tasks are exactly comparable.
3. Binary+ANF is slower than binary+LUT for **all 12** reported full-corpus comparisons, ranging from approximately 1.22× to 1.46× latency. This does not refute the exact semantics, nor future optimized bit-sliced ANF hardware representations.
4. The threshold/crossover is specific to this implementation, optimized compiler, reported machine and deterministic workload; **not** a universal 16-cell theorem or an acceleration claim for SML-25B.
5. The semantic parity condition was implemented as a pre-timing gate and the deterministic command completed, but actual test output and checksum distribution were not included in the pasted transcript. Reopen on the full evidence files before issuing a fully independent validation statement.

## Next steps, without retroactively changing the preregistered protocol

- Collect the owner's raw `benchmark.csv`, `deterministic.csv` and metadata, check exactly nine finite measurements per (level,family,sample-length,path), median/min/max and checksum consistency.
- Run the same pinned SML commit and rustc 1.89.0 on a native Thor ARM64 CPU runner through a new RemoteOps **control-plane** workflow. Keep kernel/CPU identity, governor/frequency/thermal data, elapsed raw values and checksums separate for each machine. Do not aggregate x86_64/ARM64 nanoseconds.
- Repeat paired native runs and report within-host distribution before searching for conditional CPU dispatch or any ANF fusion; only then evaluate a new model-aligned candidate under matched resource budgets.
