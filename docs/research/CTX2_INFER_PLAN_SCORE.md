# CTX2-INFER step 2: model-free plan / audit / score substrate

Status: **NOT_MEASURED**. Anchors **NOT_QUALIFIED** (mock only). RemoteOps loop
**NOT_CONNECTED**. This step loads no model weights, calls no runtime, embeds no
tokenizer and records no metric. It makes no model-quality claim.

Prereg: `docs/research/CTX2_INFER_PREREG.md` (PR #909, r1). This document does not
amend the prereg. Where the prereg leaves a detail open, the choice made here is listed
under "Plan interpretations" below. ChatGPT protocol review can reject any of them
before the prereg is merged and any seed is derived.

## Code

- `crates/sml-store/src/ctx2_infer.rs`: the root module. It holds the freeze, the seeds,
  the status constants and the SplitMix64 PRNG. It is `#![forbid(unsafe_code)]` and has no
  new dependency.
  - `generator.rs`: CTX2-FACTS-v1.
  - `plan.rs`: the arms, CONTEXT-2 prompt assembly through the CONTEXT-1a raw store,
    prompt and plan hashes, and `CallLedger`.
  - `restart.rs`: the A3r seal / drop / anchored-restore path and its negative controls.
  - `audit.rs`: the exact token-ID budget audit.
  - `compute.rs`: the compute envelope.
  - `score.rs`: EM, Wilson 95 %, the one-sided exact McNemar test, Holm, the
    session-clustered bootstrap, and strict-H2 classification.
- `crates/sml-store/tests/ctx2_infer.rs`: 29 tests, all model-free.
- No CONTEXT-2 module, wire, envelope, anchor or raw-store contract was changed. Nothing
  in BPB3, LONGCTX-B1, M2 or `crypto_universe` was touched.

## Freeze (prereg section 9)

| Item | Value |
| --- | --- |
| Path | `docs/research/CTX2_INFER_PREREG.md` |
| SHA-256 of exact bytes | `e26dfe16f7222b297d47cd02d74442868de8e7ea58388e4f45e87d8a7d51bea1` |
| Content commit (r1) | `ecec94b4a7bc12ca1b83c454475d157b025b1f37` |
| PR #909 head at freeze | `eff41fab52196809139bd8b7646f0712a1de6450` |
| Merge state | `Unmerged`. `test_seeds()` and `build_frozen_plan()` return `PreregNotMerged` |

The test `prereg_freeze_matches_repository_bytes` hashes the checked-in prereg with
`include_bytes!`. Any in-place edit of the prereg therefore fails CI until the pin is
updated deliberately in a reviewed commit. Test seeds can only be derived after
`attest_merge(<merge sha>)`. The merge SHA is caller-attested (format-checked, not
verified on-chain here). The observation document must cite it.

## Plan interpretations (open details of the prereg, fixed here)

1. **Test seed encoding.** `merge_sha_of_this_prereg` enters the SHA-256 input as its 40
   lowercase ASCII hex characters: `SHA-256("CTX2-INFER-test-v1" || hex40 || u8(i))`, with
   the first 8 bytes read as a little-endian u64.
2. **Dev seeds.** The prereg fixes their count (2) but not their derivation. Here they are
   `SHA-256("CTX2-INFER-dev-v1" || u8(i))`, first 8 bytes LE, `i ∈ {0, 1}`. That gives
   `0x3fc3d3ad6bd3e87a` and `0x13f3d0a2cfece573`. They do not depend on the merge SHA and
   are used only for the precheck, the smoke plan and debugging.
3. **Per-session generator seed.** `SHA-256("CTX2-FACTS-v1/session/" || split || seed_le ||
   index_le)`, first 8 bytes LE, fed to SplitMix64. Sampling uses rejection (no modulo bias).
4. **Turn bytes.** `Note <n>: The <attribute> of <entity> is <value>.\n`. The trailing
   newline is part of the segment, so concatenating segments in order is unambiguous.
   Query text is exactly as in the prereg.
5. **Session construction.** The five buckets plus one extra bucket drawn by the seed are
   shuffled. Each query gets a distinct answer turn `j` with `d = T − j + 1` uniform in its
   bucket, and a distinct (entity, attribute) key. With probability 1/2 a prior `Set` of
   that key is placed at an earlier free turn, and turn `j` becomes the `Update`. All
   other turns are distractors over keys that are never queried in the session. Values are
   4-digit strings unique within the session.
6. **Vocabularies.** Dev uses 12 entities × 8 attributes and test uses 12 × 8, with the
   two lists disjoint (`generator.rs`).
7. **Stream and segment IDs.** The stream ID is session index + 1, the segment ID is the
   turn number, and the fingerprint is the first 8 bytes LE of the content hash, forced
   to be non-zero.
8. **Fixed tamper position per seed.** `SHA-256("CTX2-INFER-tamper-v1" || seed_le)`, first
   8 bytes LE, modulo the envelope length. The flipped byte is XORed with `0x01`.

## What the tests establish (model-free, structural only)

- **Freeze:** the pinned prereg hash matches the checked-in bytes. A one-byte flip,
  truncation or extension is refused. Test seeds fail closed while unmerged, and an
  invalid merge SHA is refused. Seed derivation is recomputed independently.
- **Generator:** it is deterministic. The dev seed 0 session file is pinned at SHA-256
  `f16eb9847eddf8ebf5cc936c54bb9e3b02a1a311bd0cd13ea84739b29eef5ba4`; this is a
  regression pin of this code, not a measurement. Values are unique, every bucket is
  covered with exactly one drawn twice, and gold is the latest assignment and occurs in
  exactly one turn. Distractors never touch queried keys, and the dev and test lists are
  disjoint.
- **Distance convention:** `d ≤ k` holds if and only if the answer turn is inside the last
  `k` turns, including the boundary cases `d = k` and `d = k + 1`. The prereg example
  holds (`k = 4` keeps turns 45–48).
- **H1 (prompt level):** A3 history bytes equal A2 for every dev session and every `k`.
  On the 27 648-call grid, built with a clearly synthetic merge SHA, the A3/A2 prompt
  identity is 5 760 / 5 760.
- **H2p (prompt level):** the clean A3r path reproduces A3 with three anchored checkpoints
  (generations 12, 24, 36). On the full grid it is 5 760 / 5 760.
- **H3a:** the gold value is absent from the A2, A3 and A3r history bytes whenever
  `d > k`, and present whenever `d ≤ k` (positive control). There are 0 leaks on the
  full grid.
- **Restart negative controls (fail closed, no prompt produced):**
  - the per-seed byte flip;
  - truncation to 0, 1, len/2 and len−1 bytes;
  - an absent anchor and an unverifiable anchor;
  - an envelope from a foreign stream;
  - rollback (an older envelope presented after the anchor advanced);
  - rollback requested at the first checkpoint.

  All of these are refused at the expected turn. Every single-byte flip of a sealed
  envelope is refused, and the live context's snapshot is left byte-identical. The
  unmodified envelope is admitted.
- **Raw-store tamper:** a corrupted retained artifact (header or raw bytes), a missing
  segment, two retained segments swapped, or a retained segment swapped with an evicted
  one all make prompt assembly fail closed.
- **Token audit (prereg section 4.1)** runs on supplied token arrays. Test fixtures use a
  byte-level tokenizer and a pair-merging one; neither is the model tokenizer.
  - It enforces `|H| = B(s, k)` for A1, A2 and A3r, where `B(s, k) = |H_A3|` (consistent
    within a session), and requires an empty `H` for A0.
  - It requires `T_pre` and `T_post` to be identical across the arms of each query, and
    checks `|P| + n_predict ≤ n_ctx`. It also checks the server-reported prompt count.
  - Every planned call must appear exactly once, with no unplanned call, and the tokens
    must be bound to the planned history bytes by SHA-256.
  - It compares `Σ|P|` per `(k, seed)` across A1, A2, A3 and A3r, and reports the A1 = A2
    rate without a prediction.
  - Each violation has its own negative test.
- **Compute envelope:** `ComputeEnvelope::derive()` equals the prereg numbers: 34 560
  naive calls, 26 880 test calls, 768 precheck calls, 27 648 on the admitted host and 84
  smoke calls. The ceiling is 72 cell-equivalents × 3 600 s = 72 h, with 14 h per seed and
  2 h for the precheck. The built full grid has exactly 27 648 unique call keys.
  `CallLedger` refuses unplanned and duplicate calls, allows one retry after a transport
  failure, treats a second failure as unrecoverable, and refuses an incomplete finish.
  The wall-clock check fails closed.
- **Plan digest:** the smoke plan (84 calls) is pinned at SHA-256
  `864054b5f9de9295eb774b2c6e62a11a88f14feeef84a3dc3d3814898066a8a7`. A changed gold, a
  reordered record, a dropped record or a wrong pin is refused.
- **Scorer:** checked against hand-computed values:
  - EM edge cases;
  - Wilson intervals for 0/10, 5/10 and 10/10;
  - McNemar for `(5,0) = 1/32`, `(0,5) = 1`, `(3,1) = 5/16`, `(1,3) = 15/16`,
    `(10,10) = 616 666 / 2^20`, and `b + c = 0 → 1`;
  - Holm on a 4-test family;
  - a deterministic bootstrap with seed `0xC72_1AFE`;
  - the strict-H2 truth table (never `PASS` without the precheck).

## Compute and hardware

The step-4 test runs need RemoteOps admission on Dell or Thor. They should use the GPU
through a non-CUDA ggml backend (Vulkan) where the host exposes one; otherwise the run is
labelled CPU. No mandatory NVIDIA stack is involved. This step ran only `cargo` on the
Grok box and on GitHub-hosted CI, and that is not a measurement host.

## Not in this PR (next steps, unchanged from the prereg)

- `crates/sml-cli` `ctx2-infer plan` / `ctx2-infer score` subcommands.
- `research/ctx2-infer/driver.py`, written with the Python standard library only.
- `.github/workflows/ctx2-infer-smoke.yml`. This is step 3: pinned unpatched llama.cpp,
  SHA-checked Q8_0 GGUF, and the 84-call smoke, which carries no metric.
- The determinism precheck and the test runs. These are step 4, blocked on an admitted
  host. Thor `thor-ccos-arm64-01` is offline and the loop is `NOT_CONNECTED`.
- Merging #909 and attesting its merge SHA. This must happen before any test seed is
  derived.

LONGCTX-B1 remains **BLOCKED**. No G24 and no new mutation-matrix series. M2 (RemoteOps
#1430) is ChatGPT's and is not touched.
