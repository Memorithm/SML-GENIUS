# ENG-2a — model-owned feasibility gate for exact n-gram representation transitions

**Preregistered 2026-10-13, before any ENG-2a code or tests.**
Status: research-only, opt-in, non-actuating. No quality/RAM-promotion claim.

## Motivation and ownership

ENG-1a PR #525 provides a full-suffix byte N-gram, ENG-1b PR #527 a
lossless reversed-suffix-trie representation. Both emit the same Witten–Bell
256-class distribution for *equal training and explicit context depth*.
Separate EL-NG1 PR #528 established case C4(e): an ElasticXxx
`VersionFrontier` may structurally accept an attested widening
`Reencode` after the SML source discarded information; the SML
representation rejects the operation as physically impossible.

SML owns model/representation/domain feasibility. ElasticXxx owns generic
resource state transitions, not domain quality. This study adds a
**separate opt-in research adapter** to the existing
`sml-elastic-bench` crate (which already pins ElasticXxx
`958ce6301730ceeec4bb1252ff12222e5bb7d431`). It must not modify
`sml-core` model semantics, the frozen EL-NG1 control protocol,
the runtime facade or ElasticXxx core.

## Frozen model and guard

- Train **both** `ElasticNGramLong` and `ElasticNGramTrie` from the
  *same exact immutable byte slice*, storing a copy of the full source
  bytes in a research-only snapshot. No hash or user-provided fingerprint
  is substituted for exact byte-for-byte source identity.
- Standard 64 contiguous lanes from `ChunkLayout`; maximum depth
  `0..=32`, maximum independent TRAIN next-byte events for this
  verification gate `8192`, plus the existing model construction
  budgets (`<=500000` events, `<=250000` contexts/nodes and
  `<=500000` follower pairs). If any model exceeds its declared
  limits, reject without publishing a combined snapshot.
- The requested transition is exactly
  `(format, depth) -> (format', depth')`. Formats are
  `FullSuffix` and `SharedTrie`, both explicit. This gate
  approves inspection only if depth=depth' and
  `depth <= trained_depth`. An attempted depth change, even to
  a smaller depth, is *model semantics change*, not a lossless
  representation transition; reject without staging.
- For a same-depth format change, require **Recompute** with the
  retained immutable source as the declared reconstruction provenance.
  A requested `Reencode` is **rejected before** ElasticXxx structural
  validation because no physical codec has been qualified. Unverified
  synthetic attestations must never authorize physical action.
- Require exact source bytes on every inspection, and a distinct
  nonempty **64-lane probe** (must not equal TRAIN bytewise).
  Reject mismatches, invalid/empty probes, nonexact depth changes,
  unsupported mechanism, missing physical implementation and invalid
  requested epoch.
- Before asking ElasticXxx to validate, compare the FULL 256-way
  `f64::to_bits()` distributions from both fitted models at the
  frozen requested depth on **every** TRAIN position t>=1 and every
  probe position t>=1, preserving lane resets. Reject a single
  mismatch, nonpositive/nonfinite output or sum error >1e-10.
  Compare probe `ByteScore` bitwise (tokens, bpb, accuracy).
  These are bounded empirical oracles, **not a mathematical
  universal equivalence proof over 256^32 histories**.
- Instantiate `VersionFrontier` and declare only two version-1,
  depth-bound target representations, with explicit allowed
  Recompute mechanism. Derive, propose and validate a shadow
  transition with the *existing* `TransitionAttestations`
  `attest_recompute_source_available`, then **ROLLBACK**
  unconditionally. Confirm the frontier's prior committed state,
  epoch and empty pending state after rollback.
  Do not invoke `commit`, a physical actuator, a memory
  allocator, a service, a GPU driver or any live resource mutation.
- One outcome record includes source byte length and scoped
  TRAIN/probe counts, source/target profiles, checked 256-way vectors,
  snapshot's model logical counts and the synthetic structural frontier
  verdict. It does **not** report a physical memory saving,
  hard RAM budget or cryptographic attestation.
- Same-profile lookup is explicitly a no-op. A request to perform
  production actuation must return a typed `PhysicalActuationUnavailable`.

## Predefined tests and negative controls

1. Patterned synthetic **64x40-byte TRAIN**, distinct **64x40-byte
   probe**, maximum depth 12. Strict 256-class full parity for every
   TRAIN/probe trajectory at depths `0,4,7,12`, with both
   directions FullSuffix⇄SharedTrie, and unchanged/rollback.
2. An explicit 12-byte two-teacher fixture where last seven
   bytes are identical but the older byte changes the next
   200/201 target. Request depth 12 must preserve the
   class probabilities and not truncate to 7.
3. A mismatched retained TRAIN source, a tampered/trained-on-wrong-data
   shadow trie, missing/identical probe, bogus epoch and depth
   change must be rejected **before** any generic transition claim.
4. `Reencode` must be refused despite the generic attestation
   being structurally sufficient in EL-NG1 C4(e).
   `Recompute` must validate structurally but never commit,
   retain model counts and have no physical actuator.
5. Up to 8192 checked TRAIN events; 8193 or more rejects.
   Invalid/oversized depth and model budgets reject.
6. Existing EL-NG1, ENG-0, ENG-1a and ENG-1b tests still pass.
   All frozen A6-E4/E5 tests/programmes remain untouched.

## Qualification and next gates

- GitHub-hosted isolated PR workflow (not Thor): pinned Rust 1.89,
  `cargo fmt --all -- --check`,
  `cargo check --locked -p sml-core -p sml-elastic-bench --all-targets`,
  `cargo clippy --locked -p sml-core -p sml-elastic-bench --all-targets -- -D warnings`,
  `cargo test --locked -p sml-elastic-bench eng2_trie_gate`,
  plus the existing SML-CORE exact ngram tests and general CI.
- Exact final PR HEAD mandatory CI before merge.
- Real physical representation conversion, peak RSS acceptance,
  source-bound cryptographic identity/epoch and post-actuation
  verification+rollback require **ENG-2b** with separate
  preregistration and real provider.
- ENG-1c independent language corpora are already frozen in
  PR #533, but their full TRAIN sets exceed the current 500000-event
  model capacity. Do not use their TEST to tune this guard.
- A pass is a **functional precommit gate**, not a default SML route,
  physical adaptation, semantic-depth authority or SML-25B proof.

References:
- https://github.com/Memorithm/ElasticXxx/issues/244
- https://github.com/Memorithm/SML-GENIUS/pull/527
- https://github.com/Memorithm/SML-GENIUS/pull/528
- https://github.com/Memorithm/SML-GENIUS/pull/533
