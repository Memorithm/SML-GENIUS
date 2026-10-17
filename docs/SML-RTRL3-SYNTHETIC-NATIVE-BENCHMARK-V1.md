# RTRL-3 standalone native Dell benchmark — pre-registration, synthetic v1

The merged RTRL-3 kernel (`sml-core::rtrl3_elastic`, [PR #599](https://github.com/Memorithm/SML-GENIUS/pull/599)) has passed its numerical and CI tests, but this by itself **does not measure its predictive performance, throughput or real RAM consumption**. This new standalone benchmark measures an explicit, narrower set of quantities before any integration into the physical 50M MOR atlas.

## Deterministic data and independent repeats

The CLI `sml-rtrl3-bench --synthetic-v1` constructs a **synthetic**, independently seeded train stream of 16,386 octets (16,384 predicted training bytes) and evaluation stream of 8,194 octets (8,192 predicted holdout bytes). For the first 8 bytes, generate seeded pseudorandom bytes; subsequently predict a byte that is normally either 0xAA or 0x55 according to parity of the immediately previous and seven-step-old bytes, with 1/32 seeded random exceptions. This is a toy causal temporal task, **not natural language, not SEC**. Both exact SHA-256 stream digests are printed and must be identical across independent runs. No previously used DEV or 2024 TEST is touched.

Run 3 independent UORO sketch seeds (11, 23, 37) for 5 configurations, 15 separate train-and-score runs:

| H hidden | Fixed inter-state rank M | UORO sensitivity rank K | Exact vs approximate |
|---:|---:|---:|---|
| 1 | 0 | 1 | Analytic exact scalar trace |
| 8 | 2 | 2 | Stochastic rank-2 |
| 16 | 2 | 4 | Stochastic rank-4 |
| 32 | 4 | 8 | Stochastic rank-8 |
| 64 | 4 | 8 | Stochastic rank-8 |

Fixed hyperparameters for all arms are recurrent and readout learning rate 0.005 and recurrent L2 clip threshold 1.0, with `train_document` once over the same bytes and unmodified `evaluate_document` over independent holdout bytes. Report: train/holdout bits per byte, effective training/evaluation predicted bytes per second on native Dell, model-side wall-clock seconds, f64 **payload byte count** (not RSS), trainable recurrent coefficients, per-arm actual clipped steps, maximum preclip gradient norm, immutable stream SHA and each sketch seed.

No adaptive hyperparameter selection on the holdout. Cross-H capacity increases and neural architectures differ, so performance differences cannot establish an isolated sensitivity rank effect. Single-machine per-run wall times are exploratory; compare repeated runs to understand variance, but do not make statistical claims from just three sketch seeds.

## Limits

The standalone RTRL-3 model uses its own 8-bit logistic head and trainable 3H recurrent local coefficients, **not** the 50,000,896-slot physical paged MOR expert model of RTRL-2. Therefore its bits/byte and throughput **are not apples-to-apples comparable** to the prior Dell RTRL-2 result 3.616609952 bits/byte on the SEC source. The pilot measures algorithmic functionality and native efficiency on this synthetic task, not end-to-end SML-GENIUS performance.

Memory accounting from `resident_payload_bytes` only counts persistent f64 slices and excludes allocator, Vec headers, temporary buffers, stack and process memory. The Dell workflow should additionally capture `/usr/bin/time -v` or `VmHWM` for a process-level peak and CPU architecture; absent this, don't claim actual RSS.

Preserve research governance `G0 NOT_ADMITTED` for SEC (not used in this benchmark) and 2024 TEST `UNOPENED`. Do not infer a trained 25B model, 50M atlas learning, multilingual reasoning, or any production-ready throughput from synthetic measurements.
