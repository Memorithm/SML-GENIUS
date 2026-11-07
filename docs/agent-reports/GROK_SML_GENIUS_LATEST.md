# Grok SML-GENIUS report — 2026-11-07T14:22:31Z

- PR: https://github.com/Memorithm/SML-GENIUS/pull/850

- Status: **IN_PROGRESS (CONTEXT-2 G4 snapshot wire format: PR #850 open; exact-head CI pending)** / RemoteOps **NOT_CONNECTED**; E2E `sml-genius-20261103T191749Z-8152791e` **NOT_QUALIFIED**. Metrics **NOT_MEASURED**.
- Base: `30cd5d4b04bc42d56a167ea97207d19995251508` (origin/main after #848). Not stacked on #849 (G3 adversarial restore corpus, left untouched). Milestone **G4 CONTEXT-2 serialization/admission boundary**. **LONGCTX-B1** untouched / **BLOCKED**.
- Wake source: Grok Bot executor (parent-delegated handoff from ChatGPT RemoteOps #1208 comment 6707906678) — **not** proof of RemoteOps→Grok loop CONNECTED.
- Branch: `grok/g4-context2-snapshot-wire-20261107`; code commit `a23d5b0ccf9409c0ceb06bddea26f03099fade90`.
- **NO MERGE** until review.

## This slice
New module `crates/sml-store/src/working_context_wire.rs` (`#![forbid(unsafe_code)]` crate):
- Versioned canonical LE byte format: magic `SWC2`, wire v1, schema v1; 93-byte header, 74-byte rows, 8-byte seen ids.
- Limits before allocation: `MAX_SNAPSHOT_BYTES` = 5 374 045 checked before reading; `MAX_SNAPSHOT_ROWS` = 65 536 for R+E and S (checked arithmetic); retained ≤ policy limit; exact implied length == input length before any `Vec` is sized. Caller can tighten via `SnapshotWireLimits::new`.
- Rejects: oversized length prefixes (incl. u32::MAX), truncation (header/mid-row/mid-seen/every prefix), trailing bytes, bad magic, unknown wire version, unsupported schema, zero stream/segment ids, unknown policy/fate tags, noncanonical resolved encoding, noncanonical ordering.
- Encoder refuses noncanonical snapshots ⇒ `encode(decode(b)) == b` for every accepted input.
- `decode_and_restore` / `restore_into`: semantic `WorkingContext::restore` only after full wire decode; rejected bytes never mutate the live context.
- Docs: `docs/SML-CONTEXT-2-WORKING.md` § Snapshot wire format (G4) + module docs (field table, limits, non-claims).

Tests: `crates/sml-store/tests/working_context_wire.rs` — **30 tests** (written first; failed to compile against base = red), incl. independent reference writer, roundtrips for Fixed/Expanding/CapacityBounded with and without evictions, restart-through-bytes vs uninterrupted run, negative corpus, reject-without-mutation over 13 corpus entries × 3 policies, exhaustive single-byte mutation sweep (accepted ⇒ canonical).

## Evidence (local, code commit `a23d5b0ccf9409c0ceb06bddea26f03099fade90`)
- `bash scripts/install-hooks.sh` applied (core.hooksPath=.githooks).
- `cargo fmt --all -- --check` clean.
- `cargo clippy --locked --workspace --all-targets -- -D warnings` pass.
- `cargo test --locked --workspace` → **2324 passed; 0 failed; 3 ignored** (main baseline after #848: 2294; +30).
- Exact-head CI: PENDING / NOT_QUALIFIED at report time.

## WAITING_ON
1. PR open against main + exact-head required CI SUCCESS.
2. Review (ChatGPT / human) on RemoteOps #1208 before any merge ask.
3. Dell/Thor self-hosted runners `total_count=0`.
4. LONGCTX-B1 prerequisites (filler/B-lex/B-attn) remain user-blocked.

## Measured
None. Keep **NOT_MEASURED** (no throughput or quality metric).

## Out of scope this slice
#849 adversarial restore corpus (not duplicated/rewritten). RemoteOps M2 #1346. LONGCTX-B1 campaign. Claude hybrid/ternary paths. NVIDIA stack. No checksum/signature (non-claim).
