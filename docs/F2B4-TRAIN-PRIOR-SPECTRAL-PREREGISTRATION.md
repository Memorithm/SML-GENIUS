# F2b-4a — TRAIN-fitted prior baseline for conditional spectral admission

**Preregistered 2026-10-20 before implementation and before any F2b-4 experiment.** This is an **engineering policy and synthetic unit-test tranche**, not a new native/evaluation result. It follows F2b-2 null false admissions (RemoteOps #626) and the narrower F2b-3 balanced IID-null control (RemoteOps #630).

## New failure mode

For IID labels P(Y=1)=0.2 independent of inputs, a constant zero classifier obtains **20% error**. Requiring only <49% error would therefore permit a non-causal representation with no useful input signal. A spectrally proposed model must also beat a **TRAIN_fit-frozen constant-label prior** on each TRAIN_cal partition. That is not a cure for correlated time-series targets.

## Frozen API and gate

- A FrozenConstantBitPrior uses only (train_ones,train_examples) from TRAIN_fit, predicts 1 if 2*ones>examples else 0 (tie=>0), never refits from CAL or DEV. It scores CAL from the observed label-one count without changing its prior, with u128 for exact products.
- A TrainOnlySpectralBaselineGate accepts paired target samples, constant prior errors, Hybrid errors, W3 errors and amortized search+calibration CPU nanoseconds on each of two disjoint supervised TRAIN_cal partitions. Caller must verify provenance and paired target identity.
- Per partition at least **40960 bits**. Qualify each engine only when its error <= **490000 ppm**, its *absolute* reduction against TRAIN-fitted constant prior >= **10000 ppm** in both partitions, and cost <= Hybrid **500000 ns per target** or W3 **1000000 ns per target**. Walsh3 additionally beats Hybrid by >= **10000 ppm** on each partition, otherwise prefer admissible Hybrid. If neither qualifies => Abstain. Freeze once before DEV.
- Reject malformed config, zero samples, error counts > samples, unknown partition, zero elapsed CPU, overflows, repeated freeze and observations after freeze with no partial state mutation. Use checked integers/u128 for margins. Do **not** alter historical TrainOnlySpectralMethodGate or F2b-1/2/3 TSV and CLI.

## Predeclared synthetic unit controls

1. Imbalanced IID P(Y=1)=0.2: frozen prior predicts 0 at 20% error; candidate also 20% => **Abstain**, despite satisfying old 49% cap.
2. Balanced IID P(Y=1)=0.5: tiny chance improvement below 1% => **Abstain**.
3. Real 40%-noisy XOR3: prior ~50%, Walsh3 ~40%, Hybrid ~50%, W3 within budget => **Walsh3Exhaustive**.
4. Imbalanced **predictive** condition: Hybrid improves a 20% baseline significantly on both partitions, W3 not stronger => **HybridW2SketchMitm**.
5. W3 too expensive, absent partition, bad config, counter overflow, freeze immutability => predictable fallback or safe rejection.

## Future empirical F2b-4b: separate preregistered campaign

Evaluate new independent seeds for balanced IID, imbalanced IID (20/80), time-correlated stationary Markov labels (also require a frozen causal last-label baseline), clean/noisy Boolean teacher functions and prior shifts; report complete discovery/calibration and end-to-end CPU/RSS, error-rate gains against frozen constant, last-label and raw baselines, IID false admissions and selective abstentions on DEV. This module alone **does not handle Markov-null dependence**. Do not use calibration to refit prior, do not weaken threshold after results.

SEC G0 NOT_ADMITTED, 2024 frozen TEST UNOPENED; no GPU/NVIDIA SDK, external-language or 25B-trained-model quality claims.
