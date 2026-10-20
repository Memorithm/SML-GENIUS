# F2b-4c — Causal first-order Markov null baseline (preregistered before code)

Date: 2026-10-20. The real spectral engine / frozen TRAIN majority-prior F2b-4b qualification is complete (source SML #641; native RemoteOps #631 and report #632). These successes on balanced and 20%-positive IID null data do not qualify autocorrelated null targets.

## Failure case and testable hypothesis

For an input-independent stationary Markov bit stream with P(Y_t = Y_(t-1)) = 0.9 and class prevalence near 0.5, a constant predictor yields ~50% bit error while a causal last-label predictor yields ~10%. A spectral gate should not claim useful discovery if it merely detects this null target dependence without improving a frozen, equally informed Markov baseline.

## New oracle F2b-4c-1

Add a standalone Rust FrozenBitMarkovPrior, trained exclusively from successive 64-lane true target words in TRAIN_fit. For each previous-label state 0 or 1, learn the majority of the immediately following label over all valid per-lane TRAIN transitions. Tie predicts 0; if a state has zero observed transitions, fall back to the TRAIN global-majority bit. Freeze before any calibration or DEV. Evaluating a separate document may inspect its past true labels at time t-1, never Y_t before predicting time t. No transitions across document or TRAIN/DEV boundaries; skip the first target word in each independent document. Bitmask implementation over u64 with checked counters and no exponential allocation. Source labels of teacher functions do not enter the baseline.

Unit tests: persistent fair Markov, alternating stream, imbalanced independent stream, tie, never-observed previous state, invalid length, deterministic replay, scalar-lane reference equality, and document boundary reset. No mutation after fit and no changes to existing F2b-1/2/3/4 commands or evidence.

## Next empirical F2b-4c-2

Follow with an explicitly separately preregistered native experiment comparing W3/Hybrid fitted models against BOTH a frozen TRAIN-fit constant prior and a frozen TRAIN-fit first-order Markov prior. Require a fixed absolute improvement of >=10000 ppm in each TRAIN_cal partition over BOTH baselines, in addition to existing max error/cost and W3 vs Hybrid advantage. Null cases: fair and 20%-positive IID, balanced stationary Markov with 10% flip, skewed two-state Markov, plus noisy and clean Boolean teachers. Fresh independent seeds; no use of prior F2b-4b DEV to adjust thresholds; n32/n508 W3 workload cap still applies. Record complete training, calibration and evaluation CPU, error differences and abstentions. The exact future seed schedule must be frozen before testing.

Important inference limitation: a teacher-forced Markov predictor uses previous observed true labels, which might not be available under autonomous rollout. It is a **stronger diagnostic baseline**, not an equivalent deployment contract. A later autonomous-matched comparison is required before any runtime admission.

No SEC (G0 NOT_ADMITTED), frozen 2024 TEST UNOPENED, no LPN general theorem, GPU/NVIDIA toolkit or 25B-trained-model claim.
