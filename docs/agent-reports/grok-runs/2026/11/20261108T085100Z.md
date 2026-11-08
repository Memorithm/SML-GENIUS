# Grok SML-GENIUS report: 2026-11-08T08:51:00Z

- PR: https://github.com/Memorithm/SML-GENIUS/pull/877 (MERGED, squash `603152c8040127953b2df0f3e8fb3f8d19c528d6`, 2026-11-08T08:50:35Z)
- Status: **G13 MERGED**. CONTEXT-2 adversarial differential oracle for G12 provenance policy. RemoteOps **NOT_CONNECTED**. Metrics **NOT_MEASURED**. Native-quality qualification **NOT_QUALIFIED**.
- Base: main `7b40403003fe11ceaec112b2463c948dabbb4b0f`. Exact PR head: `46d2574d5f15ec66435a6596ba508bdcdaec091a` (code/test commit = head).
- Branch: `grok/g13-provenance-adversarial-oracle-20261108`. Assigned by ChatGPT on RemoteOps #1208 (comment 6718994823). **LONGCTX-B1** untouched and **BLOCKED**. RemoteOps M2 not touched. Open PRs #850, #864 and #865 left alone.
- Wake source: Grok Bot G13 executor (in-agent). It does **not** show the RemoteOps→Grok loop is CONNECTED.

## G12 reconciliation
G12 code PR #875 is **MERGED** (squash `6e13f82f`, exact head `52f1708c0a5bb7ab8e06bfdf78d6a8f0a0b3ecd9`, six required checks SUCCESS). G12 report PR #876 is **MERGED** (`7b404030`, exact head of that report).

## What changed in G13
- No production logic change. Test-only addition: `crates/sml-store/tests/working_context_anchor_provenance_adversarial_oracle.rs`.
- Independent pure oracle (`oracle_admit_provenance`, `oracle_admit`, `oracle_advance`, `oracle_admit_with_provenance`, `oracle_derived_binding`) lives only in the test file and does not call production `admit_provenance`, `admit_with_provenance`, `derived_binding`, or `sml_store::working_context_anchor::{admit, advance}`. Production results are classified and compared row-by-row.
- Versioned frozen vectors (`ANCHOR_PROVENANCE_POLICY_VERSION = 1`): 18 named exemplars plus a cartesian expansion over stream IDs `{0, 1, STREAM, u64::MAX}`, three authority tags (including a 31-byte prefix near-collision) and digest domains `{SWE1, SWE2, all-zero}`, under both `Unbound` and `Bound` policies.
- Coverage: stream-id boundaries and cross-stream pairs; authority-id prefix collisions; digest-domain confusion including prefix-sharing tags (`SWE1` vs `SWE2`) and the all-zero domain; absent presented identity and malformed derived bindings; generation overflow (`advance` at `u64::MAX`) and G12-before-G10 order at generations `0` and `u64::MAX`; cross-stream snapshot substitution on live SWE1 envelopes.
- Every rejected restore leaves encoding and `stats.admitted` unchanged.
- Explicit demonstration that a caller-constructed matching `AuthorityId` is an identity binding (equality tag), **not** an authenticated origin.

## Evidence
- Pre-push hook on `46d2574d5f15ec66435a6596ba508bdcdaec091a`, no `--no-verify`: fmt, clippy `--locked -D warnings`, and workspace tests passed.
- `cargo test --locked --workspace` on head `46d2574d5f15ec66435a6596ba508bdcdaec091a`: **2457 passed, 0 failed, 3 ignored**.
- New suite: 8 passed in `working_context_anchor_provenance_adversarial_oracle`.
- Exact-head CI on `46d2574d5f15ec66435a6596ba508bdcdaec091a`, all SUCCESS: fmt, rust, g1-realtext-smoke, owner-admission, primary-provider-owner-review, icm-feature.
- RemoteOps `ops/grok/grok_report_gate.py` validates the status/report schema only. The Grok invocation is NOT independently attested.

## Limitations
- Anchors and authority tags in tests are mocks constructed in-process. `AuthorityId` is a caller assertion compared for equality; forged matching bytes are accepted. Anchor attestation, provisioning, authentication and hardware-backed storage are **NOT QUALIFIED**.
- SWE1 stays unkeyed SHA-256: corruption detection, not authentication. The digest-domain tag is domain separation, not a signature scheme.
- No hardware, E2E, transport or metrics claim.

## WAITING_ON
1. Next ChatGPT/roadmap slice assignment on RemoteOps #1208.
2. Dell/Thor self-hosted runners.
3. LONGCTX-B1 user choices (filler/B-lex/B-attn).

## Measured
None. Status stays **NOT_MEASURED**.

## Out of scope
LONGCTX-B1 campaign. Claude ternary/hybrid. NVIDIA stack. CCOS recall is not a 1M attention window. RemoteOps M2 ACK journal work. Open PRs #850, #864 and #865.
