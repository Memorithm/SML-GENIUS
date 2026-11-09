# Grok SML-GENIUS report: 2026-11-09T17:30:00Z

- **PR:** https://github.com/Memorithm/SML-GENIUS/pull/914. It is **OPEN and not merged**, held for ChatGPT protocol review and the owner gate. Grok does not merge it.
  - Code commit: `1a8547404fa4b92aa1e39871acffd6b6f9fb0db1`. This report is committed on top of it in the same PR. The final exact head is posted on RemoteOps #1208.
  - Base: main `af6a9c56e70dc80b9f17157bdfe6c2a563494053`.
  - Branch: `grok/ctx2-infer-score`.
  - Stacked on #909's head `eff41fab52196809139bd8b7646f0712a1de6450`, then merged with main, so the diff includes #909's two prereg docs. **Do not merge before #909.**
- **Status:** CTX2-INFER step 2 delivered. It is a model-free Rust plan/audit/score substrate. RemoteOps→Grok loop **NOT_CONNECTED**. Metrics **NOT_MEASURED**. Anchors **NOT_QUALIFIED** (mock). Qualification **NOT_QUALIFIED**.
- **Commitment:** Grok committed to this step on RemoteOps #1208 (6746179009), following checkpoint 6746153203.
- **Untouched:**
  - #909 itself; it stays unmerged.
  - The CONTEXT-2 substrate (G2 #845, SWC2/SWE1/anchor).
  - BPB3, the `crypto_universe` files, and RemoteOps M2 (#1430, which is ChatGPT's).
  - LONGCTX-B1 stays **BLOCKED**. There is no G24 and no mutation-matrix series.
- **Wake source:** Grok Bot, delegated in-agent. It does **not** show that the RemoteOps→Grok loop is CONNECTED.

## What changed
- New `crates/sml-store/src/ctx2_infer.rs` and `crates/sml-store/src/ctx2_infer/`, with `#![forbid(unsafe_code)]` and no new dependency. The submodules are:
  - `generator`: CTX2-FACTS-v1;
  - `plan`: arms A0/A1/A2/A3/A3r/R, CONTEXT-2 prompt assembly through the CONTEXT-1a raw store, prompt and plan SHA-256, and `CallLedger`;
  - `restart`: A3r seal, drop and anchored restore;
  - `audit`: the exact token-ID budget audit;
  - `compute`: the compute envelope;
  - `score`: EM, Wilson, one-sided exact McNemar, Holm, the session-clustered bootstrap, and strict H2.
- New `crates/sml-store/tests/ctx2_infer.rs` with 29 model-free tests.
- New `docs/research/CTX2_INFER_PLAN_SCORE.md`. It covers the freeze, the plan interpretations of details the prereg leaves open, the coverage, and what is deferred.

## Pins
- **Prereg:** `docs/research/CTX2_INFER_PREREG.md`.
  - SHA-256 `e26dfe16f7222b297d47cd02d74442868de8e7ea58388e4f45e87d8a7d51bea1`.
  - Content commit `ecec94b4a7bc12ca1b83c454475d157b025b1f37`; PR #909 head `eff41fab`.
  - A test hashes the checked-in bytes, so any in-place edit fails CI.
- **Test seeds:** `SHA-256("CTX2-INFER-test-v1" || merge_sha_hex40 || u8(i))`. They **fail closed** with `PreregNotMerged` until #909's merge SHA is attested. None was derived.
- **Dev seeds:** `0x3fc3d3ad6bd3e87a` and `0x13f3d0a2cfece573`. The derivation is a plan interpretation, open to review.
- **Regression pins of this code** (not measurements):
  - dev-seed-0 session file `f16eb9847eddf8ebf5cc936c54bb9e3b02a1a311bd0cd13ea84739b29eef5ba4`;
  - smoke plan (84 calls) digest `864054b5f9de9295eb774b2c6e62a11a88f14feeef84a3dc3d3814898066a8a7`.

## What the tests check (structural, model-free)
- **Token-ID budget audit (prereg section 4.1).** The audit runs on supplied token arrays; the test tokenizers are fixtures, not the model tokenizer. It checks:
  - `|H| = B(s, k) = |H_A3|` for the strong no-CONTEXT-2 baselines A1 (token truncation) and A2 (sliding window), and for A3r;
  - an empty `H` for A0;
  - identical `T_pre` and `T_post` across arms;
  - `|P| + n_predict ≤ n_ctx` and the server-reported count;
  - exactly one call per planned key, and token sources bound to the planned history SHA-256;
  - equal `Σ|P|` per `(k, seed)`.

  Each violation has its own negative test.
- **Structural invariants on the 27 648-call grid.** The grid is built only with a clearly synthetic merge SHA, to check its size.
  - H1: A3 = A2 prompt bytes, 5 760 / 5 760.
  - H2p: A3r = A3, 5 760 / 5 760.
  - H3a: 0 gold leaks on evicted queries. The gold is present when `d ≤ k`, which is the positive control.
- **Restart controls.** These are refused at the expected turn with no prompt produced:
  - a fixed per-seed byte flip;
  - truncation;
  - rollback;
  - an absent anchor and an unverifiable anchor;
  - an envelope from a foreign stream.

  Every single-byte envelope flip is refused, and the live context's snapshot stays byte-identical.
- **Raw-store tamper.** A corrupted, missing or swapped artifact fails assembly closed.
- **Compute envelope.** It is derived and equals the prereg: 34 560 naive calls, 26 880 test, 768 precheck, 27 648 on the admitted host and 84 smoke. The ceiling is 72 h, with 14 h per seed and 2 h for the precheck. The ledger refuses unplanned and duplicate calls, allows one retry, and fails an incomplete finish.
- **Scorer.** It matches hand-computed values, for example McNemar `(5,0) = 1/32`, `(3,1) = 5/16`, `(10,10) = 616666/2^20`, and `b + c = 0 → 1`. Holm, Wilson and the strict-H2 truth table are also checked.

## Evidence
- **Local run** on the Grok box, on the tree of code commit `1a854740`:
  - `bash scripts/install-hooks.sh`: hooks enabled
  - `cargo fmt --all -- --check`: OK
  - `cargo clippy --locked --workspace --all-targets -- -D warnings`: OK
  - `cargo test --locked --workspace`: **2585 passed, 0 failed**, 3 ignored
  - The pre-push hook ran on push, with no `--no-verify`.
- **Exact-head CI** was PENDING at report time. The final run IDs and conclusions are posted on RemoteOps #1208.
- RemoteOps `ops/grok/grok_report_gate.py` (main `8b30124`) returns VALIDATED, which covers the status/report schema only. The Grok invocation is NOT independently attested.

## Limitations
- There is no model, runtime, tokenizer, prefill or decode, and no measurement of any kind.
- H0, strict H2, H3b, H4 and E1 need model outputs and are not evaluated.
- The merge SHA is caller-attested (format-checked only). Anchors are mocks. SWE1 is unkeyed SHA-256 and detects corruption only.
- Plan interpretations (seed encoding, dev seeds, turn newline, session construction, tamper position) are listed for ChatGPT review. They can still change before #909 merges and before any seed is derived.

## WAITING_ON
1. Exact-head required CI on this PR.
2. ChatGPT protocol review of #909 and of this PR's plan interpretations.
3. The owner gate and merge of #909 before this PR, then attestation of its merge SHA.
4. Step 3: sml-cli `ctx2-infer plan/score`, a stdlib `driver.py`, and the CI-only smoke workflow.
5. An admitted Dell/Thor host for step 4. A non-CUDA GPU backend is preferred; Thor `thor-ccos-arm64-01` is offline.
6. LONGCTX-B1 user choices (filler/B-lex/B-attn).

## Measured
None. Status stays **NOT_MEASURED**.

## Out of scope
- G24 and any mutation-matrix series.
- The LONGCTX-B1 campaign.
- Claude's ternary/hybrid architecture.
- The NVIDIA stack.
- CCOS recall, which is not a 1M attention window.
- RemoteOps M2 (#1430).
