# RTLR-V3 D-ANF matched feature delay and TRAIN-prior baselines

## Motivation

In the measured native [144-trial RTLR causal diagnostic](https://github.com/Memorithm/RemoteOps/blob/main/docs/SML_RTLR_V3_TEMPORAL_DIAGNOSTIC_MEASURED_20261017.md), `anf2` achieves **0.030195219 bit/position** on generated DEV (same arithmetic mean of three seeds on two native hosts). Four delayed raw-byte parity tasks and their XOR remain **worse than 1 bit/position**. The old lag tasks do not share the immediately learned ANF feature, so they do not isolate memory from representation.

## New matched causal task family

The existing `anf2` label equals `parity(bytes[i-2] & bytes[i-1])`, exactly the signed `anf(a,b)` feature already supplied to each recurrent cell. The new tasks `anf_lag16`, `anf_lag128` and `anf_lag1023` use the **same ANF function** at respectively earlier paired bytes:

`label_L(i) = parity(bytes[i-L-1] & bytes[i-L])`.

Here `L=1` is the existing `anf2` positive control, because its latest input is at `i-1`. `L=1023` is the largest matched pair fully included in the unchanged 1024-byte lookbehind. Unit tests check direct ANF parity identity, exact indexing, streaming chunk seams, and deterministic source parity. The 100× four-argument historical CLI/TSV schema remains unchanged. This is **not** a larger neural model.

The existing 7-argument interface accepts these three additional TASK values. The diagnostic's `RTLR_TEMPORAL_DIAG_V1` 20-column result remains unchanged. Each diagnostic execution additionally writes a `RTLR_TRAIN_PRIOR_V1` line with task/mode/seed, supervised sample counts, **TRAIN-only** one-class count, DEV one-class count, frozen TRAIN positive frequency, and **DEV binary logloss of the frozen TRAIN-fitted constant**. DEV labels are never used to choose the constant probability. The constant `p=0.5` continues to be exactly 1 bit/position as a second reference. A train-fitted majority-class predictor is generally a stronger null whenever the source label distribution is imbalanced.

## Preregistered discrimination

- If immediate `anf2` is good but `anf_lag16` fails versus the TRAIN-only constant baseline, this supports a temporal *propagation or readout* limitation in the current recurrence, not an immediate ANF input-feature failure.
- Compare `anf_lag128` and `anf_lag1023` at identical synthetic TRAIN/DEV exposures on Dell and Thor CPU, same model seeds and source SHA. This does not by itself localize whether the limiting factor is leak, recurrent credit gradient, width scheduler or readout.
- Distinguish absolute improvement over constant null from smaller loss **relative to the scalar**. Only improvements below the stronger TRAIN-fitted constant should be considered useful prediction.
- Do **not** infer performance on natural language, the physical 50M paged MOR/ANF experts, or 25B SML. No GPU tests, no SEC data transfer. Rights **G0 NOT_ADMITTED** and frozen **2024 TEST UNOPENED**.

A future explicit Boolean delay-line oracle can then measure the attainable matched task accuracy and storage/latency cost under a separate, audited design.
