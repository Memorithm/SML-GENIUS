# Grok SML-GENIUS report: 2026-11-08T16:42:15Z

- PR: https://github.com/Memorithm/SML-GENIUS/pull/889 (MERGED, squash `a837fb6bf4478d89d41ebce2aeb3a67519908781`, 2026-11-08T16:41:58Z)
- Status: **G16 MERGED**. CONTEXT-2 metamorphic and mutation-sensitivity qualification of the G15 error-precedence oracle (one published decision gate at a time). RemoteOps **NOT_CONNECTED**. Metrics **NOT_MEASURED**. Native-quality qualification **NOT_QUALIFIED**.
- Base: main `38e765ea1e4874de624f8e887f385562f8b65f83`. Exact PR head: `616132096f276307b63a7b4c76c6cd56ba8ded67`.
- Branch: `grok/g16-mutation-sensitivity-20261108`. Assigned by ChatGPT on RemoteOps #1208 (comments 6723028903 and 6723647281). **LONGCTX-B1** untouched and **BLOCKED**. RemoteOps M2 not touched.
- Wake source: Grok Bot watchdog routine/executor (in-agent). It does **not** show the RemoteOps→Grok loop is CONNECTED.

## G15 reconciliation
G15 code PR #883 is **MERGED** (squash `30568a3fc12969e1d19dc5e819096f4287d13ce7`, exact head `b0f1292a737648cfdc5ebccfae8f3992ed68c23a`). G15 report PR #886 is **MERGED** (squash `38e765ea1e4874de624f8e887f385562f8b65f83`, 2026-11-08T15:37:31Z).

## What changed in G16
- Tests-only suite: `crates/sml-store/tests/working_context_anchor_mutation_sensitivity.rs`, `G16_CORPUS_VERSION = 1`, 10 frozen rows (1 positive-control source + 9 single-gate mutants) covering the five published decision gates: version, SWE1 envelope, G12 provenance, G10 stream, G10 rollback.
- Each follow-up perturbs exactly one gate of the accepting source. Mutants: `VersionUnknown(2)`, `VersionDowngrade(0)`, envelope truncate, envelope trailer flip, G12 authority, G12 presented stream, G12 domain, G10 stream (matching digest on a foreign-stream anchor), G10 rollback (older Home1 snapshot; generation check precedes digest equality).
- Frozen detection matrix: every mutant is killed by `typed_error` (must report the independently expected typed cause, not an accept and not a different error) and by `nonmutation` (canonical SWC2 encoding, SWE1 seal and all five `WorkingContextStats` counters stay byte-for-byte unchanged; the anchor is not advanced).
- Each row runs through an independent pure oracle (own SWE1 layout and unkeyed-digest check, own G12/G10 decision order, own version gate; shares only the SHA-256 primitive; takes stream/generation from frozen facts instead of decoding SWC2) and through production `restore_into_anchored_with_versioned_provenance`. Expected outcomes are reasoned from the published gate order, not from the production function body. Outcomes must be identical. The unmutated positive control remains accepted and replaces the target exactly; it is re-checked after every mutant.
- Frozen vectors: SHA-256 of the 2 sealed fixtures (`2afe5e81998a3ade1462ceb5972862f19edfe292c10bb36712479486a25ec3b0`, `124d72f9a5ec3ff4917a31667aa6f3b8ffbe555d72dd4001114983f223b9c72b`) and of the canonical corpus rendering (`42aebafe40cddf394ca119390dec4ac9c6593f220f77ae94d39ea78293fc6055`) are pinned and re-checked for determinism.
- Positive control: matching version, intact envelope, matching G12 identity and matching G10 stream/generation is accepted and replaces the target exactly. 4 tests.
- Production change: none. The suite did not expose a defect; production already implements the published order (version → envelope → G12 → G10 stream → G10 rollback). No policy migration, signing or hardware claim. The G14 and G15 corpora are untouched.

## Evidence
- Pre-push hook, no `--no-verify`: fmt, clippy `--locked -D warnings`, and workspace tests passed (as recorded in the PR #889 body).
- `cargo test --locked --workspace` (local, per PR #889 body): **2472 passed, 0 failed, 3 ignored** (G15 baseline 2468 + 4 new G16 tests).
- New suite: 4 tests in `working_context_anchor_mutation_sensitivity`.
- Exact-head CI on `616132096f276307b63a7b4c76c6cd56ba8ded67`, all SUCCESS: fmt, rust (job 128788789478, 2026-11-08T16:41:37Z, 20m24s), g1-realtext-smoke, owner-admission, primary-provider-owner-review, icm-feature.
- RemoteOps `ops/grok/grok_report_gate.py` validates the status/report schema only. The Grok invocation is NOT independently attested.

## Limitations
- Anchors and authority tags in tests are mocks. The authority tag is an equality binding, not an authenticated origin. Attestation, provisioning, authentication and hardware-backed storage are **NOT QUALIFIED**.
- SWE1 stays unkeyed SHA-256: corruption detection, not authentication. The declared policy version is a caller assertion, not authenticated.
- No hardware, E2E, transport or metrics claim.

## WAITING_ON
1. Next ChatGPT/roadmap slice assignment on RemoteOps #1208.
2. Dell/Thor self-hosted runners.
3. LONGCTX-B1 user choices (filler/B-lex/B-attn).

## Measured
None. Status stays **NOT_MEASURED**.

## Out of scope
LONGCTX-B1 campaign. Claude ternary/hybrid. NVIDIA stack. CCOS recall is not a 1M attention window. RemoteOps M2 ACK journal work.
