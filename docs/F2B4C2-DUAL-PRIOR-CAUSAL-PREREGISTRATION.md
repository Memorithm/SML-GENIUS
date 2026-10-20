# F2b-4c-2 — dual frozen-prior causal qualification (prospective protocol)

**Frozen on 2026-10-20 before F2b-4c-2 code or new measurements.** Parent F2b-4c-1: SML #642. Keep F2b-1/2/3/4 CLI, TSV records, fitted weights, earlier seeds and published negative findings unchanged. This protocol defines a **new opt-in** diagnostic, not runtime admission.

## Question and falsifiable null

A constant TRAIN-fit label prior cannot control for input-independent serial correlation. A first-order Markov target with P(Y_t=Y_(t-1)) = 0.9 and stationary prevalence 0.5 offers approximately 10% error to a last-observed-label predictor even when the proposed input sources are independent noise. Any W3/Hybrid method must outperform **both** the TRAIN-fit frozen constant prior and the TRAIN-fit frozen causal Markov prior on the **identical scored target suffix of each disjoint TRAIN_cal document**. An apparent advantage over the constant prior alone is a diagnostic failure.

## Locked synthetic design, pairings and independent streams

New CLI flag only: `--f2b4c2 xor|mixed|null|null20|markov50|markov20 0|40 127|131|137|139 32|508`. `xor` and `mixed` allow independent Bernoulli label noise 0% and 40%; all four null families are noiseless *with respect to their generated target process* and reject an additional noise argument other than 0. Total **64 trials per host**: (2 Boolean tasks × 2 noise rates + 4 independent-of-input target families) × 4 fresh seeds × 2 source grids, with exactly the same labels for paired methods/prior baselines. n32 and n508 under a seed are not independent trials. Dell and Thor are execution replicas, not distinct datasets.

- `null`: independently generated fair Bernoulli(0.5) target bits; `null20`: independently generated Bernoulli(0.2) target bits.
- `markov50`: each bit lane is an independent stationary two-state Markov chain, P(1|0)=0.10 and P(0|1)=0.10; stationary prevalence 0.5.
- `markov20`: independent lanes with P(1|0)=0.05 and P(0|1)=0.20; stationary prevalence 0.2. Initial per-lane state is sampled from the respective stationary distribution for *each independent document*.
- The four null target generators MUST NOT consult Boolean source inputs, the teacher, the fitted circuit, or DEV outcomes. Seed-index and domain-separated RNG implementation for the new mode must be committed and reported; never share RNG state between source and target generation or across FIT, internal validation, CAL1, CAL2, DEV.
- All models fit on new TRAIN_fit only (128 useful words for clean and null; 512 for 40%-noisy teacher tasks; 128-step input halo). The Markov oracle receives only TRAIN_fit truth, no transitions across independent documents or phases. Constant prior uses exactly those TRAIN_fit label counts. Both remain frozen for CAL and DEV.
- Fit/validate through existing real C12h proposers, fitter and committed-network simulator, under the existing n32/n508 search bounds. If n508 W3 exceeds one million candidate triplets, **do not run** W3; represent it as absent, not a zero-error arm.
- CAL1 and CAL2 each contain **641** target words (one context word + 640 scored transitions), enabling **40,960 paired bit targets** per partition. The Markov score excludes t=0 because it has no previous true label within the document. The other three scorers MUST exclude the same t=0 label, while still processing the first input word for any model state. DEV contains **4097** words (4096 scored steps), is generated only after route freeze, and receives no fitting or route changes. Never concatenate documents to recover the lost first transition.
- The frozen Markov diagnostic is **teacher-forced**: t-1 truth can be read for a t prediction. This is stronger than an autonomous predictor and **not an equivalent deployment comparator**.

## Frozen TRAIN-only gate and exact statistics

A separate `TrainOnlySpectralDualPriorGate` (do not alter F2b-4a's gate) accepts two distinct, paired calibration partitions. Each observation contains the exact same positive-length target count, constant prior errors, Markov prior errors, Hybrid errors and, when available, W3 errors; measured search+fit+CAL wall-clock allocation per candidate must not be free. All errors are bit counts on the **same 40,960 scored labels**, not time steps, projected labels or source-match metrics.

On **each** partition independently require:

1. At least 40,960 paired bits and candidate loss <= 490,000 ppm (49%).
2. **Absolute candidate error reduction >= 10,000 ppm against BOTH frozen priors**. An arm that beats the constant prior but ties or loses to the Markov prior abstains.
3. Amortized Hybrid search+fit+CAL <= 500,000 ns per scored bit; W3 <= 1,000,000 ns per scored bit. Retain F2b4b two-partition cost charging.
4. To select W3 it must also beat Hybrid by >= 10,000 ppm in both partitions. Otherwise prefer an eligible Hybrid, else `Abstain`. Absent W3 is never eligible.

Checked u64 accumulators and u128 cross multiplication; reject invalid samples/partition, out-of-range losses, zero candidate CPU, availability mismatches, overflow or second freeze atomically. Route freeze occurs **before** any DEV materialization. Never interpolate missing Markov scores. The caller is responsible for authenticating target pairing/split provenance; the counter gate cannot prove it.

## Output, qualification and STOP

Create a **new versioned** report `F2B_ROUTE_V5`, keeping the 35 fields of V4 in their original order and appending enough explicitly named fields for CAL1/CAL2/DEV Markov errors, scored-bit counts, scorer availability, threshold and route reason; never relabel old V4 as dual-prior evidence. Include source SHA, host architecture, all phase RNG identifiers, full scan/search/fitting/CAL/DEV costs, both shadow-arm DEV losses, exact route/abstention and artifact SHA256. Validate the final schema length in tests before publishing it.

Engineering acceptance: Rustfmt, Clippy `-D warnings`, targeted and workspace Rust tests, locked deterministic replay, 64/64 V5 rows on each independently attested Dell and Thor source SHA, zero unauthorized W3 at n508, complete paired suffix counts, no t=0 leakage and route/shadow consistency. A failed scientific condition must be retained, not made a CI failure or hidden by filtering.

Provisional synthetic scientific acceptance: **zero promotions** in each of the four input-independent null families, and at least one new noisy XOR3 n32 case selecting W3 with calibrated error below both priors; show all abstentions and false positives even when the hoped-for positive condition fails. This is not a population false-discovery bound: adaptive search, correlated samples and replicated hosts invalidate treating all rows as independent. The 2024 TEST remains UNOPENED, SEC G0 NOT_ADMITTED, no external-data rights claim, no native GPU/NVIDIA toolkit requirement, no autonomous runtime admission, and no evidence of a trained 25B model.

## Implementation sequence

1. Commit this protocol separately before code.
2. Implement the independent **counter gate** and adversarial unit controls without changing F2b4 pathways.
3. Add the new native mode and causal paired scorer; then add RemoteOps Dell/Thor reproducibility workflows and evidence ingestion.
4. Only after exact-head CI and prospective native runs, publish losses and decide STOP/CONTINUE without modifying this frozen protocol.
