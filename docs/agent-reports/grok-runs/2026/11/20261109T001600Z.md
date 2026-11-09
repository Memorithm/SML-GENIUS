# Grok SML-GENIUS report: 2026-11-09T00:16:00Z

- PR: https://github.com/Memorithm/SML-GENIUS/pull/898 (**MERGED** by ChatGPT after qualification: squash `485a51a334cd45af1fc3c2a02589016a0e7fa9d5`, 2026-11-09T00:56:15Z, exact head `84edfa38bbdac9fe54bc51db3e32f259f571680b`)
- Status: **G19 code MERGED** (ChatGPT ACK and merge in RemoteOps #1208 comment 6729688978; Codex review on `84edfa38` completed with no findings). These are CONTEXT-2 adversarial drift negatives against the frozen G18 permutation/transcript guard and the G17 corpus. RemoteOps **NOT_CONNECTED**. Metrics **NOT_MEASURED**. Qualification **NOT_QUALIFIED**.
- Base: main `f9adf172014c20f87c0a07b40b9003e9e5a8fb11`. Branch: `grok/g19-g18-drift-negatives-20261108`. Assigned by ChatGPT on RemoteOps #1208 (comment 6728339742). **LONGCTX-B1** is untouched and still **BLOCKED**. RemoteOps M2 was not touched.
- Wake source: the Grok Bot watchdog routine (in-agent). It does **not** show that the RemoteOps→Grok loop is CONNECTED.

## G18 reconciliation
G18 code #895 merged as `3ace36f9`. G18 report #896 merged as `f9adf172` (exact head `0f0c3730`, 2026-11-08T22:16:33Z), and ChatGPT acknowledged it in 6728339742. The earlier Codex review on #896 failed on usage limits, so #896 still has **no successful independent review**. That limitation stays open.

## What changed in G19
- The change is tests-only: `crates/sml-store/tests/working_context_anchor_g18_drift_negatives.rs`, with 4 tests. The 5 G17 tests also re-run in the same binary. No G17 or G18 file or fixture changed. G17 is read through its existing `shared` module.
- The G18 renderers (SplitMix64 permutations, permutation rendering, transcript) are re-implemented. They count as the frozen guard only because, on the unmutated control, they reproduce the frozen G18 digests `b2b50f6c27f14fd4f3009ade800cf6fc7aa3633fb810b837a7564bce443edf4f` (permutations) and `de2098e90bc23b72ecc839d60835ddb966f390014ef2770d1df66574658deaa1` (transcript), plus the G17 corpus digest `15ceba9cdb3ed42f92acd73bdf2208dcd7050298bd3c489a7f5a64ee0b298bb8`.
- Each test applies one independent mutation. The detector sets below are asserted exactly:

| mutation | detectors that reject it |
|---|---|
| `unmutated_positive_control` | none |
| `corpus_row_bytes_flip` | `CorpusDigest` |
| `corpus_row_omitted` | `CorpusDigest`, `PermutationShape`, `PermutationDigest`, `TranscriptDigest` |
| `permutation_index_overwrite` | `PermutationShape`, `PermutationDigest`, `TranscriptDigest` |
| `permutation_index_swap` | `PermutationDigest`, `TranscriptDigest` |
| `duplicate_permutation` | `PermutationDistinct`, `PermutationDigest`, `TranscriptDigest` |
| `typed_error_expectation` | `OracleAgreement` |
| `repeated_positive_control` | `PermutationShape`, `PermutationDigest`, `TranscriptDigest` |

- The matrix rendering (version 1) is pinned at SHA-256 `fc9dacbbb9569e0b7cc43919554d14dfd63cb98f2c1824776e250dafce11c9ff`. For each mutation it records the detector set, the corpus, permutation and transcript digests, and the number of production rejections checked. That count is 96 per scenario, 88 for the omitted row and 95 for the repeated control.
- On every production rejection in every replay, SWC2, SWE1 and stats are preserved byte-for-byte, and the anchor is never advanced (G17's adapter asserts this).
- **Negative result kept**: `StateInvariant` rejects none of the mutations. A repeated positive control is idempotent on the live target, so only the frozen permutation and transcript pins catch it.
- Production change: none. No defect was exposed.

## Evidence
- Local run on the Grok box, commit `84edfa38bbdac9fe54bc51db3e32f259f571680b`, with the pre-push hook and no `--no-verify`:
  - `cargo fmt --all -- --check` OK
  - `cargo clippy --locked --workspace --all-targets -- -D warnings` OK
  - `cargo test --locked --workspace`: **2495 passed, 0 failed**
- Exact-head CI on `84edfa38bbdac9fe54bc51db3e32f259f571680b`: all six required checks SUCCESS (fmt, rust job 128910215794, g1-realtext-smoke, owner-admission, primary-provider-owner-review, icm-feature). Workflow runs: CI 42539195510, G1 realtext 42539196039, Trusted RSI 42539196020, External weight admission 42539196008, grok-icm 42539195900.
- Local tests are Grok-reported and were not independently reproduced.
- RemoteOps `ops/grok/grok_report_gate.py` validates the status/report schema only. The Grok invocation is NOT independently attested.

## Limitations
- Anchors and authority tags are mocks. The authority tag is an equality binding, not authenticated provenance. SWE1 is unkeyed SHA-256.
- These are harness-level guards, not production gates.
- There is no hardware, E2E, transport, remote-execution, attestation or metrics claim.

## WAITING_ON
1. ChatGPT qualification of this report PR (do not merge before). G20 (paired-perturbation matrix completeness, assigned in 6729688978) starts after this reconciliation.
2. The unresolved independent review of #896.
3. Dell/Thor self-hosted runners.
4. LONGCTX-B1 user choices (filler/B-lex/B-attn).

## Measured
None. Status stays **NOT_MEASURED**.

## Out of scope
- The LONGCTX-B1 campaign.
- Claude's ternary/hybrid architecture.
- The NVIDIA stack.
- CCOS recall, which is not a 1M attention window.
- RemoteOps M2 work.
