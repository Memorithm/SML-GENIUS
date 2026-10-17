# RTLR-ELASTIC Boolean hybrid (experimental v4, Rust only)

## Architecture

This is a **new learned selection mechanism**, built on the measured RTLR-V3 and the independently qualified bounded Boolean ANF delay-line implementation. It is *not* proof of language-model quality or a replacement of the existing 50M MOR/ANF physical memory.

`RtlrBooleanHybrid` wraps a caller-constructed `RtlrElastic` instance (including the V3 multiscale constructor when applicable) and a bit-packed `AnfDelayLine` of explicit bounded capacity (e.g. **1024 bits = 128 backing bytes**). The online selector has up to 16 **predeclared, sorted unique** Boolean delay candidates. Each per-position prediction has:
- one nonlinear RTRL recurrent expert `sigmoid(4 * local_readout)`;
- one deterministic bit-line expert per candidate lag, with conservative probability floor `0.001` and causal feature `parity(older & newer)`;
- softmax-normalized nonnegative expert weights fit by multiplicative log-likelihood updates, **TRAIN targets only**.

At TRAIN time, `advance_train(older,newer)` reads only past bytes, creates a pending prediction, and `learn_target(y)` commits the currently known TRAIN target, updates the selector log scores, and scales existing recurrent eligibility credit using the recurrent expert's pre-update mixture weight. All delay weights/log scores remain frozen during `advance_inference`. Calling TRAIN forward twice without a target or mutating inference with a pending label is rejected. `reset_state` discards volatile recurrence and Boolean observations, *not* learned selector weights; a new document requires a causal burn-in if long lags are to be evaluated fairly.

The packed-delay experts are **not learned features**: the ANF function is given, and the candidate delays are fixed before training. Only the selector weights and recurrent model parameters are learned. This distinction is mandatory in any scientific comparison.

## Boundaries, tests, resource accounting

The selector validates finite `0 < eta <= 1`, capacity within `1..=1_048_576` bits, and 1–16 sorted unique lag candidates in range. Every selector uses `(1 + number_of_lags) * 8` bytes of floating-point score storage plus its bounded Boolean packed storage, the RTRL active per-cell payload and transient heap/vector metadata. `logical_payload_bytes` intentionally excludes system allocators, file-backed dormant banks, model runtime, buffers and process RSS; measure all these separately on physical Dell and Thor runs.

Unit tests cover:
- correct bounded/unique/sorted candidate validation and byte accounting;
- enforced TRAIN-forward/label separation, frozen DEV parameters, causal history reset;
- synthetic lag16 learning from prior-only paired input bytes, with held-out continuation and no inference selector updates.

## What remains unproven

The preceding measured [RTLR-V3 matched ANF panel](https://github.com/Memorithm/RemoteOps/blob/main/docs/SML_RTLR_V3_MATCHED_ANF_MEASURED_20261017.md) establishes a **2.56% smaller DEV logloss** at lag16 on one generated distribution (0.974432743 versus TRAIN-fitted constant 1.000007848); the baseline V3 has no established gain at lag1023. The [packed Boolean oracle](https://github.com/Memorithm/RemoteOps/blob/main/docs/SML_RTLR_V3_ANF_ORACLE_MEASURED_20261017.md) showed zero exact-retrieval mismatches at all four tested lags with 128 packed backing bytes; it is not a trained comparator.

Before saying the hybrid is superior: validate the hybrid's actual trained model against standalone V3, no-memory current-feature and oracle on the **same** TRAIN and independent DEV stream across at least three data-source seeds and three model initializations. Report true binary logloss plus classification error (oracle reports only exact mismatch), active cells, selector lag probabilities, bit-ring bytes and measured CPU kernel *and end-to-end* throughput. Distinguish the cost of supplying known lag candidates from learning them, and validate unknown/unlisted delays as a negative control. No claim for unknown long-range memory, 25B model training, NVIDIA GPU speedups, or SEC acceptance.

Rights remain **G0 NOT_ADMITTED** for SEC; frozen **2024 TEST UNOPENED**. The hybrid does not read these data and does not load the physical paged 50M MOR/ANF atlas. The old 100× benchmark CLI and its result schema are not changed.
