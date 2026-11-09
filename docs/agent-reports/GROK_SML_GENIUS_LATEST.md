# Grok SML-GENIUS report: 2026-11-09T09:50:00Z

- **PR:** https://github.com/Memorithm/SML-GENIUS/pull/902. It is **OPEN and not merged**, and is held for ChatGPT qualification. Grok does not merge it.
  - Exact head: `9508f6ad416a68345415c1c7f923e07c571f5553`
  - Base: main `9eedaa6d454ae8bbbb1522f11375b98be33f0910`
  - Branch: `grok/g21-triple-perturbation-20261109`
- **Status:** G21 code delivered. It checks CONTEXT-2 triple-perturbation completeness over the frozen G17–G20 detector matrices. RemoteOps **NOT_CONNECTED**. Metrics **NOT_MEASURED**. Qualification **NOT_QUALIFIED**.
- **Assignment:** ChatGPT, on RemoteOps #1208 (comment 6736254436); accepted by Grok in 6736314644.
- **Untouched:** LONGCTX-B1 is untouched and still **BLOCKED**. RemoteOps M2 was not touched.
- **Wake source:** Grok Bot, delegated in-agent. It does **not** show that the RemoteOps→Grok loop is CONNECTED.

## G20 reconciliation
- G20 code #900 merged as `0257f530`, from exact head `6a9a653b`.
- G20 report #901 merged as `918eb233`, from exact head `853f5cbf`. ChatGPT ACKed G20 after checking exact-head CI (#1208 comment 6736254436).
- The earlier independent review gap on G18 report #896, where Codex failed on usage limits, is still open.

## What changed in G21
- **Tests only.** One new file, `crates/sml-store/tests/working_context_anchor_g21_triple_mutations.rs`, with 6 tests. The 5 G17 tests also re-run in the same binary. No G17, G18, G19 or G20 file or fixture changed, and the four source files are pinned by SHA-256 inside the suite. There is no production change.
- **Trust preconditions.** The harness is copied from G20 and re-proven on the unmutated positive control:
  - G17 corpus digest `15ceba9cdb3ed42f92acd73bdf2208dcd7050298bd3c489a7f5a64ee0b298bb8`.
  - G18 digests `b2b50f6c27f14fd4f3009ade800cf6fc7aa3633fb810b837a7564bce443edf4f` (permutations) and `de2098e90bc23b72ecc839d60835ddb966f390014ef2770d1df66574658deaa1` (transcript).
  - G19 matrix digest `fc9dacbbb9569e0b7cc43919554d14dfd63cb98f2c1824776e250dafce11c9ff`.
  - G20 matrix digest `020d1f0de9fa35a358f44d2553234d2d5cb912d425d69732e41df609d7d7911f` and the G20 masking list.
- **Triples, each applied in all six orders:**
  - 30 catalogue triples: each of the 5 G20 masking pairs (they include the 3 order-dependent pairs and the net-null swap self-pair) plus each of 6 independent thirds (corpus flip, expectation retarget, index swap, index overwrite, duplicate, repeated control). Thirds act on orders 2..8 and on rows other than the G19 row under mutation, the retarget source and the positive control.
  - 24 seeded triples from SplitMix64 seed `0x4721000000000021`, which picks the masking pair, the kind of third and its parameters.
  - 1 detection positive control: three mutually independent thirds.
  - 2 net-null negatives: a swap involution plus a value-preserving expectation retarget.
- **Independent typed oracle.** `triple_oracle` is new for G21 and shares no code with G20's `predict`, the harness detectors or production. It tracks per-row corpus facts, owned name orders and an expectation map judged against G17's pure oracle. It agrees with the harness on all 342 triple evaluations, the control and the 7 singles, for the detector set, the rejection count and net-null status.
- **Matrix v1** is pinned at SHA-256 `e9e53022c4092e86a95399d06006d7eb4920af5e8a818b607e1fa675e9ac1835`. Results:
  - In every order of every non-net-null triple, the detector set equals the pair's set (in the same relative order) united with the third's set. There are no exceptions.
  - No triple gains a detector outside the union of its singles.
  - The detection positive control is the exact union {CorpusDigest, PermutationDigest, OracleAgreement, TranscriptDigest} in all six orders.
  - **Lost detectors (negative results, kept on purpose):** 22 of 30 catalogue triples and 17 of 24 seeded triples still lose a single detector, inherited from the pair's mask.

| lost detector | triples |
|---|---|
| `PermutationDistinct` | 24 |
| `OracleAgreement` | 10 |
| `PermutationDigest` + `TranscriptDigest` | 7 (incl. the 2 net-null negatives) |

  - The other 8 catalogue and 7 seeded triples are exact unions only because the third fires the masked detector itself.
- **Order-dependent:** 24 triples (15 catalogue, 9 seeded). In every case the set differs exactly in the orders where `b` precedes `a` (bitmask `0b101100` over abc, acb, bac, bca, cab, cba).
- **Undetected:** only the 2 net-null negatives, in all six orders. Each is asserted net-null (the scenario equals the frozen one). With an independent third, the G20 swap self-pair is never undetected, but it adds nothing: the triple fires exactly the third's detectors.
- **Further negative result:** `StateInvariant` fires for no triple.
- **Byte preservation.** Across 343 evaluations (including the control), 32,136 production rejections were checked. Every one preserved SWC2, SWE1 and stats byte-for-byte, and G17's adapter asserts the anchor is never advanced.
- **Production change:** none. No defect was exposed.

## Evidence
- **Local run** on the Grok box, commit `9508f6ad416a68345415c1c7f923e07c571f5553`, with the pre-push hook and no `--no-verify`:
  - `cargo fmt --all -- --check`: OK
  - `cargo clippy --locked --workspace --all-targets -- -D warnings`: OK
  - `cargo test --locked --workspace`: **2516 passed, 0 failed**, 3 ignored
- **Exact-head CI** on `9508f6ad416a68345415c1c7f923e07c571f5553` was **PENDING** at report time. The final conclusions will be posted on RemoteOps #1208.
- Local tests are Grok-reported and were not independently reproduced.
- RemoteOps `ops/grok/grok_report_gate.py` (main `e1f2d26`) returns VALIDATED, which covers the status/report schema only. The Grok invocation is NOT independently attested.

## Limitations
- Mutations act on the test-side scenario (corpus rendering, orders and expectations), as in G19 and G20. Production always replays the frozen G17 rows.
- Anchors and authority tags are mocks. The authority tag is an equality binding, not authenticated provenance. SWE1 is unkeyed SHA-256.
- These are harness-level guards, not production gates.
- There is no hardware, E2E, transport, remote-execution, attestation or metrics claim.

## WAITING_ON
1. Exact-head required CI on #902 and on this report PR.
2. ChatGPT qualification and merge of #902, then of this report PR. Grok merges neither.
3. G22 is handed off only after G21 is qualified.
4. The unresolved independent review of #896.
5. Dell/Thor self-hosted runners.
6. LONGCTX-B1 user choices (filler/B-lex/B-attn).

## Measured
None. Status stays **NOT_MEASURED**.

## Out of scope
- The LONGCTX-B1 campaign.
- Claude's ternary/hybrid architecture (G21 is Grok's own, independent test architecture).
- The NVIDIA stack.
- CCOS recall, which is not a 1M attention window.
- RemoteOps M2 work.
