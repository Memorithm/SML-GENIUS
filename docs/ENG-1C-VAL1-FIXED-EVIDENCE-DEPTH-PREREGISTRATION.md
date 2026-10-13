# ENG-1c/VAL1 — independent real-text log-loss of fixed and evidence-gated ElasticNGram depths

**Preregistered 2026-10-13 before any ENG-1c/VAL1 scoring code or any
model prediction on the selected validation bytes.** Existing corpus
identity, TRAIN storage, source SHA-256 and model mathematics are
frozen by ENG-1c/0 (PR #533) and CAP2b (PR #541).

The purpose is to test a **falsifiable prediction-quality hypothesis**:
Does a model-owned, TRAIN-count-evidence gate select context depth more
effectively than a fixed-depth Witten–Bell model *on independent VAL
text*, when all candidates share the **same trained depth-32 paged
count store**? A VAL improvement is exploratory and not a
confirmation on untouched TEST. No ANF augmentation, CTW/PPM
estimator, GPU acceleration, resource actuation, or SML-25B model
promotion is included.

## Identical frozen data and predictors

- FR corpus: Project Gutenberg #17989 Dumas/Maquet, source raw SHA256
  9ea7a8b127a9dfa36de0b0cf9c2e26293beea5c37a0fc988fb9308b63160f282,
  body SHA256 24ebc9a0037fc57bf93bf3ac6788c5ca64a9a6bbaf92b9fdbdfe6ca0f4114dde,
  body 747855 bytes, TRAIN prefix 598284 bytes, VAL next 74785
  bytes, TEST next 74786 bytes **never passed to any model**.
- EN corpus: Gutenberg #1342 Austen, raw SHA256
  3f6bb9d6f78e0293b56acd4714dd68cb7d6d1d293402031ce9d5a216bcaf9d75,
  body SHA256 e79303d04de9260489f9d5a2ad4aae5776680fcec484f2e732d6ded7411bf402,
  body 737950 bytes, TRAIN prefix 590360 bytes, VAL next
  73795 bytes, TEST next 73795 bytes not passed to any model.
- Normalize exactly as ENG-1c/0: delete CR; exclude Gutenberg START/END
  delimiter lines; preserve all remaining LF-terminated body lines.
  Parse 80/10/10 byte split with a=floor(8*N/10),
  b=floor(9*N/10). Download and compare BOTH raw and body
  SHA-256 and exact sizes. Create only TRAIN and VAL files
  in private fresh hosted-runner temporary directories; no
  TEST data file is created. No private service or credentials.
- Source TRAIN SHA256 must exactly match established CAP2b result:
  FR ec538157302ad0d39668288c8109779b70492a93041d432f7f97b2dc96e535cd;
  EN 43553238be6ec4a88e01a679a53381cd89aca8374d815e021e92317f71cb47d0.
- TRAIN is fitted with already qualified immutable
  \`sml_core::elastic_ngram_paged\` at max depth 32, existing
  independent research quotas: <=650000 next-byte TRAIN events,
  <=16m distinct contexts and context/follower pairs, <=768 MiB
  counted store, <=100k sparse index entries, <=1GiB sorted
  scratch, <=2GiB total scratch + store + index. Do not alter
  the frozen model or its learned 256-way smoothing rules.
  As measured by CAP2b, the full-TRAIN depth32 store contains
  FR 14281533 contexts/14878280 follower pairs, EN
  13932625 contexts/14519708 follower pairs.
- VAL is cut into exactly 64 **contiguous** lanes with
  \`L=floor(VAL_bytes/64)\`: FR L=1168, scored 74688 events
  (33 trailing VAL bytes unused); EN L=1153, scored 73728
  events (3 trailing VAL bytes unused). Score only t=1..L-1
  within each lane. No TRAIN-to-VAL history transfer or
  cross-VAL-lane context carry. All candidate policies consume
  IDENTICAL target positions.
- **NO TEST prediction/readout whatsoever** in this PR.
  Any TEST evaluation requires a separate preregistration and
  an immutable VAL-selected candidate after this experiment.

## Fixed predictor and new safe observer

- Fixed depth policies are **[4,7,12,32]**, not retuned after
  VAL results. All use the **same depth32 trained count store**
  and original Witten–Bell 256-way conditional:
  P_0 prior 1/256, then for each available context of depth d,
  P_d(y) = P_(d-1)(y) * T_h/(N_h+T_h)
           + count(h,y)/(N_h+T_h),
  with all 256 scale multiplications done before sorted follower
  additions. A missing context backs off unchanged. No learning
  or count update on VAL.
- Add a read-only \`depth_ladder_readout(history)\` to the paged
  store. It emits four **bit-identical** 256-way distributions
  relative to calling existing \`distribution(history,d)\`
  separately for d=4/7/12/32, along with the exact TRAIN
  occurrence total at *that full requested suffix depth*.
  Missing/too-short depth -> evidence count zero; do not leak
  current target byte. The observer reads depth layers only
  once (0..=min(32,history.len())) and does not modify
  stored counts or probabilities.
- Evidence-gated policies use fixed MINIMUM observed TRAIN
  context count **[2,4,8]**. At every prediction, select the
  deepest candidate in [32,12,7] whose exact full suffix
  TRAIN count is at least the threshold. If none qualifies,
  use fixed depth4. No lookahead, future position, label,
  VAL learned count, ANF flag or budget setting is permitted
  in selection. The selected distribution is exactly that
  fixed-depth readout from the SAME 32-deep model; this
  gate is *model-owned* and changes semantics, not a
  representation/RAM-only switch.
- There are thus **seven frozen candidate policies**, in
  immutable tie-order:
  FIXED_4, FIXED_7, FIXED_12, FIXED_32,
  MIN_COUNT_2, MIN_COUNT_4, MIN_COUNT_8.
  The evidence-gated policies must report fractions of
  VAL events selecting depths 4/7/12/32. All policy
  denominators are exactly 74688 FR / 73728 EN.
- The model bitwise observer is independently checked on
  synthetic 64x40 TRAIN and separate 64x40 probe at all
  four depths against both original ENG-1a
  \`ElasticNGramLong\` and ENG-1b \`ElasticNGramTrie\`.
  Missing contexts, newest-7 collision vs older depth12,
  all-zero suffixes and 64-lane reset are included.
  Any probability mismatch is a hard CI failure.
- No extra post-processing, probability temperature, floor,
  clipping, bespoke smoothing, logit arbiter or ANF learner
  can be introduced after VAL observation.

## Frozen scoring, selection and reproducibility

- For each candidate and every full VAL target, accumulate
  \`-log2(P(target|history))\`, all-256 argmax accuracy (ties
  broken by earliest byte, consistent with ENG references).
  Output total model bits, bits per byte, and accuracy,
  with exact common scored-token denominator.
- Store 64 lane-local bit sums per candidate to compute
  descriptive paired differences relative to FIXED_4.
  Predeclared bootstrap: **2048** 64-lane resamples
  with replacement from 64 lanes, deterministic xorshift64
  PRNG seed \`0xE1C5_2026_1013_B001\`, sort 2048
  bootstrap mean differences and report order-statistics
  at indexes **51** and **1996** (approx 95% interval).
  This is descriptive because the VAL data also chooses
  the candidate; NOT confirmatory confidence/TEST evidence.
- Report the best candidate in each language by lowest
  full-VAL bits/byte. If exact \`f64::to_bits()\` ties,
  resolve in the frozen order above, with FIXED_4 first.
  The selected policy is not changed after observing results.
- Resource accounting: record actual depth32 counted
  bytes and sparse index bytes from immutable CAP2b,
  Linux process VmHWM for build/score separately where
  observable, training and scoring durations, and no
  claim of physically reduced RAM due to the evidence
  gate. Frozen comparator store depth4 on same TRAIN
  used 4-5MB counted data in CAP2b; a gate backed by
  depth32 incurs the larger 550-565MB store. This
  trade-off must be explicitly visible in the result.
- Strict **exploratory** adaptive-evidence gain threshold:
  for BOTH languages, the best gated candidate must have
  full-VAL bpb at least **0.01 bits/byte lower** than
  the best of the four fixed-depth candidates, and
  its paired 64-lane bootstrap interval on
  improvement vs the best fixed must have lower bound >0.
  Otherwise the scientific result is **STOP/NOT_PROVEN**,
  including mixed or negative results. **Never fail
  compilation CI because science returned STOP**.
- Fixed-depth ordering quality comparisons vs FIXED_4
  must be reported irrespective of adaptive result,
  including depth32 if it performs worse.
- The reference and candidate models never score TEST
  in this experiment. Later TEST promotion (if any)
  requires a **separate** preregistration, freezing
  the VAL winner, budgets and stop criteria BEFORE
  a future run sees TEST bytes.

## Hardware / CI / fail-closed criteria

- GitHub-hosted Ubuntu 24.04 x86_64 only; no persistent
  Thor code from an untrusted PR, no CUDA/NVIDIA toolkits.
  Rust 1.89.0 pinned. One job per language, isolated
  reproducible sorted count-store construction in
  a fresh directory, read-only reopen verified by
  SHA256/source-depth/counts. Use \`ulimit -v 262144\`
  to cap virtual address space during each fresh
  release build/read/scoring process; 1GiB scratch,
  2GiB simultaneous source/model quota,
  timeout <=50 minutes per full VAL scoring process,
  80 minutes per language job. These are operating
  bounds, not GPU/page-cache/MemAvailable guarantees.
- Artifacts contain a tiny text manifest with per-arm
  VAL metrics, lane-bootstrap intervals, chosen depths
  and the source/program commit SHA; NO Gutenberg
  text, no sorted records, no count-store bytes and
  no per-token labels.
- Hash mismatch, corpus size/split drift, a single
  256-way oracle bit difference, scored-token mismatch,
  missing/NaN probability, incomplete profile,
  missing output artifact or process crash => **CI FAIL**.
  A zero/negative adaptive gain despite passing all
  scientific/engineering checks => **SCIENCE STOP**.
- Test no TEST file is passed to model. Keep all existing
  ENG/EL-NG1/CAP2 scientific source and ANF frozen fixtures
  untouched. General SML CI and dedicated VAL1 workflow
  must pass on the same exact final PR head before merge.

## Interpretation boundaries

A positive VAL result is a hypothesis to verify on protected
TEST in a future separate PR, **not** a production proof,
a Transformer replacement, ANF innovation, source-level
resource saving, or live ElasticXxx actuation. A negative
VAL result is useful science: static backoff can already
recover much of the value of long contexts, while
millions of singleton histories increase metadata/I/O.
Compare against CTW/PPM in a separate preregistered
estimator benchmark. SML owns all model quality;
ElasticXxx owns generic resources only.
