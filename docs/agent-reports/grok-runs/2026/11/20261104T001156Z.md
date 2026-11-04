# Grok SML-GENIUS report — 2026-11-04T00:11:56Z

- Status: **NOT_CONNECTED** for the RemoteOps 30-minute loop (no session_attestation; E2E task `sml-genius-20261103T191749Z-8152791e` still NOT_QUALIFIED; 30m schedule NOT enabled). Code work done in a manually-triggered session.
- Base: 25794bcb (main, #809). Tested code commit: 19a4b958e4a1e2b4729eca2d3b259a58219f0769 on `feat/g1-context3-latency-rss`. Milestone G1 / CONTEXT-3. LONGCTX-B1 / BDE-1 / PR #805 untouched.
- PR: https://github.com/Memorithm/SML-GENIUS/pull/810

## Change (measurement only; no retrieval behaviour change)
- `crates/sml-store/src/context_retrieve_bench.rs`: nearest-rank percentiles, `LatencySummary` (p50/p95/max ns), `measure_latency` (warmup + per-call wall clock, fail-closed), Linux `VmHWM`/`VmRSS` parsing and `/proc/self/clear_refs` HWM reset (`None` → NOT_MEASURED). 5 unit tests.
- `crates/sml-store/examples/g1_latency_rss.rs` + shared `examples/support/g1_fixture.rs` (same declared synthetic distributions as the two-level tests).
- `crates/sml-store/tests/context_retrieve_latency.rs` (2 tests: harness on capped/uncapped paths, determinism).

## Protocol
`cargo run --release --locked -p sml-store --example g1_latency_rss [N] [QUERIES]`; prefix_bits 8, top_k 1, 256 queries, 1 warmup pass, single run, no repeats → no uncertainty interval. Engines: `exhaustive` (uncapped reference), `neighbor` (`retrieve_with_neighbor_radius`) and `global_wfull` (`retrieve_global_probe`, width 56), radius {1,2} × cap {32,64,128,256}. R@1 here = top-1 Hamming distance equals oracle best (tie-tolerant), computed in an untimed pass. Latency includes verified raw reread (file I/O, page cache warm). Host: shared Linux VM, 8 vCPU "Intel Xeon Processor", x86_64 — NOT a qualified runner (Dell/Thor not used).

## Measured — N=1024 (synthetic)
| dist | engine | r | cap | R@1 | p50 µs | p95 µs |
|---|---|---|---|---|---|---|
| clustered | exhaustive | – | none | 1.000 | 30.6 | 38.1 |
| clustered | neighbor | 2 | 32 | 0.133 | 6.7 | 6.8 |
| clustered | global_wfull | 1 | 32 | 0.969 | 10.8 | 11.1 |
| clustered | global_wfull | 2 | 32 | 1.000 | 11.1 | 16.1 |
| clustered | global_wfull | 2 | 256 | 1.000 | 16.8 | 21.9 |
| noisy_clustered | exhaustive | – | none | 1.000 | 30.7 | 34.5 |
| noisy_clustered | global_wfull | 2 | 32 | 1.000 | 12.3 | 15.2 |
| uniform | exhaustive | – | none | 1.000 | 30.5 | 37.3 |
| uniform | global_wfull | 2 | 32 | 1.000 | 11.5 | 13.1 |

N=4096: exhaustive p50/p95 151.6/231.3 µs (clustered), 132.9/148.2 (noisy), 130.0/171.6 (uniform); global_wfull r2 cap32 26.9/47.9 (R@1 1.000), 25.4/35.6 (1.000), 20.9/29.9 (1.000); neighbor r2 cap32 clustered R@1 0.031.

Peak RSS: process VmHWM 3200 KiB (N=1024 run), 3972 KiB (N=4096 run). Per-cell HWM delta (reset per cell) = 0 KiB for every capped cell; exhaustive cells 0–56 KiB (N=1024) and 0–224 KiB (N=4096). Page-granular, so sub-page allocations are invisible.

## Honesty notes
- Synthetic only; single run on a shared non-qualified VM; latencies are noisy (max column shows spikes). No generalization claim; not LONGCTX-B1.
- global_wfull's in-memory scan is O(probed members) — cap bounds rerank/reread only; at N=4096 its p50 is 2–4× neighbor's but ~5× below exhaustive.
- NOT_MEASURED: page faults, qualified-runner latency, real-text corpora, repeated-run variance.

## Next
Repeat on a qualified runner (Dell/Thor via RemoteOps) with ≥5 repeats and page-fault counters; then N≥16k scaling.

## Raw (N=1024)
```
G1_LAT_META profile=release os=linux arch=x86_64 segments=1024 queries=256 warmup=1 prefix_bits=8 top_k=1 hwm_reset=true synthetic=true
G1_LAT dist N engine radius cap R@1 mean_shortlisted p50_us p95_us max_us rss_before_kib peak_rss_delta_kib
G1_LAT clustered 1024 exhaustive 0 none 1.000 1024.0 30.6 38.1 54.6 2916 0
G1_LAT clustered 1024 neighbor 1 32 0.133 31.0 6.6 8.1 16.7 2916 0
G1_LAT clustered 1024 global_wfull 1 32 0.969 31.0 10.8 11.1 16.8 2916 0
G1_LAT clustered 1024 neighbor 1 64 0.262 62.0 7.8 8.0 12.9 2916 0
G1_LAT clustered 1024 global_wfull 1 64 0.969 62.0 12.0 12.6 19.0 2916 0
G1_LAT clustered 1024 neighbor 1 128 0.520 124.0 10.5 12.4 16.3 2916 0
G1_LAT clustered 1024 global_wfull 1 128 0.969 124.0 12.7 15.2 19.6 2916 0
G1_LAT clustered 1024 neighbor 1 256 0.969 248.0 16.5 17.2 23.1 2916 0
G1_LAT clustered 1024 global_wfull 1 256 0.969 248.0 16.8 17.2 35.9 2916 0
G1_LAT clustered 1024 neighbor 2 32 0.133 32.0 6.7 6.8 6.9 2916 0
G1_LAT clustered 1024 global_wfull 2 32 1.000 32.0 11.1 16.1 26.4 2916 0
G1_LAT clustered 1024 neighbor 2 64 0.270 64.0 8.0 8.3 18.4 2916 0
G1_LAT clustered 1024 global_wfull 2 64 1.000 64.0 11.4 16.2 21.3 2916 0
G1_LAT clustered 1024 neighbor 2 128 0.535 128.0 10.2 10.5 10.7 2916 0
G1_LAT clustered 1024 global_wfull 2 128 1.000 128.0 12.7 17.6 22.3 2916 0
G1_LAT clustered 1024 neighbor 2 256 1.000 256.0 16.5 17.1 22.4 2916 0
G1_LAT clustered 1024 global_wfull 2 256 1.000 256.0 16.8 21.9 38.3 2916 0
G1_LAT noisy_clustered 1024 exhaustive 0 none 1.000 1024.0 30.7 34.5 53.2 3116 56
G1_LAT noisy_clustered 1024 neighbor 1 32 0.785 29.4 7.6 8.0 14.5 3124 0
G1_LAT noisy_clustered 1024 global_wfull 1 32 0.969 29.4 9.0 11.3 19.3 3124 0
G1_LAT noisy_clustered 1024 neighbor 1 64 0.926 52.2 8.8 9.2 13.5 3124 0
G1_LAT noisy_clustered 1024 global_wfull 1 64 0.969 52.2 9.0 10.4 15.7 3124 0
G1_LAT noisy_clustered 1024 neighbor 1 128 0.969 76.0 10.0 12.3 22.0 3124 0
G1_LAT noisy_clustered 1024 global_wfull 1 128 0.969 76.0 9.3 11.8 15.5 3124 0
G1_LAT noisy_clustered 1024 neighbor 1 256 0.969 79.5 9.8 14.4 19.6 3124 0
G1_LAT noisy_clustered 1024 global_wfull 1 256 0.969 79.5 9.2 12.3 13.3 3124 0
G1_LAT noisy_clustered 1024 neighbor 2 32 0.793 32.0 8.4 9.8 20.0 3124 0
G1_LAT noisy_clustered 1024 global_wfull 2 32 1.000 32.0 12.3 15.2 41.7 3124 0
G1_LAT noisy_clustered 1024 neighbor 2 64 0.934 63.6 9.6 10.4 16.4 3124 0
G1_LAT noisy_clustered 1024 global_wfull 2 64 1.000 63.6 12.0 14.3 18.3 3124 0
G1_LAT noisy_clustered 1024 neighbor 2 128 0.984 124.1 13.0 13.8 19.0 3124 0
G1_LAT noisy_clustered 1024 global_wfull 2 128 1.000 124.1 13.5 18.7 23.3 3124 0
G1_LAT noisy_clustered 1024 neighbor 2 256 1.000 201.0 19.5 24.1 32.5 3124 0
G1_LAT noisy_clustered 1024 global_wfull 2 256 1.000 201.0 15.8 19.8 22.4 3124 0
G1_LAT uniform 1024 exhaustive 0 none 1.000 1024.0 30.5 37.3 53.7 3196 4
G1_LAT uniform 1024 neighbor 1 32 0.922 31.3 8.9 10.2 16.7 3200 0
G1_LAT uniform 1024 global_wfull 1 32 0.969 31.3 8.1 8.4 14.9 3200 0
G1_LAT uniform 1024 neighbor 1 64 0.969 37.3 8.6 9.1 9.3 3200 0
G1_LAT uniform 1024 global_wfull 1 64 0.969 37.3 8.8 9.6 17.4 3200 0
G1_LAT uniform 1024 neighbor 1 128 0.969 37.3 8.6 9.2 15.5 3200 0
G1_LAT uniform 1024 global_wfull 1 128 0.969 37.3 8.3 11.0 12.6 3200 0
G1_LAT uniform 1024 neighbor 1 256 0.969 37.3 8.8 10.9 20.1 3200 0
G1_LAT uniform 1024 global_wfull 1 256 0.969 37.3 8.3 9.1 17.1 3200 0
G1_LAT uniform 1024 neighbor 2 32 0.922 32.0 9.1 9.4 10.2 3200 0
G1_LAT uniform 1024 global_wfull 2 32 1.000 32.0 11.5 13.1 18.3 3200 0
G1_LAT uniform 1024 neighbor 2 64 0.977 64.0 11.2 14.0 21.0 3200 0
G1_LAT uniform 1024 global_wfull 2 64 1.000 64.0 11.5 13.6 23.5 3200 0
G1_LAT uniform 1024 neighbor 2 128 0.996 128.0 15.0 16.9 43.2 3200 0
G1_LAT uniform 1024 global_wfull 2 128 1.000 128.0 12.6 15.9 28.7 3200 0
G1_LAT uniform 1024 neighbor 2 256 1.000 150.0 16.9 18.8 25.2 3200 0
G1_LAT uniform 1024 global_wfull 2 256 1.000 150.0 13.1 13.9 17.6 3200 0
G1_LAT_PROCESS peak_rss_kib=3200
```
## Raw (N=4096)
```
G1_LAT_META profile=release os=linux arch=x86_64 segments=4096 queries=256 warmup=1 prefix_bits=8 top_k=1 hwm_reset=true synthetic=true
G1_LAT dist N engine radius cap R@1 mean_shortlisted p50_us p95_us max_us rss_before_kib peak_rss_delta_kib
G1_LAT clustered 4096 exhaustive 0 none 1.000 4096.0 151.6 231.3 277.8 3640 0
G1_LAT clustered 4096 neighbor 1 32 0.031 31.0 7.3 15.5 27.9 3352 0
G1_LAT clustered 4096 global_wfull 1 32 0.969 31.0 36.7 48.9 53.1 3352 0
G1_LAT clustered 4096 neighbor 1 64 0.062 62.0 8.2 14.2 16.9 3352 0
G1_LAT clustered 4096 global_wfull 1 64 0.969 62.0 27.0 45.9 58.6 3352 0
G1_LAT clustered 4096 neighbor 1 128 0.125 124.0 10.8 19.6 26.1 3352 0
G1_LAT clustered 4096 global_wfull 1 128 0.969 124.0 35.5 54.9 61.6 3352 0
G1_LAT clustered 4096 neighbor 1 256 0.234 248.0 17.3 27.4 75.5 3352 0
G1_LAT clustered 4096 global_wfull 1 256 0.969 248.0 34.2 61.3 70.3 3352 0
G1_LAT clustered 4096 neighbor 2 32 0.031 32.0 7.3 14.6 19.3 3352 0
G1_LAT clustered 4096 global_wfull 2 32 1.000 32.0 26.9 47.9 78.4 3352 0
G1_LAT clustered 4096 neighbor 2 64 0.062 64.0 9.1 18.2 26.8 3352 0
G1_LAT clustered 4096 global_wfull 2 64 1.000 64.0 27.6 50.1 97.1 3352 0
G1_LAT clustered 4096 neighbor 2 128 0.129 128.0 10.8 17.0 22.3 3352 0
G1_LAT clustered 4096 global_wfull 2 128 1.000 128.0 32.3 56.2 81.8 3352 0
G1_LAT clustered 4096 neighbor 2 256 0.246 256.0 17.2 18.7 25.0 3352 0
G1_LAT clustered 4096 global_wfull 2 256 1.000 256.0 33.6 54.9 77.1 3352 0
G1_LAT noisy_clustered 4096 exhaustive 0 none 1.000 4096.0 132.9 148.2 245.0 4056 224
G1_LAT noisy_clustered 4096 neighbor 1 32 0.465 31.9 7.8 8.5 17.7 3704 0
G1_LAT noisy_clustered 4096 global_wfull 1 32 0.969 31.9 14.1 20.9 35.2 3704 0
G1_LAT noisy_clustered 4096 neighbor 1 64 0.598 63.1 9.6 14.1 29.8 3704 0
G1_LAT noisy_clustered 4096 global_wfull 1 64 0.969 63.1 14.6 20.9 36.0 3704 0
G1_LAT noisy_clustered 4096 neighbor 1 128 0.758 118.5 12.6 17.8 27.6 3704 0
G1_LAT noisy_clustered 4096 global_wfull 1 128 0.969 118.5 15.9 22.0 24.9 3704 0
G1_LAT noisy_clustered 4096 neighbor 1 256 0.918 210.5 19.0 21.5 27.4 3704 0
G1_LAT noisy_clustered 4096 global_wfull 1 256 0.969 210.5 20.3 27.3 57.6 3704 0
G1_LAT noisy_clustered 4096 neighbor 2 32 0.465 32.0 9.0 10.9 21.1 3704 0
G1_LAT noisy_clustered 4096 global_wfull 2 32 1.000 32.0 25.4 35.6 43.4 3704 0
G1_LAT noisy_clustered 4096 neighbor 2 64 0.598 64.0 9.8 10.3 13.8 3704 0
G1_LAT noisy_clustered 4096 global_wfull 2 64 1.000 64.0 26.5 37.8 40.3 3704 0
G1_LAT noisy_clustered 4096 neighbor 2 128 0.762 128.0 12.8 14.4 20.6 3704 0
G1_LAT noisy_clustered 4096 global_wfull 2 128 1.000 128.0 28.6 43.9 69.0 3704 0
G1_LAT noisy_clustered 4096 neighbor 2 256 0.922 255.4 21.4 26.9 33.4 3704 0
G1_LAT noisy_clustered 4096 global_wfull 2 256 1.000 255.4 35.0 50.8 194.6 3704 0
G1_LAT uniform 4096 exhaustive 0 none 1.000 4096.0 130.0 171.6 226.0 4320 224
G1_LAT uniform 4096 neighbor 1 32 0.703 32.0 8.3 8.9 18.4 3972 0
G1_LAT uniform 4096 global_wfull 1 32 0.969 32.0 10.6 12.2 30.6 3972 0
G1_LAT uniform 4096 neighbor 1 64 0.805 64.0 9.6 10.4 20.1 3972 0
G1_LAT uniform 4096 global_wfull 1 64 0.969 64.0 11.3 15.2 22.5 3972 0
G1_LAT uniform 4096 neighbor 1 128 0.934 127.8 13.6 16.4 27.4 3972 0
G1_LAT uniform 4096 global_wfull 1 128 0.969 127.8 13.1 18.8 28.8 3972 0
G1_LAT uniform 4096 neighbor 1 256 0.969 144.1 14.1 15.5 23.1 3972 0
G1_LAT uniform 4096 global_wfull 1 256 0.969 144.1 12.5 14.0 20.3 3972 0
G1_LAT uniform 4096 neighbor 2 32 0.703 32.0 9.1 11.6 40.1 3972 0
G1_LAT uniform 4096 global_wfull 2 32 1.000 32.0 20.9 29.9 35.0 3972 0
G1_LAT uniform 4096 neighbor 2 64 0.805 64.0 10.4 11.0 14.6 3972 0
G1_LAT uniform 4096 global_wfull 2 64 1.000 64.0 20.9 26.5 39.9 3972 0
G1_LAT uniform 4096 neighbor 2 128 0.934 128.0 13.9 17.9 25.3 3972 0
G1_LAT uniform 4096 global_wfull 2 128 1.000 128.0 22.5 36.8 49.0 3972 0
G1_LAT uniform 4096 neighbor 2 256 0.973 256.0 22.1 28.8 34.2 3972 0
G1_LAT uniform 4096 global_wfull 2 256 1.000 256.0 26.0 32.5 72.5 3972 0
G1_LAT_PROCESS peak_rss_kib=3972
```
