# RTLR-ELASTIC v5: TRAIN-only discovery of previously unknown Boolean lags

## Prototype and scientific scope

The previous native [RTLR v4 paired evidence](https://github.com/Memorithm/RemoteOps/blob/main/docs/SML_RTLR_V4_PAIRED_HYBRID_MEASURED_20261017.md) achieved near-certain accuracy only for predeclared ANF feature lags 1/16/128/1023 and showed zero improvement at **unlisted lag64**. The new TrainLagDiscovery module scans *candidate delay indices that are not supplied with the target*, using supervised TRAIN labels exclusively, and returns a small validated list for downstream RTLR Boolean hybrid training.

The ANF source feature parity(older & newer) remains supplied. The label is revealed for TRAIN positions after the pair is observed. The algorithm does **not** invent new Boolean features, and does not infer a delay from unlabeled signals. It is supervised delay selection.

## Mechanism

The dynamic bit-packed causal feature ring has capacity 1..4096 positions, using ceil(K/64)*8 backing bytes. The TRAIN-only evidence bank has exactly K dynamically allocated four-u64 vote records (32K bytes), then at most 16 candidate delays survive. Every configurable sample_stride TRAIN targets, the program scores all currently observed feature lags: a correct/incorrect Boolean match. Alternating sampled positions go to two **disjoint TRAIN** partitions, discovery and confirmation; they are not assumed statistically independent in time.

For both partitions, the empirical match frequency must exceed 0.5 plus an exploratory Bonferroni/Hoeffding margin sqrt(ln(2K/alpha)/(2n)), with at least minimum_partition_samples observations each. This familywise bound is only a conservative screening rule under correlated/adaptively chosen stream positions, **not a proof of independently distributed false-positive control**. Ties select shorter delays deterministically; truncate to retain<=16 candidates. A failed gate returns no candidate; do not silently fall back to a DEV-chosen delay.

All updates require observe_train_pair immediately followed by learn_train_target; no repeated label, future-byte read or mutation after freeze. reset_history preserves accumulated TRAIN evidence but clears past features; prime_pair builds a causal pre-document halo without scoring any label. Neither reset nor prime can be used with an uncommitted target.

## Experimental parameters and pre-registered tests

Primary panel: 1,024-bit ring (128 bytes), 1,024 * 32 = 32,768 bytes of vote records, 32-position sampling stride, retain<=3, min 64 sampled TRAIN positions per partition, alpha=0.01. Logical ring+vote payload: **32,896 bytes**; excludes Vec headers, allocator metadata, CPU caches, models, and stream buffers.

Initial deterministic unit tests:
- Recover *unlisted* lag64 and lag1023 independently from TRAIN-only synthetic streams after causal halo; both partitions exceed 99% correct.
- Reject invalid capacities, unsorted/duplicate sample states, invalid confidence levels, uncommitted targets, learning after freeze and double-freeze.
- Reject unrelated target stream (conservative false-candidate control) rather than always selecting a delay.

The first PR provides the **discovery primitive**, not an integrated v5 language model nor a native speedup claim. Native Thor and Dell require tests on the exact commit before any merge. Next integrate two-stage TRAIN: discovery on TRAIN-only subset, freeze, instantiate existing RtlrBooleanHybrid with learned candidate list sorted by lag, train selector/recurrence on a **nonoverlapping TRAIN-only subset**, evaluate immutable parameters on independent DEV across multiple input seeds. Pair against v3, v4 predeclared and Boolean-only discovery/selector controls; account for all candidate-scan time and 32KiB vote bank rather than quoting the 128-byte inference bit ring in isolation.

The old 100x, v3 and v4 benchmark CLIs are untouched. No SEC corpus may be transferred (G0 NOT_ADMITTED, frozen 2024 TEST UNOPENED), and no physical 50M MOR, SML25B or GPU result is implied.

Applicable conceptual benefit to KVLab/ElasticXxx/SciRust remains **prospective**, not implemented in those repositories.
