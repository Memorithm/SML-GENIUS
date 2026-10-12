# ENG-0 — ElasticNGram reference and ElasticXxx ownership (preregistration)

**Status:** development-only preregistration; experimental model component, not qualified elastic actuation.
**Frozen before ENG-0 implementation or ENG-0 tests.** Date: 2026-10-12.

## Ownership and exactness boundary

SML-GENIUS owns the probabilistic n-gram model, smoothing, tokenization,
statistics, semantic changes and model-quality decisions. ElasticXxx owns
only the generic, versioned adaptive-resource contract, numeric budget
admission and independently verified transactions. The new component is
**self-sufficient Rust** in `sml-core`, without an ElasticXxx runtime
dependency, GPU toolkit, service, or changes to pre-existing
A6-E1/E2/E3/E4/E5 frozen experiment source/tests.

Two *different* axes must never be conflated:

1. **Semantics preserving**: cache residency, representation or placement
   changes with unchanged trained histograms and byte-for-byte (or
   bit-for-bit float) next-byte distribution. A future ElasticXxx facade
   adapter may plan such transitions, but real actuation requires independent
   precommit and post-actuation equivalence, trace, rollback and freshness.
2. **Model semantics changing**: reducing the maximum queried context
   depth may change the next-byte distribution. No resource planner may
   label this an exact transition; model authority must explicitly qualify
   a new depth/profile using heldout log-loss and resource evidence first.

A profile recommendation, a lower memory count or successful unit tests
never grant model-quality authority.

## ENG-0 exact reference algorithm

- Train only on the existing `ChunkLayout`: 64 contiguous lanes, reset
  context at lane boundaries, predict the byte at time t from earlier
  bytes of the same lane, for t>=1. Preserve `char_baselines::NGramModel`
  baseline semantics, not a new global-stream token accounting.
- Vocabulary: 256 byte classes; context depths d=0..7 (u64 injective suffix
  key for up to seven bytes, no hash collisions).
- Immutable trained hierarchical context->next-byte histograms, ordered
  `BTreeMap` storage. Support explicit construction budgets: max depth,
  max training predictions, distinct contexts and distinct context/follower
  pairs. On an exceeded budget, fail **without publishing a partial model**.
- Prediction at explicitly caller-selected depth r<=trained depth:
  initialize a 256-way uniform distribution, then apply each observed
  Witten-Bell context of depths 0..r using
  `P_d(y)=[count_d(h,y)+T_d(h)*P_(d-1)(y)]/[n_d(h)+T_d(h)]`,
  with T_d number of distinct followers. Missing contexts retain the
  shorter distribution.
- For fixed depth r, the full 256-class distributions and heldout
  scoring must match the existing `NGramModel::train(train,r)` reference
  for all tested contiguous lanes and contexts. A different r is
  explicitly a **model-semantic transition**, not an exact optimization.
- Record per-depth observed context counts, unique follower counts,
  training predictions, and **logical counter payload only**, excluding
  allocator overhead, `BTreeMap` nodes, RSS, page cache and disk traffic.
  No assertion of a resident RAM ceiling from logical payload alone.

## ENG-0 functional gate

1. Each depth 0..7 matches the independent existing NGramModel oracle for
   256-way distributions on frozen deterministic train/test fixture.
2. Learned depth=4 exposes all r=0..4 with valid normalized positive
   distributions and correct scope; no TEST label access in training.
3. Depth transitions are explicitly labelled `Unchanged` or
   `ModelSemanticsChanged`; no silent quality-preserving downgrade.
4. Invalid size/depth/resource budgets and insufficient corpus reject.
   Two independent fits of the same training stream are deterministic.
5. Existing `char_baselines.rs`, `anf_byte_backoff.rs`,
   `anf_joint_feature_selection.rs` remain unchanged.
6. Required exact PR-head CI: `cargo fmt --all -- --check`,
   `cargo check --locked -p sml-core --all-targets`,
   `cargo clippy --locked -p sml-core --all-targets -- -D warnings`,
   `cargo test --locked -p sml-core --lib elastic_ngram` and
   existing general workspace CI if required. No test-based performance
   or natural-language superiority claim.

The study does NOT implement CTW (a different estimator) or claim
CTW-style redundancy guarantees. It also does NOT implement physical
tiering, learned dynamic controllers, adaptive train-stream mutation,
online compaction, or an ElasticXxx production controller.

## Next distinct phases, not prequalified by ENG-0

- **ENG-1:** context trees / CTW and byte-class PPM alternatives against
  Witten-Bell, under same train/validation/test protocol.
- **ENG-2:** versioned, source-bound memory-residency and exact
  equivalence contract; proposed generic resource dimensions only in
  ElasticXxx, no NGram-specific model semantics there.
- **ENG-3:** model-authorized context-depth profile selection, loss and
  calibration guardrails, fresh split; static profile versus adaptive
  quality/resource tradeoff with metadata and traffic accounted.
- **ENG-4:** hybrid ElasticNGram + ANF expert, model-conditioned arbiter,
  measured improvement attributable to actual composite ANF features
  versus a matched raw-bit hybrid.
- **ENG-5:** spill/mmap/cold-page transitions through validated
  ElasticXxx facade adapters and rollback on a real backend. SML
  retains the final inference-time semantics independently.

## Prior sources

- `crates/sml-core/src/char_baselines.rs` — existing exact static Witten-Bell reference.
- `crates/sml-core/src/anf_byte_backoff.rs` — existing separate sparse
  Boolean-signature conditional byte readout.
- ElasticXxx `README.md`, `AGENTS.md`, off-main ML-maturity overlay
  and ElasticXxx ecosystem roadmap, as read before ENG-0.
- F. M. J. Willems, Yu. M. Shtarkov & T. J. Tjalkens (1995),
  *The context-tree weighting method: basic properties*,
  IEEE Transactions on Information Theory 41(3), 653–664,
  https://doi.org/10.1109/18.382012 (theoretical inspiration, not used as
  a guarantee for Witten-Bell).
