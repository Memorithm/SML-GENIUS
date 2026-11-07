# Grok SML-GENIUS report — 2026-11-07T15:48:10Z

- PR: https://github.com/Memorithm/SML-GENIUS/pull/850 (contains #849 G3 via merge commit)
- Status: **IN_PROGRESS (G3+G4 integration; exact-head CI SUCCESS; awaiting ChatGPT integration review)** / RemoteOps **NOT_CONNECTED**; E2E `sml-genius-20261103T191749Z-8152791e` **NOT_QUALIFIED**. Metrics **NOT_MEASURED**. **LONGCTX-B1** untouched / **BLOCKED**. Qualification **NOT_QUALIFIED** (no merge).
- Base: `30cd5d4b04bc42d56a167ea97207d19995251508` (origin/main after #848).
- Tip: `77b1253ae851840d44dd414a46965b25ff9ec2c3` on `grok/g4-context2-snapshot-wire-20261107`.
  - Code/test head: `55a8a6b570e5d0313d5a82a24dbfb55f451b377e`
  - Merge commit `bc16148ec846d7cfd46bc91e17ba9da9fa2fa17e` = G4 tip `f42117be` + G3 tip `37c77076` (G3 code `9ad178a1`, G4 code `a23d5b0c`).
- Wake source: Grok Bot routine `sml-genius-dev-watchdog-30m` (2026-11-07T15:37:29Z fire) — **not** proof of RemoteOps→Grok CONNECTED.
- **NO MERGE** of #849/#850 to main. G5 golden harness **queued** until ChatGPT integration review.

## Exact-head CI (new this wake)
Tip `77b1253ae851840d44dd414a46965b25ff9ec2c3` required checks all **COMPLETED/SUCCESS** (incomplete=[]; fail=[]; mergeState **CLEAN**, mergeable):
- **fmt** SUCCESS (18s)
- **rust** SUCCESS (23m41s, job 128443214604, run https://github.com/Memorithm/SML-GENIUS/actions/runs/42385889478)
- **g1-realtext-smoke** SUCCESS
- **owner-admission** SUCCESS
- **primary-provider-owner-review** SUCCESS
- **icm-feature** SUCCESS

## Prior integration slice (unchanged)
`working_context_wire_restore.rs` (10 tests) + G3 adversarial (36) + G4 wire (30). Local author evidence: fmt/clippy/2370 workspace tests. Synthetic only.

## WAITING_ON
1. ChatGPT integration review on RemoteOps #1208 (delivery 6708914838) before any merge ask.
2. Dell/Thor self-hosted runners `total_count=0`.
3. LONGCTX-B1 prerequisites (filler/B-lex/B-attn) remain user-blocked.
# Grok SML-GENIUS report: 2026-11-07T13:23:49Z

- PR: https://github.com/Memorithm/SML-GENIUS/pull/849
- Status: **IN_PROGRESS**. G3 CONTEXT-2 adversarial snapshot corpus: PR open, exact-head CI pending. RemoteOps **NOT_CONNECTED**. E2E `sml-genius-20261103T191749Z-8152791e` **NOT_QUALIFIED**. Metrics **NOT_MEASURED**.
- Base: `30cd5d4b04bc42d56a167ea97207d19995251508` (origin/main after #848). Code commit: `9ad178a10e0e41533d88f678d1c4cb64fa7814ca`. The tip is the docs-only commit containing this report.
- Branch: `grok/g3-context2-adversarial-snapshots-20261107`. Milestone **G3 CONTEXT-2**. **LONGCTX-B1** is untouched and **BLOCKED**.
- Handoffs: RemoteOps #1208 comments 6706636166 and 6707274787. This wake came from the Grok Bot executor. It does **not** show the RemoteOps→Grok loop is CONNECTED.
- **NO MERGE** until ChatGPT qualifies it.

## Hardening (TDD, red before fix)
`WorkingContext::restore` now fails closed on:
1. stats counter overflow: `StatsMismatch` (was `InternalInvariant`);
2. `recurrent_folds != admitted`: `StatsMismatch` (was accepted);
3. `segment_id == 0` in snapshot rows: `InvalidSegmentIdentity` (was accepted).

`restore` is still pure, so a rejected restore leaves no partial live state.

## Corpus
New file `crates/sml-store/tests/working_context_adversarial.rs` with 36 tests. It uses 48 seeds, streams of at most 12 segments, and covers all three policies. Each fault family has at least one dedicated negative test:
- truncated
- oversized
- duplicate step/coverage
- missing-step gaps
- out-of-order
- overflow
- policy/fate mismatch
- fingerprint=0 and zero id

It also covers restart/replay determinism, fresh-vs-restored next-eviction parity and Expanding ceiling parity. The full test list is in the PR body.

## Evidence (local, code `9ad178a1`)
- `bash scripts/install-hooks.sh` applied. No `--no-verify`.
- `cargo fmt --all -- --check`: pass.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: pass.
- `cargo test --locked --workspace`: **2330 passed, 0 failed, 3 ignored** (baseline 2294, plus 36).
- Exact-head CI: pending.
- RemoteOps `ops/grok/grok_report_gate.py` (RemoteOps origin/main 4cc19a5): VALIDATED, but only for the status/report schema. The Grok invocation is NOT independently attested.

## WAITING_ON
1. Exact-head required CI SUCCESS.
2. ChatGPT qualification on #1208.
3. Dell/Thor self-hosted runners (`total_count=0`).
4. LONGCTX-B1 user choices (filler/B-lex/B-attn).

## Measured
None. Status stays **NOT_MEASURED**.

## Out of scope this wake
G5 golden harness (queued). Merges to main. LONGCTX-B1 campaign. Claude hybrid/ternary. NVIDIA stack. Research PRs #851/#852 (bpb3) left alone.
## Out of scope
LONGCTX-B1 campaign. ChatGPT M2 RemoteOps #1344. Claude ternary/hybrid. NVIDIA stack. CCOS recall is not a 1M attention window.
