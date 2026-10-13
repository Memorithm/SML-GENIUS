# ENG-1c/VAL16 — explicitly post-hoc exploratory occurrence threshold 16

**Dated 2026-10-13. Protocol committed before writing VAL16 code or
running VAL16 predictions.** The previous immutable VAL1 scientific
pre-registration was committed as 5122a0357d5c64dd17826ab64f4a74e203058c6b,
and its negative findings were merged in PR #542 at
a9a8a8f4e30656c0810c4c8297b65ea6bd49c1b4 BEFORE this follow-up.

**Crucial independence boundary:** This extra threshold was requested
AFTER the same two validation datasets revealed the inferiority of
count thresholds 2/4/8. Thus this is a POST-HOC EXPLORATORY probe
on REUSED, EXPOSED validation data, NOT a valid independently heldout
confirmation, not part of the seven originally predeclared candidate
policies, and not a reset or revision of VAL1's scientific STOP.

## Frozen candidate and immutable controls

- Add exactly ONE new candidate, \`MIN_COUNT_16\`; no other
  thresholds, depth choices, estimator modifications, learnable
  parameters, annealing or search are allowed in this PR.
- Use the identical immutable, complete-TRAIN depth32
  \`sml_core::elastic_ngram_paged\` Witten–Bell count store already
  scientifically verified by CAP2b and used in VAL1.
- For each VAL prediction history, inspect only the counts from
  the exact full-context suffix in TRAIN, then pick deepest among
  depths [32,12,7] with occurrence count >=16, else use depth4.
  The selected probability distribution is the exact corresponding
  fixed-depth Witten–Bell 256-class vector, from the unchanged
  shared \`depth_ladder_readout()\`, with no VAL label/lookahead.
- Frozen, mandatory *positive test controls* are:
  \`FIXED_4\` and \`MIN_COUNT_8\` from original PR #542.
  The candidate registry and output order are exactly:
  [FIXED_4, MIN_COUNT_8, MIN_COUNT_16].
  They must reproduce original 12-decimal bpb, accuracy and
  selected-depth counters for each language or FAIL CI.
- Original VAL1 baseline reference figures (already exposed):
  FR FIXED_4 bpb=2.063036593417, accuracy=0.565633033419;
  MIN_COUNT_8 bpb=2.089344543258,
  depth choices [47520,25301,1867,0].
  EN FIXED_4 bpb=1.876564702039, accuracy=0.604180230035;
  MIN_COUNT_8 bpb=1.913076323341,
  depth choices [43834,27449,2422,23].
- Compare MIN_COUNT_16 against both FIXED_4 and MIN_COUNT_8
  on precisely identical VAL next-byte positions, not only
  classification accuracy. Report total log2 bits, bpb,
  accuracy, and chosen-depth counts 4/7/12/32. More
  restrictive count threshold may select shorter contexts
  more often; this is a hypothesis not an expected result.

## Frozen corpus identities, splits and TEST protection

- French: Gutenberg #17989 Dumas/Maquet, frozen body
  SHA256=24ebc9a0037fc57bf93bf3ac6788c5ca64a9a6bbaf92b9fdbdfe6ca0f4114dde,
  complete TRAIN byte SHA256
  ec538157302ad0d39668288c8109779b70492a93041d432f7f97b2dc96e535cd,
  now-exposed VAL SHA256
  d3ddafca85d8bfe057503bdb88c450f8110644b51fabbc2600cdb634b2933593;
  TRAIN 598284 bytes, VAL 74785 bytes, 74688 scored VAL target bytes.
- English: Gutenberg #1342 Austen, frozen body
  SHA256=e79303d04de9260489f9d5a2ad4aae5776680fcec484f2e732d6ded7411bf402,
  TRAIN SHA256
  43553238be6ec4a88e01a679a53381cd89aca8374d815e021e92317f71cb47d0,
  VAL SHA256
  f1a278d07c7d98a15f976d9ad7a673029a50d038b07d9d2508ffa1a4d41f882b;
  TRAIN 590360 bytes, VAL 73795 bytes, 73728 scored VAL target bytes.
- Reuse the already checked-out \`scripts/eng1c/fetch-val1-splits.sh\`
  without modifying it. Public raw/body SHA256 and byte-split
  normalization stay unchanged; verify both TRAIN and VAL SHA256
  in the new Rust binary. Extract only TRAIN and VAL into
  private runner temp, never TEST. Exactly 64 contiguous
  VAL lanes with t>=1 each, no history carry and no
  model update during validation.

## Frozen performance and uncertainty readout

- For each candidate, calculate all-256 argmax accuracy (lowest
  label on ties), total -log2 P(target), bpb = total_bits/n,
  n exactly 74688 FR / 73728 EN; report chosen depths
  and never hide negative outcomes.
- For paired improvements of MIN_COUNT_16 relative to
  FIXED_4 and MIN_COUNT_8, resample 64 VAL lane-local
  differences exactly 2048 times with xorshift64 seed
  \`0xE1C5_2026_1013_B001\`; report sorted indexes
  51 and 1996 as **descriptive** approximate intervals.
  Because the threshold choice was informed by prior VAL,
  the intervals are NOT confirmation or independent tests.
- Report a single diagnostic comparison:
  \`16_bpb - 8_bpb\` and \`16_bpb - fixed4_bpb\`.
  Classify only as \`EXPLORATORY_BETTER_THAN_FIXED4\`
  if strictly lower bpb than FIXED_4, else as
  \`EXPLORATORY_NOT_BETTER_THAN_FIXED4\`. No science
  or CI gate may be silently recast as a production win.
- A negative result passes engineering CI if byte-exact
  data, predictions, counts and artifact checks pass.
  A positive same-VAL result is only a hypothesis,
  requiring NEW preregistered nonoverlapping corpus
  validation before any performance promotion.
- All policies use a depth32 counted store (FR ~565MB,
  EN ~552MB) and inherit its physical construction,
  query I/O and count-store metadata cost. A depth gate
  cannot be described as saving memory because the
  complete store is still built.
- No model hardware actuation, ANF learner, PPM/CTW
  estimator, source identity update, extra post-hoc
  threshold, context-depth reencoder or kernel change
  is allowed in this phase.

## Engineering and publication

- A NEW binary \`sml-eng1c-val16\` in sml-core, leaving
  the frozen VAL1 binary, readout/model code and results
  unchanged. Add a separate hosted Rust 1.89 workflow
  with FR/EN matrix; verify strict fmt/check/Clippy,
  adversarial threshold-choice tests, original reference
  scores, exact source hashes, full VAL denominators
  and complete diagnostic output.
- Build the full TRAIN depth32 model with unchanged
  CAP2b storage, scratch/virtual-resource ceilings.
  \`ulimit -v 262144\`, timeout <= 3000 seconds per
  scoring process; no access from untrusted PR to Thor.
- Artifacts contain only tiny aggregate text, no
  Gutenberg bytes, count store, per-token predictions,
  TEST labels or private credential state.
- General SML CI and dedicated FR+EN must be GREEN
  on the exact final PR SHA; report and keep both
  positive or negative outcomes. No auto-default
  SML model, proof of attention replacement, adaptive
  policy production or ElasticXxx hardware authorization.
- A later new-corpus phase is tracked by SML issue #543.
  One-time protected TEST of the previously VAL1-selected
  FIXED_4 reference is unrelated and remains UNREAD.

References:
- https://github.com/Memorithm/SML-GENIUS/pull/542
- https://github.com/Memorithm/SML-GENIUS/issues/543
- https://github.com/Memorithm/SML-GENIUS/issues/530
