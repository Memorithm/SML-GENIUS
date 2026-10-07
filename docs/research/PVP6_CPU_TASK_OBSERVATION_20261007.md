# PVP6 CPU activation-plus-query observation — 2026-10-07

Author: Memorithm. Status: retained native shared-host CPU diagnostic.
No architecture, model-quality or default-route promotion.

## Execution identity

- Protocol and benchmark: SML [PR231](https://github.com/Memorithm/SML-GENIUS/pull/231),
  exact source `ffad90f795ccd806998efe07f347ba303def5cfa`,
  merged as `6d4620efe7037c41e270517a099904798ec99a87` after exact-head CI success.
- RemoteOps [PR313](https://github.com/Memorithm/RemoteOps/pull/313) control and
  [PR314](https://github.com/Memorithm/RemoteOps/pull/314) builtin-timer correction;
  executed control `f3010810b542fd0290580e07d1afe537d8aaa790`, exact-head contract CI success.
- Host `tarek`, native aarch64, Rust 1.89.0 release, no RUSTFLAGS override at build.
- Executable SHA-256:
  `48568b82730f7539cab10a5a6b1cca895e625ec99f088ef102dd9c27c859ef5e`.
- Frozen fixture SHA-256:
  `feb1a144e23d9a29439b7cad6704f56637a7f7ead1a9b9abb39ad9c0ac5325fd`.
- Successful campaign `campaign.s3aWvYit`; controller exit 0.
- Dynamic unprivileged UID per process; observed local memory.max=1073741824
  and cpu.max=100000 100000 in every worker.
- Ten sequential fresh processes, 20 unique cells, five arms, five warmups,
  twenty measured repetitions; configured 120s per process/1800s campaign bounds.
- Host remains shared. The cooperative campaign lock is not CPU exclusivity.
  No existing service was stopped; no GPU or vendor API was used.

## Complete evidence

- 20,000 measured and 5,000 warmup samples.
- 115,160,000 complete output-word comparisons, zero mismatches.
- All ten processes exit 0; all complete end markers present.
- 1000 per-process/cell/arm median and p95 summaries independently recomputed.
- The Rust offline verifier and a separate Python arithmetic cross-check agree.
- Five verifier tests exercise a complete synthetic audit corpus and rejection
  of missing/duplicate samples, source drift, wrong output length/exactness,
  altered summary arithmetic/lifecycle and resource limits.
- Observed final cgroup memory peaks: 2,129,920..2,502,656 bytes, including
  the worker and oracle work. This is not learned-model RAM or VRAM.
- Archive preserves 95 original files across success, failed setup and untimed probe.
- Archive SHA-256:
  `fc0e1f7c41c95d7df36762425f27840a57e927c6588cd2e718b7e13ac02003b7`.

Replay: `bash scripts/check-pvp6-cpu-evidence.sh`.
The script validates the archive, every original file hash, Rust verifier tests,
all raw sample keys/order, process/source/compiler/executable/control identity,
observed cgroup limits, coverage, output equality flags, and summary arithmetic.
It then requires byte-identical audit output to the retained audit.csv.
The frozen benchmark itself performs every full output comparison against the
monomial oracle outside timing; the offline CSV audit does not rerun the model.

## Setup failure retained

`campaign.3Yyr9L5q` exits 127 because external /usr/bin/time is absent on Thor.
The worker reached cgroup observation but never entered the SML benchmark:
zero warmup/measured samples. PR314 replaces that dependency with Bash builtin
time; Instant sample timing, SML source, executable hash and task protocol do not
change. The failed attempt is archived separately, not included among or used to
replace the ten measured processes. `campaign.8NV6sN5n` is the untimed adapter probe.

## Workload surface and arithmetic

Cell IDs refer to the frozen [timing protocol](../SML-PVP6-CPU-TASK-TIMING-PROTOCOL.md).
K=2048, G=16; synthetic PB8 ANF coefficients, not trained-model banks.
Times below are microseconds: the median of ten per-process medians, not a
pooled median. Ratio is the median of ten per-process ratios:
`min(sparse median, submask median, generic median) / Pascal median`.
Its range includes all ten process ratios. It is not the quotient of displayed
central time columns, a kernel-only speedup, a p-value or a confirmatory estimate.
The resident arm has prepaid activation and is excluded from that ratio.

| Cell | Pascal µs | Sparse µs | Submask µs | Generic µs | Resident µs | Primary ratio | Process ratio range |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 14.183 | 42.597 | 674.731 | 2535.814 | 14.437 | 3.003 | 2.763–3.018 |
| 1 | 14.164 | 139.653 | 675.633 | 2588.062 | 13.834 | 9.864 | 9.548–9.907 |
| 2 | 14.162 | 933.633 | 676.956 | 2657.669 | 13.833 | 47.744 | 47.044–47.880 |
| 3 | 14.225 | 3660.887 | 677.385 | 2659.245 | 13.831 | 47.617 | 44.112–48.114 |
| 4 | 6.974 | 234.193 | 158.440 | 2652.653 | 4.254 | 22.692 | 22.291–22.824 |
| 5 | 14.308 | 933.917 | 629.736 | 2658.590 | 13.889 | 44.026 | 43.404–44.322 |
| 6 | 43.354 | 3736.941 | 2518.121 | 2686.109 | 52.525 | 58.021 | 56.931–58.857 |
| 7 | 43.143 | 3735.862 | 2704.660 | 2685.999 | 52.160 | 62.225 | 60.785–62.622 |
| 8 | 160.801 | 15001.856 | 10829.156 | 2808.144 | 206.256 | 17.564 | 17.036–17.671 |
| 9 | 42.660 | 3745.838 | 2685.099 | 2700.736 | 52.021 | 62.690 | 61.176–63.016 |
| 10 | 158.088 | 15002.909 | 10639.084 | 2814.854 | 205.900 | 17.824 | 17.278–17.980 |
| 11 | 618.963 | 59956.346 | 42697.830 | 3288.915 | 816.517 | 5.316 | 5.288–5.466 |
| 12 | 6.984 | 234.197 | 9.046 | 2648.253 | 4.259 | 1.297 | 1.066–1.302 |
| 13 | 14.210 | 933.584 | 69.925 | 2656.160 | 13.819 | 4.923 | 4.778–4.948 |
| 14 | 6.981 | 234.215 | 827.530 | 2647.865 | 4.257 | 33.554 | 33.017–33.720 |
| 15 | 14.190 | 933.721 | 1711.742 | 2652.934 | 13.822 | 65.816 | 65.469–66.149 |
| 16 | 5.171 | 3.648 | 43.199 | 2520.698 | 1.875 | 0.705 | 0.692–0.711 |
| 17 | 5.153 | 3.636 | 2.194 | 2518.387 | 1.877 | 0.424 | 0.416–0.462 |
| 18 | 5.185 | 3.639 | 343.266 | 2519.684 | 1.875 | 0.703 | 0.691–0.706 |
| 19 | 621.865 | 244990.171 | 44015.806 | 3315.636 | 817.669 | 5.335 | 5.286–5.415 |

## Decision and limits

Pascal is faster than the best activation-plus-query control in 17 cells,
with every one of the ten process ratios above 1 in those cells.
The three density=4/Q=32/R=1 populations favor direct evaluation in every
process. Those negative regimes are retained. Generic materialization is the
stronger control in some high-reuse cells and must not be omitted to inflate
Pascal's ratio. Resident lookup is not uniformly faster despite prepaid
activation; this campaign does not isolate the reason.

Retain the workload-dependent CPU utility surface. Do not install a default
router or selection threshold: the host is shared and these banks are synthetic.
The complete task includes validation, allocation, materialization and queries;
it does not measure training, coefficient updates, recurrent state, language
quality, model tokens/s, SIMD/GPU task utility or capacity scaling.
The closed PGF2 SBG representation-mixing result remains closed.

Next: consume the same fixture and output contract through native SciRust SIMD
and optional FLAT/NNIS portable GPU paths; qualify cost and correctness in each
destination. Export declared integration/learned banks before architecture
selection. Final SML remains self-sufficient and its harness remains separate.
