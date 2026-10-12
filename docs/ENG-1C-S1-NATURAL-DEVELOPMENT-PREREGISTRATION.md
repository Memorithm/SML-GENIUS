# ENG-1c/S1 — fixed-budget natural-text development smoke (preregistered)

**Preregistered on 2026-10-12 before ANY S1 benchmark source,
native model training, validation readout, or measured model losses.**
This is strictly a **capacity-bounded development probe on VAL**.
It does not qualify ENG-1c's desired *full-corpus* experiment or
evaluate/consume a heldout TEST score.

## Immutable input provenance

The independently read-only fingerprint stage was merged as
[ENG-1c/0 PR #533](https://github.com/Memorithm/SML-GENIUS/pull/533).
Source/body hashes and untouched body extraction are pinned by
`docs/ENG-1C-FROZEN-CORPORA.md` and
`scripts/eng1c/fingerprint-eng1c.sh` and are NOT adjustable during
the pilot.

- French `17989`: body SHA256
  `24ebc9a0037fc57bf93bf3ac6788c5ca64a9a6bbaf92b9fdbdfe6ca0f4114dde`,
  **747855** bytes.
- English `1342`: body SHA256
  `e79303d04de9260489f9d5a2ad4aae5776680fcec484f2e732d6ded7411bf402`,
  **737950** bytes.
- Original 80/10/10 split only; training source is prefix `[0,a)`,
  VAL is `[a,b)`, `a=floor(8*N/10)`,
  `b=floor(9*N/10)`. **Never compute a next-byte score on
  `[b,N)` TEST.** No previous Jules Verne, Candide or Flaubert TEST.

## Frozen bounded TRAIN population: 64 × 112 bytes

Both full TRAIN splits exceed the hard `MAX_TRIE_EVENTS=500000`
restriction; a full 32-deep context tree risks exceeding the
250000-node/follower physical reference capacity.
For the first *development smoke only*, use exactly **112 contiguous
bytes from the CENTER of each of the 64 original full-TRAIN lanes**.
Specifically, for a TRAIN split length `S`:

1. `lane_len = floor(S/64)`; discard the remainder exactly as
   `ChunkLayout` does.
2. For lane `j=0..63`, take
   `full_train[j*lane_len + start .. j*lane_len + start + 112]`,
   where `start = floor((lane_len-112)/2)`. No target- or
   score-driven sampling. Preserve exact byte values, even when a
   byte falls inside a multi-byte UTF-8 character.
3. Concatenate the 64 windows in lane order. Train each reference
   on the identical **7168 bytes**, `64*(112-1)=7104`
   scored TRAIN next-byte events. Keep original full VAL split
   and all **74,688 FR / 73,728 EN** VAL next-byte events.
4. No context or token crosses the 64-window boundaries or VAL
   lane boundaries. The two books are trained/evaluated separately.

The subset has a mathematically hard upper bound:
`nodes <=1+7104*32 = 227329 (<250000)`,
`(node,next-byte) pairs <=7104*33=234432 (<500000)`,
and full-suffix context key bytes
`<=7104*(32*33/2)=3750912 (<8MiB)`.
All caps are based only on metadata and declared depth, not observed
prediction quality. Training beyond these resources returns STOP,
never silent context truncation.

**Major limitation:** the pilot uses only about 1.2% of the TRAIN
text. It cannot establish that a context-depth change improves
a well-trained language model; it only tests whether a depth difference
is useful at this exact constrained exposure.

## Fixed estimator and comparisons

- Immutable Rust `sml_core::elastic_ngram_trie::ElasticNGramTrie`
  and `elastic_ngram_long::ElasticNGramLong`, each fit once
  from the same sampled TRAIN, max depth **32** with original
  safe bounded defaults. Zero new model coefficients,
  no training on VAL/TEST and no change of interpolation rule.
- Independently train original
  `char_baselines::NGramModel` at depths 0, 4, 7
  on exactly the same TRAIN subset.
- Report at all predetermined query depths
  `[0,4,7,12,16,24,32]` on identical full VAL:
  256-class `bits_per_byte` (log-loss) and argmax accuracy,
  same denominator and target positions; metadata:
  TRAIN target count and retained node, edge, follower count
  plus logical key/counter bytes.
- Every `ElasticNGramTrie` depth score must agree exactly in
  `f64::to_bits()` for loss and accuracy with
  the full-suffix `ElasticNGramLong`; reference n-gram
  at k<=7 must also match exactly.
  On a deterministic representative 64-lane VAL history grid
  (first 24 steps and last 24 steps per lane), all 256
  class probabilities must also agree bitwise.
- Every computed 256-way distribution must be finite,
  strictly positive and sum to 1 within 1e-10. No hidden
  clipping or zero probability substitution.
- No multiple model policies are selected on TEST.
  An exploratory VAL winner (minimum bpb; smaller depth
  on ties) is recorded but has no confirmatory authority.

## Locked scientific development outcome

For **each** language, `LONG_DEPTH_DEV_GAIN` is declared
only if the k32 VAL log-loss is at least **0.02 bits/byte**
better than **both** k4 and k7 controls
(`bpb32 <= min(bpb4,bpb7)-0.02`).
If either language fails, the combined development result is
`ENG1C_S1_DEPTH_UTIL_STOP` (not an engineering CI error).
A negative or neutral result MUST be reported and the gate
**must not be revised post-observation**.
No statistical significance or generalization claim follows even
from PASS on this tiny frozen sample.

Functional correctness parity and complete provenance are
mandatory CI gates; negative *quality* result is admissible in
the science evidence and does not fail CI.

## Read-only runtime and reproducibility

- New `sml-core` opt-in benchmark binary and separate fetched corpus
  body loader; same raw/body SHA-256 guards as ENG-1c/0
  (no alternative source when a pin drifts), temp files only.
- GitHub-hosted `ubuntu-24.04`, read-only credentials,
  pinned Rust 1.89, `fmt/check/clippy -D warnings/tests`
  on exact HEAD. Two separate French/English execution jobs
  may score **VAL**, must never score TEST.
- Report actual source SHA, corpus body hashes, sample length and
  lane offsets, depth and reference parameter counts, metrics and
  scientific STOP/PASS. Retain only small CSV/logs. No model
  weights or raw source/body text in Git/artifacts.
- Real RSS/throughput not claimed by logical counters;
  ENG1b native CPU synthetic study
  `docs/RESULTS-ENG-1B-THOR-NATIVE-5X.md` is an independent
  physical reference, not a natural language quality result.
- No ElasticXxx physical actuation, no GPU, no SML production
  depth policy or 25B model promotion authorized.

## Next real ENG-1c quality milestone (not fulfilled by S1)

To use a *substantial* natural-language TRAIN population, first
freeze and qualify a scalable, exact memory/index capacity redesign
or a separately justified sparse support-pruning policy with true
resource measurements. A full-document or useful-scale cross-corpus
quality comparison then needs a distinct preregistered final TEST
and model-authorized depth guardrails.
