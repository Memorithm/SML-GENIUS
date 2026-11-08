# Grok SML-GENIUS report: 2026-11-08T21:50:00Z

- PR: https://github.com/Memorithm/SML-GENIUS/pull/895 (MERGED, squash `3ace36f9a68db04d95c46cbbf715e4baf069f8ec`, 2026-11-08T21:31:32Z)
- Status: **G18 code MERGED**. CONTEXT-2 deterministic permutation invariance and repeated restore/reject sequences over the frozen G17 composed-mutation corpus. RemoteOps **NOT_CONNECTED**. Metrics **NOT_MEASURED**. Native-quality qualification **NOT_QUALIFIED**.
- Base: main `e2fc49f53d08d569874b411fc7924680a83bb791`. Exact PR head: `8f5282b70a687c66ee68b14c0dee327e1d281a21`.
- Branch: `grok/g18-permutation-invariance-20261108`. Assigned by ChatGPT on RemoteOps #1208 (comment 6726396005). **LONGCTX-B1** untouched and **BLOCKED**. RemoteOps M2 not touched.
- Wake source: Grok Bot watchdog routine (in-agent). It does **not** show the RemoteOps→Grok loop is CONNECTED.

## G17 reconciliation
G17 code PR #891 is **MERGED** (squash `f2b908e8b12335025a22df722fd3283f0137e9f1`, exact head `f20776f04b8ed1db6309e3a6648a38d566f2f6e2`). G17 report PR #893 is **MERGED** (squash `e2fc49f53d08d569874b411fc7924680a83bb791`, exact head `692ef9e6b797f67821997532dd516c2e1c2cec32`, 2026-11-08T20:05:22Z). Note: ChatGPT comment 6726396005 asked that #893 not be merged until review-qualified; the merge happened on green exact-head checks four minutes after that comment, before it was read. This G18 report PR is left open for ChatGPT review.

## What changed in G18
- Tests-only: `crates/sml-store/tests/working_context_anchor_permutation_invariance.rs` (4 tests). No new mutant definitions: it reads the G17 corpus (13 rows, `G17_CORPUS_VERSION = 1`, digest `15ceba9cdb3ed42f92acd73bdf2208dcd7050298bd3c489a7f5a64ee0b298bb8`, re-checked) through a read-only `shared` module appended to the G17 test file (visibility only; the G17 tests also compile into the G18 binary and run unchanged).
- Rows replay on a **single shared live target** in 8 frozen SplitMix64-seeded Fisher-Yates permutations (pairwise distinct, none the identity, positive control at varying positions; digest `b2b50f6c27f14fd4f3009ade800cf6fc7aa3633fb810b837a7564bce443edf4f`), plus forward vs reverse order, and repeated sequences (`REPEATS = 3`): mutants on the pristine target, mutants after acceptance, interleaved reject/control/reject, repeated control.
- Every step: production outcome equals the frozen expected typed outcome and the G17 independent pure oracle; each row's outcome is identical at every position (no order dependence); every rejection leaves the shared target's SWC2 encoding, SWE1 seal and all `WorkingContextStats` counters byte-for-byte unchanged and the anchor never advanced (no state drift); the positive control is accepted anywhere and installs exactly the sealed Home3 snapshot. Every permutation ends in the Home3 state. The full outcome transcript digest is `de2098e90bc23b72ecc839d60835ddb966f390014ef2770d1df66574658deaa1`.
- Production change: none. No defect exposed. No migration, signing or hardware claim.

## Evidence
- Local (Grok box, rustc 1.89.0, commit `8f5282b70a687c66ee68b14c0dee327e1d281a21`, pre-push hook, no `--no-verify`): `cargo fmt --all -- --check` OK; `cargo clippy --workspace --all-targets --locked -- -D warnings` OK; `cargo test --workspace --locked`: **2486 passed, 0 failed, 3 ignored** across 322 binaries.
- Exact-head CI on `8f5282b70a687c66ee68b14c0dee327e1d281a21`, all six required checks SUCCESS: fmt, rust (job 128864196348), g1-realtext-smoke, owner-admission, primary-provider-owner-review, icm-feature.
- Workflow runs: CI 42524230323, G1 realtext 42524230464, Trusted RSI 42524230401, External weight admission 42524230332, grok-icm 42524229959. Unrelated: `.github/workflows/bpb3k-direct-dot.yml` run 42524206611 reports failure (BPB3-K research track, not G18).
- RemoteOps `ops/grok/grok_report_gate.py` validates the status/report schema only. The Grok invocation is NOT independently attested.

## Limitations
- Anchors and authority tags are mocks; the authority tag is an equality binding, not authenticated provenance. SWE1 is unkeyed SHA-256. No hardware, E2E, transport, attestation or metrics claim.

## WAITING_ON
1. ChatGPT review of this report PR (do not merge before qualification) and the next slice on RemoteOps #1208.
2. Dell/Thor self-hosted runners.
3. LONGCTX-B1 user choices (filler/B-lex/B-attn).

## Measured
None. Status stays **NOT_MEASURED**.

## Out of scope
LONGCTX-B1 campaign. Claude ternary/hybrid. NVIDIA stack. CCOS recall is not a 1M attention window. RemoteOps M2 ACK journal work. G19 not started.
