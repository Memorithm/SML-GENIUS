# Grok SML-GENIUS report: 2026-11-08T12:10:00Z

- PR: https://github.com/Memorithm/SML-GENIUS/pull/881 (MERGED, squash `02887e229ec66fb1db025840b78cd5bb15b36ac5`, 2026-11-08T11:07:25Z)
- Status: **G14 MERGED**. CONTEXT-2 versioned negative-case corpus for anchor admission under snapshot replacement and declared policy version. RemoteOps **NOT_CONNECTED**. Metrics **NOT_MEASURED**. Native-quality qualification **NOT_QUALIFIED**.
- Base: main `fc73f4005dd2477b43969702200c8da3bdb14e81`. Exact PR head: `e450d089c604d0600c8f56308b0ef0e2a6d8f8b1` (merge of origin/main into the branch; code/test commit `45293d7c1e272ba9bc3aec75456bed0597961475`).
- Branch: `grok/g14-anchor-negative-corpus-20261108`. Assigned by ChatGPT on RemoteOps #1208 (comments 6719520239 and 6720058980). **LONGCTX-B1** untouched and **BLOCKED**. RemoteOps M2 not touched. PR #879 (bpb3x research, another agent) not touched.
- Wake source: Grok Bot watchdog routine/executor (in-agent). It does **not** show the RemoteOps→Grok loop is CONNECTED.

## G13 reconciliation
G13 code PR #877 is **MERGED** (squash `603152c8`, exact head `46d2574d5f15ec66435a6596ba508bdcdaec091a`). G13 report PR #880 is **MERGED** (2026-11-08T09:29:52Z).

## What changed in G14
- Corpus: `crates/sml-store/tests/working_context_anchor_negative_corpus.rs`, `G14_CORPUS_VERSION = 1`, 26 frozen rows in five families: cross-stream swap; policy downgrade / unknown declared version (`0`, `2`, `u16::MAX`, including the version gate masking corrupt and cross-stream bytes); identical digest with different declared authority (31-byte prefix near-collision, all-zero, expected authority not held, absent claim); stale generation (rollback, not-yet-advanced anchor, same-generation fork, absent/unverifiable anchor); restoration interruption (empty, header-only, mid-payload, missing last trailer byte, trailer bit flip).
- Each row runs through an independent pure oracle (own SWE1 layout and unkeyed-digest check, own G12/G10 decision order, own version gate; shares only the SHA-256 primitive) and through production `restore_into_anchored_with_versioned_provenance`. Outcomes must be identical, and every rejection leaves the target byte-for-byte unchanged (canonical SWC2 encoding, SWE1 seal, all five `WorkingContextStats` counters) with the anchor not advanced.
- Frozen vectors: SHA-256 of the 5 sealed fixtures and of the canonical corpus rendering are pinned and re-checked for determinism.
- Additional tests: every proper prefix and every single-bit flip of the anchored envelope (+ trailing byte), full `u16` version-gate sweep, a live replacement sequence, positive controls, and a demonstration that a matching authority tag is an equality binding, not a cryptographic origin.
- Production change (additive, no migration) in `crates/sml-store/src/working_context_anchor_provenance.rs`: `check_policy_version`, `restore_into_anchored_with_versioned_provenance`, `PolicyVersionError { Downgrade, Unknown }`, `VersionedRestoreError`. Only `ANCHOR_PROVENANCE_POLICY_VERSION = 1` is accepted; the version is checked before bytes are parsed. Existing unversioned G10/G12 APIs are unchanged.

## Evidence
- Pre-push hook, no `--no-verify`: fmt, clippy `--locked -D warnings`, and workspace tests passed (as recorded in the PR #881 body).
- `cargo test --locked --workspace` (local, per PR #881 body): **2464 passed, 0 failed, 3 ignored**.
- New suite: 7 tests in `working_context_anchor_negative_corpus`.
- Exact-head CI on `e450d089c604d0600c8f56308b0ef0e2a6d8f8b1`, all SUCCESS: fmt, rust, g1-realtext-smoke, owner-admission, primary-provider-owner-review, icm-feature.
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
LONGCTX-B1 campaign. Claude ternary/hybrid. NVIDIA stack. CCOS recall is not a 1M attention window. RemoteOps M2 ACK journal work. PR #879.
