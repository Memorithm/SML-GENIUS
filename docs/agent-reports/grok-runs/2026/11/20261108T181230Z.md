# Grok SML-GENIUS report: 2026-11-08T18:12:30Z

- PR: https://github.com/Memorithm/SML-GENIUS/pull/891 (MERGED, squash `f2b908e8b12335025a22df722fd3283f0137e9f1`, 2026-11-08T18:10:48Z)
- Status: **G17 MERGED**. CONTEXT-2 adversarial composed-mutation qualification of the G16 corpus (two published decision gates at a time along the assigned consecutive pairs). RemoteOps **NOT_CONNECTED**. Metrics **NOT_MEASURED**. Native-quality qualification **NOT_QUALIFIED**.
- Base: main `fcf76a314be6fdcbd3b5bd594c2952f642b676e3`. Exact PR head: `f20776f04b8ed1db6309e3a6648a38d566f2f6e2`.
- Branch: `grok/g17-composed-mutations-20261108`. Assigned by ChatGPT on RemoteOps #1208 (comment 6724375732; later comment 6725021430 noted #891 OPEN with CI in progress). **LONGCTX-B1** untouched and **BLOCKED**. RemoteOps M2 not touched.
- Wake source: Grok Bot watchdog routine/executor (in-agent). It does **not** show the RemoteOps→Grok loop is CONNECTED.

## G16 reconciliation
G16 code PR #889 is **MERGED** (squash `a837fb6bf4478d89d41ebce2aeb3a67519908781`, exact head `616132096f276307b63a7b4c76c6cd56ba8ded67`). G16 report PR #890 is **MERGED** (squash `fcf76a314be6fdcbd3b5bd594c2952f642b676e3`, exact head `b9b516c316cc904c278cd02d435ffa9686f87dfe`, 2026-11-08T17:17:20Z; ChatGPT independently ACKed it in comment 6725021430).

## What changed in G17
- Tests-only suite: `crates/sml-store/tests/working_context_anchor_composed_mutations.rs`, `G17_CORPUS_VERSION = 1`, 13 frozen rows (1 positive-control source + 12 composed mutants) covering the four assigned consecutive pairs: version+envelope, envelope+provenance, provenance+G10 stream, G10 stream+rollback.
- Each follow-up perturbs exactly two published gates of the accepting source. Expected typed errors are derived independently from published precedence (the higher-precedence altered gate): `VersionUnknown(2)` / `VersionDowngrade(0)` (version+envelope), `Envelope` (envelope+provenance), `ProvAuthority` / `ProvStream` / `ProvDomain` (provenance+G10 stream), `AnchorStream` (stream+rollback; generation and digest checks are not reached).
- Frozen pairwise coverage matrix: every composed mutant is killed by `typed_error` (must report the independently expected typed cause, not an accept and not a different error) and by `nonmutation` (canonical SWC2 encoding, SWE1 seal and all five `WorkingContextStats` counters stay byte-for-byte unchanged; the anchor is not advanced).
- Each row runs through an independent pure oracle (own SWE1 layout and unkeyed-digest check, own G12/G10 decision order, own version gate; shares only the SHA-256 primitive; takes stream/generation from frozen facts instead of decoding SWC2) and through production `restore_into_anchored_with_versioned_provenance`. Expected outcomes are reasoned from the published gate order, not from the production function body. Outcomes must be identical. The unmutated positive control remains accepted and replaces the target exactly; it is re-checked after every composed mutant.
- Frozen vectors: SHA-256 of the 2 sealed fixtures (`c4b2bc9f8c9511da0577273364b89958bfe9ed796c2f8c9ac7b4762cd039f880`, `107cac73be4a2db6071aa33425479a2b037b31ea402d46e1360277bb1554bb1a`) and of the canonical corpus rendering (`15ceba9cdb3ed42f92acd73bdf2208dcd7050298bd3c489a7f5a64ee0b298bb8`) are pinned and re-checked for determinism.
- Positive control: matching version, intact envelope, matching G12 identity and matching G10 stream/generation is accepted and replaces the target exactly. 5 tests (4 corpus/oracle/matrix tests + 1 explicit check that the authority tag is an in-process equality binding, not authenticated provenance).
- Production change: none. The suite did not expose a defect; production already implements the published order (version → envelope → G12 → G10 stream → G10 rollback). No policy migration, signing or hardware claim. The G14, G15 and G16 corpora are untouched.

## Evidence
- Pre-push hook, no `--no-verify`: fmt, clippy `--locked -D warnings`, and workspace tests passed (as recorded in the PR #891 body).
- `cargo test --locked --workspace` (local, per PR #891 body): **2477 passed, 0 failed, 3 ignored** (G16 baseline 2472 + 5 new G17 tests).
- New suite: 5 tests in `working_context_anchor_composed_mutations`.
- Exact-head CI on `f20776f04b8ed1db6309e3a6648a38d566f2f6e2`, all SUCCESS: fmt (job 128810537844, 17s), rust (job 128810613518, 2026-11-08T18:10:28Z, 23m39s), g1-realtext-smoke, owner-admission, primary-provider-owner-review, icm-feature.
- Workflow runs: CI 42506624608, G1 realtext 42506624459, Trusted RSI 42506624384, External weight admission 42506624730, grok-icm 42506624397.
- RemoteOps `ops/grok/grok_report_gate.py` validates the status/report schema only. The Grok invocation is NOT independently attested.

## Limitations
- Anchors and authority tags in tests are mocks. The authority tag is an equality binding, not an authenticated origin. Attestation, provisioning, authentication and hardware-backed storage are **NOT QUALIFIED**.
- SWE1 stays unkeyed SHA-256: corruption detection, not authentication. The declared policy version is a caller assertion, not authenticated.
- No hardware, E2E, transport or metrics claim.

## WAITING_ON
1. Next ChatGPT/roadmap slice on RemoteOps #1208 (comment 6725021430 assigned G18 only after G17 is qualified: deterministic permutation invariance of composed-mutation test ordering).
2. Dell/Thor self-hosted runners.
3. LONGCTX-B1 user choices (filler/B-lex/B-attn).

## Measured
None. Status stays **NOT_MEASURED**.

## Out of scope
LONGCTX-B1 campaign. Claude ternary/hybrid. NVIDIA stack. CCOS recall is not a 1M attention window. RemoteOps M2 ACK journal work. G18 not started in this report.
