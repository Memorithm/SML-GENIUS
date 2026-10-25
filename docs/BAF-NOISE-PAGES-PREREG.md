# BAF-NOISE-PAGES — pre-registration

Status: **written and merged before any noisy or multi-page run.** No
experiment in this document has been executed at the time of commit. (The
earlier `BAF-SEARCH-COMPARE-PREREG.md` was written after its local run; this
document fixes that ordering.)

Scope: robustness of the existing BAF joint-focus fit on the
`baf_hardened_compare` harness under (a) TRAIN-only label noise and (b) more
editable pages. Method study at width 7 only; **no learnability claim at any
width**.

## Planned harness flags (implemented in a follow-up PR)

- `--label-noise P` (0 ≤ P ≤ 0.5, default 0): each TRAIN row's two writer
  target bits (state bits 2 and 3) are flipped independently with
  probability P, from a dedicated RNG stream seeded by (family, seed).
  VALIDATION, TEST and OOD labels stay clean. Default 0 must leave the CSV
  byte-identical to `main` (except wall-clock µs).
- `--editable-pages N` (N ∈ {2, 4}, default 2): with N = 4 the two inert
  pages (30, 40) become redundant writers of bits 2 and 3 respectively, all
  four pages are editable, and `max_selected_pages` = 4. Writes are
  idempotent OR, so the target remains exactly representable.
- Wider page state (`state_bits` > 4) is **out of scope**: the harness'
  targets, OOD split and writer layout are defined on 4 state bits, and
  changing them would change every family. Deferred, not tested here.

## Grid (fixed)

Width 7, seeds 0..7, `--max-degree 3 --parity4 --max-quad-flips 70
--max-evals 5000`, families conj2/xor2/parity3/parity4/and3/random,
search ∈ {kflip, beam (width 4), gf2}, noise ∈ {0, 0.05, 0.10, 0.20},
editable ∈ {2, 4}. 24 configurations, all run, all reported. Budgets are
identical across configurations; a configuration whose preflight exceeds
the budget fails closed (status 1) and is reported as such, not re-run
with a larger budget.

## Protocol

- Selection uses TRAIN; admission uses VALIDATION once (train < baseline
  and val ≤ baseline val), unchanged.
- TEST/OOD columns are produced by the harness but are not looked at or
  reported until all 24 configurations have finished; then all four splits
  are reported together.
- No seed selection, no reruns, negatives kept.

## Hypotheses

- H1 (noise, kflip): on structured families at the sufficient degree,
  mean clean VALIDATION/TEST error is non-decreasing in P, and stays below
  the inactive baseline at P = 0.05.
- H2 (noise, gf2): for P > 0 the GF(2) system is usually inconsistent, so
  gf2 mostly returns the baseline (not admitted) — an honest failure, not
  a degraded fit. Predicted: gf2 admits on parity families drop from 8/8 at
  P = 0 to ≤ 4/8 at P ≥ 0.10.
- H3 (pages): at P = 0, editable = 4 does not change train/val errors of
  kflip at the sufficient degree versus editable = 2, but raises mean
  evaluations (≈2× preflight); beam at degree 3 is expected to fail closed
  on budget.
- H4 (null): random stays null — no configuration achieves mean TEST or
  OOD error below the inactive baseline by more than seed noise.

## Metrics

Per configuration × family × arm: mean train/val/test/ood error bits,
admitted count of 8; per fit: evaluations, wall µs, status counts.
