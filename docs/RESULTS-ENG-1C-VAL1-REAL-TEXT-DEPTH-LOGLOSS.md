# ENG-1c/VAL1 — first independent FR/EN real-text Witten–Bell log-loss

**Observed 2026-10-13 UTC and reported without altering any candidate,
threshold or methodology after observing VAL.** This is a **negative
scientific finding** for the first TRAIN-occurrence-only adaptive depth
gate, not a software engineering failure. **TEST is UNREAD.**

## Scientific provenance: frozen before scoring

- ENG-1c/VAL1 preregistration
  [ENG-1C-VAL1-FIXED-EVIDENCE-DEPTH-PREREGISTRATION.md](ENG-1C-VAL1-FIXED-EVIDENCE-DEPTH-PREREGISTRATION.md)
  was committed as `5122a0357d5c64dd17826ab64f4a74e203058c6b`
  **before** any new VAL extraction or model-quality code.
- First exact scored source HEAD:
  `753436fbf0bf510701b8f939c9fe2ee37d9338e2`.
- GitHub-hosted Ubuntu 24.04 / pinned Rust 1.89
  [dedicated run #38367705326](https://github.com/Memorithm/SML-GENIUS/actions/runs/38367705326):
  FR job `115196715590` success and
  EN job `115196715682` success, exact
  fmt/check/warnings-as-errors Clippy, all-new 256-class
  Witten–Bell bitwise ladder oracles, independent source hashing,
  frozen data extraction, VAL scoring and manifest checks.
  [Full CI #38367705436](https://github.com/Memorithm/SML-GENIUS/actions/runs/38367705436)
  was also successful on this original source HEAD.
- Unmodified first-run metrics/source diagnostic artifacts:
  [FR #11797339346](https://github.com/Memorithm/SML-GENIUS/actions/runs/38367705326/artifacts/11797339346)
  (2,172 bytes, reported compressed size) and
  [EN #11797826479](https://github.com/Memorithm/SML-GENIUS/actions/runs/38367705326/artifacts/11797826479)
  (2,173 bytes). No corpus text, per-target labels or learned
  model files were uploaded as artifacts. Archive bytes have
  not been independently downloaded to rehash.
- Source raw and normalized LF-body SHA256 of Gutenberg
  FR #17989 (Dumas) and EN #1342 (Austen) were verified
  exactly against ENG-1c/0. TRAIN was fitted only once per
  language at max depth32; separate 10% VAL bytes were
  scored at **exactly identical 64-lane positions** for all
  seven frozen candidates. TRAIN counts never updated on
  VAL. The later 10% TEST bytes were not passed to any model,
  scored, inspected for performance or materialized to disk.

## Distinct TRAIN/VAL identities and validation denominators

| Corpus | Gutenberg | Exact TRAIN SHA256 | Independent VAL SHA256 | VAL bytes | Scored next-byte targets |
|---|---:|---|---|---:|---:|
| FR | 17989 | ec538157302ad0d39668288c8109779b70492a93041d432f7f97b2dc96e535cd | d3ddafca85d8bfe057503bdb88c450f8110644b51fabbc2600cdb634b2933593 | 74785 | 74688 |
| EN | 1342 | 43553238be6ec4a88e01a679a53381cd89aca8374d815e021e92317f71cb47d0 | f1a278d07c7d98a15f976d9ad7a673029a50d038b07d9d2508ffa1a4d41f882b | 73795 | 73728 |

Scoring resets history at the beginning of each of 64
contiguous VAL lanes (t>=1), uses 74,688 FR or 73,728
EN scored events and discards only the preregistered
trailing VAL residual bytes. There is no TRAIN→VAL
history carry or cross-lane lookahead.

## Exact predictive quality on full independent VAL

**Lower bits/byte is better. Accuracy counts exact 256-class
argmax decisions with earlier-byte tie breaking.**

| Predeclared candidate | FR bits/byte | FR accuracy | EN bits/byte | EN accuracy |
|---|---:|---:|---:|---:|
| FIXED_4 | 2,063037 | 0,5656 | 1,876565 | 0,6042 |
| FIXED_7 | 2,217821 | 0,6046 | 2,020676 | 0,6379 |
| FIXED_12 | 2,666834 | 0,5995 | 2,499990 | 0,6313 |
| FIXED_32 | 2,815443 | 0,5986 | 2,665924 | 0,6298 |
| MIN_COUNT_2 | 2,231304 | 0,6018 | 2,063521 | 0,6355 |
| MIN_COUNT_4 | 2,139950 | 0,5939 | 1,968163 | 0,6288 |
| MIN_COUNT_8 | 2,089345 | 0,5852 | 1,913076 | 0,6203 |

The four static depths [4,7,12,32] are evaluated from a
**single common source-owned depth32 Witten–Bell count
store**, and the observer's 256 probabilities agree bitwise
with separate direct fixed-depth calls. Each adaptive
policy chooses the deepest among [32,12,7] with at least
its fixed [2,4,8] EXACT TRAIN context occurrences, falling
back to depth4. No VAL labels or counts influence the
switch decision.

The major negative: FR and EN **both favour fixed
depth4 in proper log-loss**. Depth7 predicts a higher
fraction of exact byte argmax labels, but its likelihood
is worse. Deep fixed depth32 is even more poorly
calibrated on independent VAL. The use of sparse,
long, low-sample contexts is a plausible hypothesis
for further study, not a causal result of this experiment.

## Actual depth mix used by the three frozen adaptive gates

Counts are selected occurrences at full VAL target
positions, in frozen depth order **4 / 7 / 12 / 32**.

| Adaptive policy | FR depth selections | EN depth selections |
|---|---|---|
| MIN_COUNT_2 | 28607 / 37833 / 8244 / 4 | 25406 / 39262 / 9030 / 30 |
| MIN_COUNT_4 | 37742 / 32805 / 4141 / 0 | 34202 / 34676 / 4826 / 24 |
| MIN_COUNT_8 | 47520 / 25301 / 1867 / 0 | 43834 / 27449 / 2422 / 23 |

In both books, even a threshold of 8 activates
higher context depths for many positions despite
depth4 assigning better average probabilities.
The evidence count alone is insufficient to
justify these changes.

## Frozen decision and descriptive paired uncertainty

The preregistered adaptive advantage gate required,
in **both** languages, best count-gated model beating
best fixed depth by at least **0.010000 bits/byte**
and a positive lower 64-lane paired bootstrap bound.
The 2048 bootstrap draws use seed
`0xE1C5_2026_1013_B001`, with order
statistics 51 and 1996.

| Book | Best fixed bpb | Best gated policy | Gated bpb | Improvement (fixed minus gated) | Approx. 95% paired interval | Result |
|---|---:|---|---:|---:|---|---|
| FR | 2.063036593417 | MIN_COUNT_8 | 2.089344543258 | -0.026307949842 | [-0.037399255519, -0.015412828260] | **VAL_STOP_NOT_PROVEN** |
| EN | 1.876564702039 | MIN_COUNT_8 | 1.913076323341 | -0.036511621302 | [-0.045224685761, -0.028173175674] | **VAL_STOP_NOT_PROVEN** |

**FROZEN SCIENCE VERDICT = VAL_STOP_NOT_PROVEN for FR and EN**.
Both point estimates indicate worse loss for the best
gated profile, and both paired descriptive intervals
exclude zero in the wrong direction. These bootstrap
intervals are not independent hypothesis tests because
VAL also selected the best policy.

This negative is not a refutation of *all possible*
ElasticN-Gramme controllers. It refutes the
**particular, preregistered occurrence-count-only
threshold policies [2,4,8]** on these two validation
corpora, against the matched fixed-depth alternatives.

## Model-storage and physical resource trade-off

All seven experimental candidates share a
**depth32 paged store**, including the fixed-depth4
*readout* control. This ensures same training and
semantics but **does not mean a dynamic-depth
controller has the disk footprint of a fixed-depth4
store**. Independent CAP2b observed a standalone
depth4 store of 4,456,716 counted bytes FR or
4,488,446 counted bytes EN, versus hundreds of
megabytes when all depth32 contexts are retained.

| Book | Depth32 contexts | Distinct follower pairs | Counted store bytes | Sparse index bytes | Build time ms | Full VAL scoring time ms | Process VmHWM peak KiB |
|---|---:|---:|---:|---:|---:|---:|---:|
| FR | 14281533 | 14878280 | 565374640 | 2440998 | 16167 | 106456 | 15824 |
| EN | 13932625 | 14519708 | 551748904 | 2382156 | 15631 | 197397 | 15536 |

`VmHWM` measures the **process** peak, not file-cache
or storage-device RAM; the Linux 256-MiB virtual address
limit and CAP2b disk quotas were respected. The full
VAL scoring times include repeated page lookups and
all seven policy updates, not seven independent model
services or a production token latency benchmark.
No GPU/VRAM, energy, SSD wear, concurrency or hard
page-cache residency was measured.

## Next science gate — new evidence, not retuning this VAL

1. **Do not add more post-hoc thresholds on the same two
   now-exposed VAL losses and claim independent validation.**
   The previous VAL1 hypothesis is falsified as stated.
2. Either preregister, before code, a distinct
   TRAIN-only out-of-fold residual/error-aware gate
   or independent PPM/CTW comparator and then evaluate
   on **new, frozen, nonoverlapping FR/EN texts**, or
   preregister one confirmatory TEST experiment for
   the already selected fixed-depth4 candidate and
   frozen null comparators. Any future TEST requires
   its own methodology commit before exposure.
3. For an adaptive model to be worth its enlarged
   count-store footprint, compare improvement in
   **log-loss per physical memory + lookup budget**,
   not only argmax accuracy. A dynamic-depth switch
   can change semantics and needs SML-owned
   quality authorization.
4. ANF bank / Boolean composition is a distinct,
   currently **unverified** residual interaction
   hypothesis. It must beat a matched raw-bit/control
   baseline on fresh independent data; no ANF claim
   can be derived from VAL1.

No production promotion, no Tensor Core/attention
replacement proof, no live ElasticXxx actuation
or SML-25B predictive improvement is authorized.

Tracking:
- https://github.com/Memorithm/SML-GENIUS/issues/530
- https://github.com/Memorithm/ElasticXxx/issues/244
