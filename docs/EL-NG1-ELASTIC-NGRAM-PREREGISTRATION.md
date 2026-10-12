# EL-NG1 — ElasticNGram control-plane qualification preregistration

Date: 2026-10-12. Status: **frozen Development-only functional experiment**.
This protocol commit must precede all new source, tests and workflow for
EL-NG1. Any change to a frozen quantity below after code exists requires a
separate dated amendment document committed before the affected rerun; a
failed criterion is reported, never repaired by editing the criterion.

## Purpose and boundary

Qualify, on a deliberately simple and exactly accountable SML-owned baseline,
how the ElasticXxx representation-transition control plane behaves when the
adaptive resource is the *physical representation of a byte n-gram table*
(context order and per-count width). The experiment exercises ElasticXxx's
resource declaration, EIR lowering, `VersionFrontier` propose / validate /
commit / rollback, transition mechanisms (`Reencode`, `Recompute`) and
attestations, and measures what the control plane does **not** check.

It is not an SML architecture result, not a claim that an elastic n-gram beats
the existing `char_baselines::NGramModel` (parity is a precondition, not a
finding), and not a performance or hardware claim. The ecosystem roadmap lets
ElasticXxx later select only among prequalified execution plans; this study
does not widen that role and gives ElasticXxx no authority over model
semantics. Resource changes are explicit in every reported row (AXSML4
invariant: adaptive resource changes remain explicit in comparison evidence).

Model sovereignty: `sml-core` stays dependency-free. ElasticXxx is a
dependency of the separate bench crate `sml-elastic-bench` only, pinned to
git revision `958ce6301730ceeec4bb1252ff12222e5bb7d431` (crates
`memorithm-elastic-core`, `memorithm-elastic-eir`). Same owner, same
PolyForm Noncommercial 1.0.0 license; no model runtime depends on it.

## Frozen model: `sml_core::elastic_ngram_packed::ElasticNgram`

_Erratum (before any frozen-corpus run): the module was named `elastic_ngram` at preregistration; it was renamed `elastic_ngram_packed` because `main` gained an unrelated ENG-0 `elastic_ngram` module. No criterion, parameter or behaviour changed._

- Interpolated Witten-Bell byte n-gram over the existing `ChunkLayout`
  (64 lanes, context reset at chunk starts, first byte of a chunk is not
  predicted), probabilities computed exactly as
  `char_baselines::NGramModel::distribution` (same level order, same
  floating-point operation order). Maximum context `k` in `0..=5`
  (`k = 0` is the unigram).
- Count width `w` in `{32, 16, 8, 4, 2}` bits. The trained base model has
  `w = 32`.
- **Reencode** to `(k', w')` with `k' <= k` and `w' <= w` (anything else is
  refused with a typed error): drop context levels above `k'`; per surviving
  context let `m` be its maximum count and `s` the smallest integer with
  `(m >> s) <= 2^w' - 1`; replace every count `c` by `c >> s`; drop followers
  whose count becomes 0; drop contexts left empty.
- **Exact accounting** (bits): header `16 + 32*(k+1)`; per context
  `8*len + 8`; per follower `8 + w`. `len` is the context length of the level.
- **Packed oracle**: a canonical bit stream (contexts sorted by key, followers
  sorted by byte, LSB-first) whose length in bits must equal the accounting
  and which must decode back to an identical model.

## Data

Frozen Candide corpus of A6-E4 (`scripts/fetch-candide-corpus.sh`: body
210655 bytes, SHA256
`48e04595f5fc681ef6c0498123180bce0125d8fbe2167f3bcaeef189b45bd4b9`),
`CharSplits` 80/10/10. TRAIN fits tables, VAL calibrates and gates.
**TEST is not read**: the bench discards `CharSplits::test` at load. VAL is
used both to measure the grid and to gate; this is Development use and
supports no generalisation claim.

## Frozen ElasticXxx declaration

Resource id `sml.elastic-ngram`, class `REPRESENTATIONAL`, allowed dimension
`REPRESENTATION`, preserved invariant
`UpholdContract("sml.elastic-ngram.val-bpb-gate")` along `REPRESENTATION`,
objective `MEMORY_FOOTPRINT`, admitted transitions `Reencode` and `Recompute`
on `REPRESENTATION` with the matching capability requirements, observed signal
`FREE_CAPACITY`. Representation id `sml.elastic-ngram.k{k}.w{w}`, schema 1;
capabilities contain exactly the 30 grid states `k in 0..=5` x `w` above.

## Frozen controller

- Grid G: the 30 `(k, w)` states. For each, size (exact bits) and VAL
  bits/byte are measured once from `reencode(base, k, w)`.
- Reference `ref = min` VAL bits/byte over G. Gate `delta = 0.50` bit/byte.
- Initial committed state `(5, 32)`, epoch 0.
- Budget schedule in KiB (1 KiB = 8192 bits), applied in order:
  `[2048, 1024, 512, 256, 128, 64, 32, 16, 8, 256, 2048]`.
- Per budget `B`: `cand` = member of G with size <= B minimising VAL bpb
  (ties: smaller size, then smaller `k`, then smaller `w`).
  1. no member fits -> outcome `NoCandidate`;
  2. `cand.bpb > ref + delta` -> `QualityInfeasible` (no transition);
  3. `cand == committed` -> `Stay`;
  4. otherwise mechanism = `Reencode` if `cand.k <= committed.k` and
     `cand.w <= committed.w`, else `Recompute`. Stage with
     `derive_target`, shadow-materialise (reencode the committed model, or
     retrain on TRAIN then reencode), validate with exactly one attestation
     (`reencoder_available` for `Reencode`, `recompute_source_available` for
     `Recompute`), commit -> outcome `Committed`.
  `NoCandidate` and `QualityInfeasible` leave the committed state unchanged
  and are reported with whether the committed size exceeds `B`.

## Criteria (all deterministic; any failure fails the run)

- **C1 parity.** For `k in 0..=5`, `ElasticNgram(k, 32)` and
  `NGramModel::train(TRAIN, k)` give identical `ByteScore` on VAL
  (`tokens` equal; `bits_per_byte`, `accuracy` bit-identical).
- **C2 packed oracle.** For all 30 grid states: packed length == accounted
  bits; decode(encode(m)) == m; no stored count exceeds `2^w - 1` (`w < 32`).
- **C3 reencode algebra.** For all grid pairs `t <= i` componentwise:
  `reencode(reencode(base, i), t) == reencode(base, t)`;
  `reencode(m, same) == m`; every widening (larger `k` or `w`) is refused.
- **C4 frontier verdicts.** Five scripted transitions from `(k2, w8)`:
  (a) `Reencode` to w16 with no attestation -> `MissingReencoderAttestation`;
  (b) `Recompute` without epoch advance -> `EpochMustAdvance`;
  (c) target `(k6, w8)` not in capabilities -> `UnsupportedTarget`;
  (d) cross-contract `Reinterpret` without equivalence ->
  `MissingSemanticEquivalenceAttestation`;
  (e) widening `Reencode` to w16 *with* a reencoder attestation -> ElasticXxx
  returns `Ok` (recorded as a finding: structural validation cannot know the
  transition is information-losing-impossible) **and** the SML feasibility
  gate refuses the same transition. Expected: ElasticXxx Ok, SML refusal.
- **C5 controller.** The executed outcome sequence equals the sequence derived
  independently from the grid table by the oracle function; for every
  `Committed`/`Stay` step the committed size <= `B` and committed VAL bpb <=
  `ref + delta`; epochs strictly increase exactly on `Committed`; for every
  `Committed` step the shadow model's VAL bpb is bit-identical to the grid
  value of its target.
- **C6 rollback atomicity.** Stage a valid proposal, roll it back: committed
  state and the FNV-1a-64 of the committed model's packed bytes are unchanged
  and a new proposal can be staged. (FNV is a change detector, not a
  cryptographic hash.)
- **C7 EIR determinism.** Lowering the frozen declaration twice gives equal
  fingerprints; both admitted transitions are present.

## Reported only (no pass/fail)

The full 30-state grid (exact bits, entries, VAL bits/byte, accuracy), the
schedule trace with mechanism and outcome per step, wall-clock of each phase
(shared CI runner, informational), and the EIR fingerprint.

## Not claimed

No quality claim beyond parity with the existing baseline; no TEST evidence;
single corpus and single language; budgets are synthetic; VAL reused for
calibration and gate; ElasticXxx fingerprints and evidence tokens are
unauthenticated structural claims; ElasticXxx validation is structural and
does not measure model quality (this study measures that boundary); no
resident-memory, latency or hardware conclusion (accounting is of the packed
representation, not process RSS); no inference about SML-25B.

## CI

Pinned Rust 1.89: `fmt --check`, `cargo check --locked`, strict Clippy
(`-D warnings`) on `sml-core` and `sml-elastic-bench`, unit tests, then fetch
the frozen corpus with `scripts/fetch-candide-corpus.sh` and run the bench in
release mode; archive only the small report.
