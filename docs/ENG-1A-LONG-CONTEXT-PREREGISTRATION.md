# ENG-1a — Exact long-context ElasticNGram (preregistered engineering study)

**Frozen on 2026-10-12 before creating any ENG-1a implementation, fixtures or running its tests.**
Status: reference/semantics only; no language-quality, latency, RSS, CTW or physical-resource result.

## Scope and prior evidence

ENG-0 (SML-GENIUS PR #523) retains an exact Witten–Bell 256-byte-class model
for context lengths 0..7, collision-free by packing into u64, with 64 isolated
contiguous training/evaluation lanes. Its public API and prior science cannot
be silently changed. ENG-1a adds a **separate, opt-in** model for longer
suffixes, up to 32 bytes, using the *complete ordered byte sequence* as
the exact context key. No hashing or lossy truncation is allowed.

This is a safe-Rust reference, not a claim that longer contexts improve
text prediction. CTW, PPM, adaptive profile selection and real disk/RAM
actuation remain explicitly deferred to separate preregistrations.

## Fixed algorithm and budgets

- Vocabulary = all 256 octets. For each TRAIN lane, for t=1..chunk_len-1,
  record y=byte[t] for each depth d=0..min(t, max_depth), using
  the exact suffix byte string byte[t-d..t].
- Context storage: one ordered map per depth from a full owned byte-string
  key to `(total:u32, sorted follower_byte -> count:u32)`. Empty key at d=0.
  Different full suffixes that share their last 7 bytes MUST NOT collide.
- Hard parameters: `max_depth <= 32`, `max_events <= 500000`,
  `max_contexts <= 250000`, `max_follower_pairs <= 500000`, and
  `max_context_key_bytes <= 8388608`. Zero/oversized ceilings fail closed;
  counters use checked arithmetic; construction returns no partially fitted
  model on resource failure.
- Scores at caller-declared `query_depth <= trained_max_depth`:
  initialize `P(y)=1/256`, iterate available suffix depths in increasing
  order, applying exactly the existing Witten–Bell recurrence
  `P_d(y) = [c_d(y)+T_d*P_(d-1)(y)]/[n_d+T_d]`, where T is the
  number of distinct observed followers. Missing suffixes retain the
  lower-depth probability. Each 256-way vector must be finite,
  strictly positive and sum to 1 within numerical tolerance.
- For depths 0..7, compare against BOTH the pre-existing ENG-0
  `ElasticNGram` and independent `char_baselines::NGramModel`,
  including exact `f64::to_bits()` distribution equality for
  deterministic TRAIN and unseen heldout input contexts.
- The training/evaluation split layout is the existing
  `anf_char::ChunkLayout`: 64 disjoint, contiguous lanes; no context
  carry across their boundaries. Scoring is read-only and does not
  alter TRAIN counts.
- Accounting: exact event count, number of stored contexts/pairs,
  exact sum of context-key payload bytes (length in bytes of each
  stored key), logical count-value bytes, and follower-key bytes.
  `Vec` buffers, map nodes, allocations, RSS, major faults, SSD
  bytes, and page-cache footprint are **not** represented by these
  logical quantities. Do not claim resident-memory savings.

## Frozen qualification fixtures

1. Deterministic 64-lane patterned corpus, length 40 per lane;
   train a 32-deep ENG-1a and compare 0..7 distributions against
   `ENG-0` and `NGramModel` at multiple lanes/timestamps,
   all 256 output symbols. Compare heldout scores using the
   same token denominator.
2. Two 12-byte histories share seven trailing zeros but have
   different first bytes; their TRAIN next bytes differ
   (`200` vs `201`). With a 64-lane repeated construction,
   the query at depth=7 must be exactly indistinguishable between
   the histories while the query at depth=12 must separate them.
   The correct-class probability at depth 12 must exceed 0.9.
   This is a synthetic **representation witness**, not language
   generalization evidence.
3. Query all depths 0..32 on deterministic observed/unseen suffixes
   and prove positive, finite 256-way distributions and normalization.
   Two identical TRAIN fits are structurally identical and inference
   does not mutate them.
4. Missing/undersized TRAIN streams, unsupported depth, every
   resource limit (events, contexts, follower pairs, key bytes)
   and invalid query depth must return a typed error.
5. Confirm `elastic_ngram.rs`, `char_baselines.rs` and all
   frozen A6 E4/E5 code remain unchanged.

## Evaluation and next phase

- ENG-1a CI: `fmt --check`, `cargo check --locked -p sml-core --all-targets`,
  `clippy --locked -p sml-core --all-targets -- -D warnings`, targeted
  `cargo test --locked -p sml-core --lib elastic_ngram_long`, plus
  the existing ENG-0 tests; general mandatory CI must pass on the
  exact final PR SHA before merge.
- No physical-runtime/controller/adapter change. SML-GENIUS owns model
  distributions and quality; ElasticXxx owns generic adaptive resource
  control only (ENG-2 issue Memorithm/ElasticXxx #244).
- Later ENG-1b must preregister **genuine** CTW/PPM comparators and
  frozen equal-data, equal-budget heldout model-quality evaluation
  (including 7/12/16/24/32-depth Witten-Bell controls), not reuse the
  synthetic witness to claim a win. A future ENG-3 may authorize
  profile switching only after independent loss/resource evidence.
- The 2026-10-12 ENG-0 model target and older A6-E4 frozen Candide
  TEST are not reopened.

## Source references

- https://github.com/Memorithm/SML-GENIUS/blob/main/crates/sml-core/src/elastic_ngram.rs
- https://github.com/Memorithm/SML-GENIUS/blob/main/crates/sml-core/src/char_baselines.rs
- https://github.com/Memorithm/ElasticXxx/issues/244
- F. Willems, Y. Shtarkov, T. Tjalkens, 1995,
  The context-tree weighting method: basic properties,
  IEEE Transactions on Information Theory 41(3), 653-664,
  DOI: 10.1109/18.382012 (background only; no CTW implementation claim).
