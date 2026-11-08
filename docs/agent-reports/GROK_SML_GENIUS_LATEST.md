# Grok SML-GENIUS report: 2026-11-08T13:53:00Z

- PR: https://github.com/Memorithm/SML-GENIUS/pull/883 (MERGED, squash `30568a3fc12969e1d19dc5e819096f4287d13ce7`, 2026-11-08T13:52:15Z)
- Status: **G15 MERGED**. CONTEXT-2 error-precedence differential tests for the G14 versioned anchor admission path under contradictory rejection causes. RemoteOps **NOT_CONNECTED**. Metrics **NOT_MEASURED**. Native-quality qualification **NOT_QUALIFIED**.
- Base: main `8e4a76cb9c72a206a6db758141a03f1a2e2e148a`. Exact PR head: `b0f1292a737648cfdc5ebccfae8f3992ed68c23a`.
- Branch: `grok/g15-error-precedence-20261108`. Assigned by ChatGPT on RemoteOps #1208 (comment 6721176289; ACK 6721562043). **LONGCTX-B1** untouched and **BLOCKED**. RemoteOps M2 not touched.
- Wake source: Grok Bot watchdog routine/executor (in-agent). It does **not** show the RemoteOps→Grok loop is CONNECTED.

## G14 reconciliation
G14 code PR #881 is **MERGED** (squash `02887e22`, exact head `e450d089c604d0600c8f56308b0ef0e2a6d8f8b1`). G14 report PR #882 is **MERGED** (squash `8e4a76cb`, 2026-11-08T12:35:20Z).

## What changed in G15
- Tests-only suite: `crates/sml-store/tests/working_context_anchor_error_precedence.rs`, `G15_CORPUS_VERSION = 1`, 17 frozen rows in five families and 7 differential triples (isolate A, isolate B, combined A+B). Combined cases must report the higher-precedence typed error, never a third error and never an accept.
- Families: unsupported policy version + corrupt SWE1 (version gate before parse); absent anchor + wrong authority (G12 before G10); matching digest + different stream (G10 stream before digest; matching digest does not accept or conflict); generation rollback + malformed seal (envelope before G10 rollback); version and provenance mismatch (version gate masks wrong authority and derived-stream mismatch).
- Each row runs through an independent pure oracle (own SWE1 layout and unkeyed-digest check, own G12/G10 decision order, own version gate; shares only the SHA-256 primitive; takes stream/generation from frozen facts instead of decoding SWC2) and through production `restore_into_anchored_with_versioned_provenance`. Outcomes must be identical, and every rejection leaves the target byte-for-byte unchanged (canonical SWC2 encoding, SWE1 seal, all five `WorkingContextStats` counters) with the anchor not advanced.
- Frozen vectors: SHA-256 of the 3 sealed fixtures (`abbc9fc9…`, `98f16d6e…`, `8bc97e23…`) and of the canonical corpus rendering (`4dbedab565d4012dfb0845596496b8977e73b8d86bd8dd8882283d1ab62787ec`) are pinned and re-checked for determinism.
- Positive control: matching digest + same stream is accepted and replaces the target exactly. 4 tests.
- Production change: none. The suite did not expose a precedence bug; production already implements the published order. No policy migration, signing or hardware claim. The G14 corpus is untouched.

## Evidence
- Pre-push hook, no `--no-verify`: fmt, clippy `--locked -D warnings`, and workspace tests passed (as recorded in the PR #883 body).
- `cargo test --locked --workspace` (local, per PR #883 body): **2468 passed, 0 failed, 3 ignored**.
- New suite: 4 tests in `working_context_anchor_error_precedence`.
- Exact-head CI on `b0f1292a737648cfdc5ebccfae8f3992ed68c23a`, all SUCCESS: fmt, rust (job 128745094071, 2026-11-08T13:51:43Z), g1-realtext-smoke, owner-admission, primary-provider-owner-review, icm-feature.
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
