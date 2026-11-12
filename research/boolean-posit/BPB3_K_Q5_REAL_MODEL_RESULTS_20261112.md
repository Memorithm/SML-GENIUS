# BPB3-K Q5-178 — first real full-model FP16-source evaluation (2026-11-12)

**Status:** successfully executed experimental CPU inference, **no release or production promotion**. This is one GitHub-hosted x86-64 VM with 2 CPU threads, not a measured Dell T430/NUMA result. Source weights are pretrained **F16**, not recycled Q3_K or Q4-146 integers.

## Source, code, evidence

- Experimental inference source: [SML-GENIUS PR #954](https://github.com/Memorithm/SML-GENIUS/pull/954), head `6179d2997ccf2be1abf927464a86884100b951b3` at qualification.
- [Successful exact-head GitHub Actions run 43126478915](https://github.com/Memorithm/SML-GENIUS/actions/runs/43126478915), retained evidence ZIP [artifact 13903334502](https://github.com/Memorithm/SML-GENIUS/actions/runs/43126478915/artifacts/13903334502) (logs only, **no GGUF models or raw vocabulary dumps**).
- Source pretrained `SmolLM2-135M-Instruct-f16.gguf`, model repo `bartowski/SmolLM2-135M-Instruct-GGUF`, pinned revision `c6f2d396a5faa22c9722908d47e8da81a5d8cebc`. Verified SHA-256 `f535f83ec568d040f88ddc04a199fa6da90923bbb41d4dcaed02caa924d6ef57`.
- Experimental sentinel-Q5 SHA-256 `7b152ae92f7a85fa4ebea011b9ea9746f4a1f6defd3e7f9b704d3c4af3419003`. Temporary matching-patched-only surrogate SHA-256 `fbf46f10a1f8e648cf726a28b4b5bb4196fe6d7394360afa32fb360af86fb51e`.
- Upstream `llama.cpp` at `f7591a6a01f9273de03331732bdc0b39721168be`; untouched stock and separate pinned-patched copies, CPU-only. Stock debugger binary SHA-256 `6e76f6854360f98253f0a53c8469b2df64dd5c1e3519da7759569023bc4e0526` and patched debugger binary `f0948be4dc8987ba42149956e4553b5eb59d33baafcbde7e2fb22c332cf0fd01`.
- Actual converter rewrote **30 F16 matrices, 103,680 blocks of 256 coefficients**, to 178-byte Q5 blocks. 20.02 seconds wall-clock, 18.59 seconds CPU user time and maximum converter RSS 2,328 KiB in `conversion-resources.txt`. This is an observed hosted-VM conversion time, not a general cost guarantee.
- Both Rust AVX2 `BPB3K_Q5_178_REAL_DOT_EXECUTED` and direct fifth-bit `BPB3K_Q5_178_REAL_TILED_EXECUTED` were observed in actual full-model inference. No native-Q3 weight unpack was used on Q5 weights.
- Model bytes reported by `llama-bench`: **256.63 MiB** unmodified source F16, **223.61 MiB** experimental Q5-surrogate (**12.87% lower nominal model size**). The quantized payload is not a standard Q5_K model.

## Full-vocabulary and token observations

The full-vocabulary observer compared the **49,152 logits** for each of **8** greedy generation steps on each of two fixed prompts, so 16 step-pairs and **786,432** paired logit values. All sixteen greedy token IDs matched their F16 control on these traces. Each step was compared only while prior token contexts were identical. This is **not proof of all-prompt fidelity**. Representative discrepancy extrema:

| Prompt | Largest observed step max-absolute logit difference | Max observed step RMSE |
| --- | ---: | ---: |
| Short arithmetic | 3.284566879 | 0.907786488 |
| Longer sequence prefill | 3.016172886 | 1.395452126 |

These nonzero differences are too large to call the logits bit-exact. Later qualification must measure task quality and held-out perplexity from the same original F16 model without conflating unrelated Q3_K controls.

## Four alternating full-model paired CPU pilot sessions

Exact command for each arm, with matching CPU-only conditions: `llama-bench -ngl 0 -t 2 -p 64 -n 32 -r 2`. Pair order alternates stock/Q5, then Q5/stock; metric columns below come from raw `bench-N-ARM.txt` and `resource-N-ARM.txt`. **t/s higher is better**.

| Pair | F16 pp64 t/s | Q5 pp64 t/s | F16 tg32 t/s | Q5 tg32 t/s | F16 peak RSS KiB | Q5 peak RSS KiB |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 939.02 | 764.75 | 149.36 | 159.47 | 303,796 | 271,420 |
| 2 | 923.97 | 788.08 | 152.30 | 159.71 | 303,708 | 271,412 |
| 3 | 911.59 | 787.49 | 150.26 | 159.45 | 304,016 | 271,508 |
| 4 | 943.79 | 788.54 | 150.72 | 159.55 | 304,108 | 271,484 |

Report the **median of the four within-pair Q5/F16 ratios**, *not* the ratio of the two arm medians:

| Metric | Median Q5/F16 ratio | Observed effect |
| --- | ---: | ---: |
| Prompt pp64 | **0.844216** | **−15.58% throughput** |
| Decode tg32 | **1.059873** | **+5.99% throughput** |
| Peak RSS | **0.893250** | **−10.68% peak RSS** |

All four paired pp64 comparisons favor native F16; all four paired tg32 comparisons favor the experimental Q5; all four observed process RSS measurements favor Q5. These effects **cannot be extrapolated** from four pairs and a single GitHub-hosted CPU, and the F16 baseline is **not the standard native Q5_K** baseline. There is no evidence here that Q5-178 is better than native Q5_K or Q4-146 on the same F16 source.

## Next gates / qualification policy

1. Add a standard **Q5_K quantization and original-weights-controlled baseline** on the same pinned F16 source, including all-model quality, pp64, tg32, RSS and quantization time. Do not compare independently sourced Q3_K and Q5 models as if their weights matched.
2. Perform a pre-registered larger paired comparison with per-pair host-contamination gating, confidence intervals and both prompt and decode curves; retain negative results and avoid unsupported superiority claims.
3. Run true physical CPU/NUMA/bandwidth work on the Dell only after **Memorithm/RemoteOps** admission and non-disruption checks. Do not install the experimental Q5 runtime into production or general GGUF loaders.
4. Test on held-out text/tasks and more than one model architecture before any promotion.

**Decision:** a real pretrained-model Q5-178 tiled execution is now demonstrated. Single-host decode speed and memory show preliminary gains versus *F16*, while prefill is slower. Industrial or native-Q5_K superiority remains **unproven**.
