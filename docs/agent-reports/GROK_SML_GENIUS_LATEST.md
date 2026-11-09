# Grok SML-GENIUS report: 2026-11-09T11:35:00Z

- **PR:** https://github.com/Memorithm/SML-GENIUS/pull/904. It is **OPEN and not merged**, and is held for ChatGPT qualification. Grok does not merge it.
  - Exact head: `e607f56498cd9b281e56fb2cc30dccd6c617e0f5`
  - Base: main `106898cbba58f8abfdc5f2dfce1dd9220571c355`
  - Branch: `grok/g22-fourth-mutation-20261109`
- **Status:** G22 code delivered. It runs CONTEXT-2 metamorphic fourth-mutation falsification over the frozen G17–G21 detector matrices. RemoteOps **NOT_CONNECTED**. Metrics **NOT_MEASURED**. Qualification **NOT_QUALIFIED**.
- **Assignment:** ChatGPT, on RemoteOps #1208 (comment 6738302613); accepted by Grok in 6738359190.
- **Untouched:** LONGCTX-B1 is untouched and still **BLOCKED**. RemoteOps M2 (#1415/#1418) was not touched or duplicated.
- **Wake source:** Grok Bot, delegated in-agent. It does **not** show that the RemoteOps→Grok loop is CONNECTED.

## G21 reconciliation
- G21 code #902 merged as `14b8ce6c`, from exact head `9508f6ad416a68345415c1c7f923e07c571f5553`.
- G21 report #903 merged as `106898cb`, from exact head `e4d0657a08b58699cd18e6aef77b1e446208d217`. ChatGPT ACKed G21 after checking exact-head CI (#1208 comment 6738302613).
- The earlier independent review gap on G18 report #896, where Codex failed on usage limits, is still open.

## What changed in G22
- **Tests only.** One new file, `crates/sml-store/tests/working_context_anchor_g22_fourth_mutations.rs`, with 7 tests. The 5 G17 tests also re-run in the same binary. No G17–G21 file or fixture changed, and the five source files are pinned by SHA-256 inside the suite. There is no production change.
- **Trust preconditions.** The harness is copied verbatim from G21 and re-proven on the unmutated positive control:
  - G17 corpus digest `15ceba9cdb3ed42f92acd73bdf2208dcd7050298bd3c489a7f5a64ee0b298bb8`.
  - G18 digests `b2b50f6c27f14fd4f3009ade800cf6fc7aa3633fb810b837a7564bce443edf4f` (permutations) and `de2098e90bc23b72ecc839d60835ddb966f390014ef2770d1df66574658deaa1` (transcript).
  - G19 matrix digest `fc9dacbbb9569e0b7cc43919554d14dfd63cb98f2c1824776e250dafce11c9ff`.
  - G20 matrix digest `020d1f0de9fa35a358f44d2553234d2d5cb912d425d69732e41df609d7d7911f`, with its masking, order-dependence and undetected lists.
  - G21 matrix digest `e9e53022c4092e86a95399d06006d7eb4920af5e8a818b607e1fa675e9ac1835`, with all G21 frozen lists. G21's own `triple_oracle` is re-checked on all 342 triple orders.
- **Pre-registered plan.** The plan covers bases, fourths, seed and controls, and contains no results. It is pinned at SHA-256 `4cd65d4976988c0960ab9e65bb3686b6b64e00bdb56368504b733c3f14d710ca`:
  - **Bases:** all 41 surviving G21 masks, meaning the G21 triples that are not the union of their singles (22 catalogue, 17 seeded, 2 net-null). They include all 24 G21 order-dependent triples.
  - **120 catalogue quadruples:** the 24 catalogue/net-null bases × 5 fourths.
    - 2 independent fourths: an expectation retarget on a free row and a swap on the free order 4.
    - 3 interacting fourths: a swap on order 0, order 1 copied onto order 0, and a restore of the G19 row's expectation.
  - **16 seeded quadruples** from SplitMix64 seed `0x4722000000000022`, which picks a base among all 41 masks and a fourth of any kind on any order or row.
  - **1 detection positive control:** four mutually independent mutations.
  - **2 net-null negatives.**
- **Insertion orders.** Every quadruple is evaluated in all 24 orders: the fourth `d` is inserted at each of 4 positions in each of the 6 G21 relative orders of `a, b, c`.
- **Metamorphic relations:**
  - **MR-1:** every order fires exactly `triple(relative order) ∪ d`.
  - **MR-2:** the detector set does not depend on where `d` is inserted.
- **Independent typed oracle.** `quad_oracle` is new for G22. It shares no code with G21's `triple_oracle`, G20's `predict`, the harness detectors or production. It uses `u16` row ids, an inline Fisher–Yates, occurrence counts, sorted/dedup distinctness and expectation sources. It agrees with the harness on all 3,336 quadruple orders and every single, for the detector set, the rejection count and net-null status. It also reproduces every G21 triple order.
- **Matrix v1** is pinned at SHA-256 `24cb859a593111ddb70f6644370d2392b0c187d6d7a3968d8440338247869160`. Results:
  - 139 quadruples × 24 orders = 3,336 order evaluations, from 2,923 distinct production replays (including the control).
  - **MR-1 and MR-2 hold** for all 48 independent quadruples and for the detection control, with 0 falsifications. The control is the exact union {CorpusDigest, PermutationDigest, OracleAgreement, TranscriptDigest} in all 24 orders.
  - **MR-1 falsified (negative results, kept on purpose): 32 quadruples.** That is 28 of 72 interacting, 3 of 16 seeded, and 1 net-null negative whose fourth repeats its third.

| MR-1 falsification: predicted but absent / present but not predicted | quadruples |
|---|---|
| `PermutationDistinct` / — | 19 |
| `PermutationShape` + `PermutationDistinct` / — | 6 |
| `PermutationDistinct` / `PermutationDistinct` (in different orders) | 5 |
| — / `PermutationDistinct` | 1 |
| `PermutationDigest` + `TranscriptDigest` / — | 1 |

  - **MR-2 falsified: 31 quadruples** (28 interacting, 3 seeded). The insertion position never matters for an independent fourth.
  - **Gained:** no quadruple fires a detector outside the union of its four singles.
  - **Lost against that union (negative results): 120 quadruples.**

| lost detector | quadruples |
|---|---|
| `PermutationDistinct` | 82 |
| `OracleAgreement` | 21 |
| `PermutationDigest` + `TranscriptDigest` | 11 |
| `PermutationShape` + `PermutationDistinct` | 6 |

  - **G21 mask:** it survives the fourth mutation in 116 quadruples and is re-exposed in 42. 88 quadruples are non-commuting.

| group | quadruples | non-union | non-commuting | re-exposed | surviving | MR-1 falsified | MR-2 falsified | undetected | distinct sequences |
|---|---|---|---|---|---|---|---|---|---|
| catalogue, independent fourth | 48 | 39 | 30 | 9 | 39 | 0 | 0 | 0 | 1,056 |
| catalogue, interacting fourth | 72 | 67 | 48 | 28 | 64 | 28 | 28 | 2 | 1,500 |
| seeded | 16 | 12 | 10 | 5 | 12 | 3 | 3 | 0 | 324 |
| positive control | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 24 |
| net-null | 2 | 2 | 0 | 0 | 1 | 1 | 0 | 2 | 18 |

- **Undetected:** only 4 quadruples, in all 24 orders, and each is asserted net-null (the scenario equals the frozen one). They are the two G21 net-null bases with the expectation restore, plus the 2 net-null negatives.
- **Further negative result:** `StateInvariant` fires for no quadruple.
- **Byte preservation.** Across 2,923 distinct production replays, 273,700 production rejections were checked. Every one preserved SWC2, SWE1 and stats byte-for-byte, and G17's adapter asserts the anchor is never advanced.
- **Production change:** none. No defect was exposed.

## Evidence
- **Local run** on the Grok box, commit `e607f56498cd9b281e56fb2cc30dccd6c617e0f5`, with the pre-push hook and no `--no-verify`:
  - `cargo fmt --all -- --check`: OK
  - `cargo clippy --locked --workspace --all-targets -- -D warnings`: OK
  - `cargo test --locked --workspace`: **2528 passed, 0 failed**, 3 ignored
  - The G22 binary passes 12 tests in about 49 s (debug build).
- **Exact-head CI** on `e607f56498cd9b281e56fb2cc30dccd6c617e0f5` was **PENDING** at report time. The final conclusions will be posted on RemoteOps #1208.
- Local tests are Grok-reported and were not independently reproduced.
- RemoteOps `ops/grok/grok_report_gate.py` (main `0d144f3`) returns VALIDATED, which covers the status/report schema only. The Grok invocation is NOT independently attested.

## Limitations
- Mutations act on the test-side scenario (corpus rendering, orders and expectations), as in G19–G21. Production always replays the frozen G17 rows.
- Anchors and authority tags are mocks. The authority tag is an equality binding, not authenticated provenance. SWE1 is unkeyed SHA-256.
- These are harness-level guards, not production gates.
- There is no hardware, E2E, transport, remote-execution, attestation or metrics claim.

## WAITING_ON
1. Exact-head required CI on #904 and on this report PR.
2. ChatGPT qualification and merge of #904, then of this report PR. Grok merges neither.
3. G23 is handed off only after G22 is qualified.
4. The unresolved independent review of #896.
5. Dell/Thor self-hosted runners.
6. LONGCTX-B1 user choices (filler/B-lex/B-attn).

## Measured
None. Status stays **NOT_MEASURED**.

## Out of scope
- The LONGCTX-B1 campaign.
- Claude's ternary/hybrid architecture (G22 is Grok's own, independent test architecture).
- The NVIDIA stack.
- CCOS recall, which is not a 1M attention window.
- RemoteOps M2 work (#1415/#1418).
