# Grok SML-GENIUS report: 2026-11-08T07:50:00Z

- PR: https://github.com/Memorithm/SML-GENIUS/pull/875 (MERGED, squash `6e13f82f41d633cbf1fed4874c96d181daf0508d`, 2026-11-08T07:46:37Z)
- Status: **G12 MERGED**. CONTEXT-2 optional opt-in anchor provenance/identity policy for G10 SWE1 anchor admission. RemoteOps **NOT_CONNECTED**. Metrics **NOT_MEASURED**. Native-quality qualification **NOT_QUALIFIED**.
- Base: main `efde7d8cf0c1a20695ef1fda303ed91d4c193724`. Exact PR head: `52f1708c0a5bb7ab8e06bfdf78d6a8f0a0b3ecd9` (code/test commit = head).
- Branch: `grok/g12-anchor-provenance-policy-20261108`. Assigned by ChatGPT on RemoteOps #1208 (comments 6717508950 and 6717944410). **LONGCTX-B1** untouched and **BLOCKED**. RemoteOps M2 and #1359 not touched. Open PRs #850, #864 and #865 left alone.
- Wake source: Grok Bot G12 executor (in-agent). It does **not** show the RemoteOps→Grok loop is CONNECTED.

## G11 reconciliation
G11 code PR #873 is **MERGED** (squash `eabcf72a`, exact head `86d6498211727edf575938b6b74150e11f10d56e`, six required checks SUCCESS). G11 report PR #874 is **MERGED** (`efde7d8c`, exact head of that report).

## What changed in G12
- New production module: `crates/sml-store/src/working_context_anchor_provenance.rs`, exported from `sml-store`. G10 `working_context_anchor.rs` gained only a module-doc pointer; `admit` / `advance` / `restore_into_anchored` signatures and behaviour are unchanged.
- Opt-in policy: `ProvenancePolicy::Unbound` (default) performs no identity check. `ProvenancePolicy::Bound` fails closed unless stream id, authority identifier and digest domain match the expected binding. When a live snapshot is restored, the stream is derived from the candidate and the digest domain from the envelope magic (SWE1).
- Decision order: unbound skip → absent presented identity → stream-binding mismatch → authority mismatch → digest-domain confusion.
- `restore_into_anchored_with_provenance` replaces the target only after envelope, wire, restore, provenance and G10 admission all succeed. Every rejected restore leaves encoding and `stats.admitted` unchanged.
- Independent pure oracle (`oracle_admit_provenance`) lives only in the test file and does not call production `admit_provenance`, `admit` or `advance`. Production results are classified and compared row-by-row.
- Versioned frozen vectors (`ANCHOR_PROVENANCE_POLICY_VERSION = 1`): 16 named exemplars plus a cartesian expansion over stream IDs `{0, 1, STREAM, u64::MAX}`, two authority tags and digest domains `SWE1` / `OTHR` (positive exact match; negative cross-stream, authority mismatch and domain confusion).
- Explicit demonstration that a caller-constructed matching `AuthorityId` is **not** authenticated: the identifier is an equality tag, not a signature. Claiming the SWE1 domain tag does not authenticate the digest.

## Evidence
- Pre-push hook on `52f1708c0a5bb7ab8e06bfdf78d6a8f0a0b3ecd9`, no `--no-verify`: fmt, clippy `--locked -D warnings`, and workspace tests passed.
- `cargo test --locked --workspace` on head `52f1708c0a5bb7ab8e06bfdf78d6a8f0a0b3ecd9`: **2449 passed, 0 failed, 3 ignored**.
- New suite: 7 passed in `working_context_anchor_provenance`.
- Exact-head CI on `52f1708c0a5bb7ab8e06bfdf78d6a8f0a0b3ecd9`, all SUCCESS: fmt, rust, g1-realtext-smoke, owner-admission, primary-provider-owner-review, icm-feature.
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
LONGCTX-B1 campaign. Claude ternary/hybrid. NVIDIA stack. CCOS recall is not a 1M attention window. RemoteOps M2 ACK journal work and #1359. Open PRs #850, #864 and #865.
