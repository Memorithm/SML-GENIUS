# HYB COPY-G — Gaussian copy-mixing competitor (pre-registration)

**Scope:** stacked on COPY-1 (the existing frozen H-1d + causal recopy expert). This is a **new research hypothesis**, not a revised interpretation of existing COPY-1/H-3 measurements. Freeze the protocol **before** inspecting CAL-SEL or any new out-of-distribution panel. Owner requested direct competition against the 20%/80% mixture on 2026-10-25.

## Arms and question

- **N**: P1 + the original frozen H-1d neural head, no copy.
- **F20**: same P1 and H-1d; use COPY-1 with exactly 20% copy / 80% neural *whenever copy is applicable*, otherwise 100% neural.
- **G**: identical P1, H-1d and COPY-1 suffix/candidate search, except that the mixture weight is a **Gaussian kernel over the longest suffix match length** (1–4), with a single amplitude calibrated **without code-loss labels on TRAIN only**. No new neural weights, rerouting or additional memory.
- **C0** (diagnostic): P1 + copy without H-1d (if implemented; it does not substitute for N/F20/G).
Both seeds 11 and 23 must be evaluated, with the **same Gaussian profile** for both; no retuning per seed.

**Not a claim about the normal distribution:** 80/20 is a Pareto-style *heuristic*, not a theorem of Gaussian statistics. Here "Gaussian" specifically means a bell-shaped, causal confidence kernel; an apparent gain must survive fixed baselines, equalized mixture exposure, a sealed sample and full lossless-coder checks.

## Frozen mathematical policy and integer realization

Let `L` be COPY-1's longest previously observed matching suffix, `L ∈ {1,2,3,4}`. If the historical copy candidate set is empty after the already-decoded current-byte bit prefix, both F20 and G use N unchanged. For a usable copy proposal:

`K(L) = exp(-(4-L)^2 / (2 × 1.5^2))`.

The deterministic, precomputed **Q1024** table (no floating point in encoder/decoder) is:
`K[0..4] = [0,139,421,820,1024]`.

TRAIN alone sets one amplitude `A ∈ [0,10240]` to minimize the unsigned deviation between effective weighted copy exposure of G and F20. The integer copy weight for match length `L` is `w_G(L) = round(K[L] × A / 1024)`, clipped to 10240, with nearest-integer half-up rounding. Weights therefore range 0–100% and are **monotone increasing** with match length. `w_F20=2048` exactly (20%, denominator 10240).

Collect `n_L` as the number of TRAIN **bit decisions** in which COPY-1 has a usable candidate at length `L`, excluding the two P1-uncoded prefix bytes per document. No target-byte outcome is scored; prefix availability is computed while consuming TRAIN bytes sequentially. Select the **smallest** `A` that minimizes `|Σ n_L × (w_G(L) − 2048)|`. Require `Σ n_L > 0` and absolute discrepancy ≤ `0.5%` of full-scale mixture exposure (i.e. ≤51.2 Q10240 units per active bit); otherwise `NOT_COMPARABLE` and no evaluation admission.

Applied per bit:
`P_G = round(((10240-w_G(L)) × P_H1d + w_G(L) × P_COPY) / 10240)`.
F20 retains its original integer-tenths mixing implementation, not a rewritten approximation. The chosen table of five weights is persisted/reported before opening CAL-SEL; no online adaptation or CAL-SEL search.

## Fairness and validation rules

- The same 256-byte ring, suffix-length maximum 4, candidate search, prefix filter, Laplace smoothing, frozen P1/H-1d, arithmetic coder, document ordering and integer probabilities on all arms.
- All arms run from independent, reset-per-document predictors. G does no future-byte lookup. Oracle recomputes from `data[..position]` without reusing ring state. For each arm: engine vs oracle probability parity on every bit and exact encode/decode byte-for-byte round trips.
- TRAIN data is disjoint by document from CAL-SEL and the sealed test panel. Calibrate G amplitude **once** on TRAIN. Record calibration counts, kernel, amplitude, achieved effective mix and any discrepancy. Reject if TRAIN and validation documents overlap.
- Before new held-out data, compare F20 and G against N on CAL-SEL. To advance, G must improve F20 container BPC by at least **0.005** in **both seeds**, keep full oracle/round-trip parity, and achieve end-to-end compression and decompression ≤1.5× F20 on matched hardware. Otherwise `NO_GO_CAL`. No sigma, window, kernel, smoothing or threshold changes after looking at CAL-SEL.
- If CAL qualifies, evaluate once on a *new* source-pinned, licensed and disjoint TEST corpus, not previously consumed EVAL3/EVAL5. Final `GO`: both seeds G − F20 ≤ −0.005 container BPC, same exactness, no regression in N comparison beyond 0.001 BPC, and matched end-to-end cost G/F20 ≤1.5. All unfavorable rows must be reported.
- Record total container bytes, BPC, mean copy mixture by active bit, counts per matched length, copy applicability rate, encoder/decoder wall times, peak RSS, source/asset SHA, complete traces' SHA, CPU ISA and compiler. Synthetic smoke does **not** qualify as TEST or prove superior generalization.
- No production default/promotion and no changes to FLAT-ATTENTION or SciRust are authorized by this experiment.

