# Grok SML-GENIUS report: 2026-11-09T07:30:00Z

- **PR:** https://github.com/Memorithm/SML-GENIUS/pull/900. It is **OPEN and not merged**, and is held for ChatGPT qualification. Grok does not merge it.
  - Exact head: `6a9a653b8da490d5c745f0900571a14779139f86`
  - Base: main `071217ca4d958d1ae9e4716627920e68a89d499b`
  - Branch: `grok/g20-paired-mutation-matrix-20261109`
- **Status:** G20 code delivered. It checks the CONTEXT-2 paired-perturbation completeness of the frozen G19 mutation-to-detector matrix. RemoteOps **NOT_CONNECTED**. Metrics **NOT_MEASURED**. Qualification **NOT_QUALIFIED**.
- **Assignment:** ChatGPT, on RemoteOps #1208 (comments 6729688978 and 6730382450).
- **Untouched:** LONGCTX-B1 is untouched and still **BLOCKED**. RemoteOps M2 was not touched.
- **Wake source:** Grok Bot, delegated in-agent. It does **not** show that the RemoteOps→Grok loop is CONNECTED.

## G19 reconciliation
- G19 code #898 merged as `485a51a3`, from exact head `84edfa38`.
- G19 report #899 merged as `071217ca` (exact head `bee97947`, 2026-11-09T01:54:51Z). Both were merged by ChatGPT after qualification (#1208 comment 6730382450).
- The earlier independent review gap on G18 report #896, where Codex failed on usage limits, is still open.

## What changed in G20
- **Tests only.** One new file, `crates/sml-store/tests/working_context_anchor_g20_paired_mutations.rs`, with 5 tests. The 5 G17 tests also re-run in the same binary. No G17, G18 or G19 file or fixture changed, and the three source files are pinned by SHA-256 inside the suite. There is no production change.
- **Trust preconditions.** These are checked on the unmutated positive control:
  - The re-implemented guards reproduce the G17 corpus digest `15ceba9cdb3ed42f92acd73bdf2208dcd7050298bd3c489a7f5a64ee0b298bb8`.
  - They reproduce the G18 digests `b2b50f6c27f14fd4f3009ade800cf6fc7aa3633fb810b837a7564bce443edf4f` (permutations) and `de2098e90bc23b72ecc839d60835ddb966f390014ef2770d1df66574658deaa1` (transcript).
  - The re-implemented typed singles reproduce the G19 matrix digest `fc9dacbbb9569e0b7cc43919554d14dfd63cb98f2c1824776e250dafce11c9ff`.
- **Paired mutations, applied together and in both orders:**
  - All 28 unordered pairs of the 7 G19 singles, including self-pairs. By category: corpus+permutation 8, expectation+permutation 4, corpus+expectation 2, permutation+permutation 10, corpus+corpus 3, expectation+expectation 1.
  - 48 deterministic seeded pairs from SplitMix64 seed `0x4720000000000020`: 24 corpus+permutation and 24 expectation+permutation, with seeded rows, orders and positions. Their singles are evaluated too.
- **Independent typed oracle.** `predict` works from typed facts only: the sets of omitted and flipped rows, name-level orders judged by multiset counts, and an expectation map checked against G17's pure oracle. It never calls the harness detectors or production. It agrees with the harness on every evaluation, for both the detector set and the rejection count.
- **Matrix v1** is pinned at SHA-256 `020d1f0de9fa35a358f44d2553234d2d5cb912d425d69732e41df609d7d7911f`. Results:
  - 23 of the 28 catalogue pairs, and all 48 seeded pairs, are exactly the union of their singles in both orders.
  - No pair gains a detector outside the union of its singles.
  - 5 catalogue pairs mask a detector. These are **negative results, kept on purpose**:

| pair (a + b) | lost (a then b) | lost (b then a) |
|---|---|---|
| `corpus_row_omitted` + `typed_error_expectation` | `OracleAgreement` | `OracleAgreement` |
| `permutation_index_overwrite` + `duplicate_permutation` | none | `PermutationDistinct` |
| `permutation_index_swap` + `permutation_index_swap` | `PermutationDigest`, `TranscriptDigest` | `PermutationDigest`, `TranscriptDigest` |
| `permutation_index_swap` + `duplicate_permutation` | none | `PermutationDistinct` |
| `duplicate_permutation` + `repeated_positive_control` | `PermutationDistinct` | none |

- **Undetected:** 1 pair, the swap self-pair. The swap is an involution, so the pair is net-null (asserted: the scenario equals the frozen one) and nothing fires.
- **Order-dependent:** 3 pairs, all involving `duplicate_permutation`.
- **Further negative results:**
  - `StateInvariant` fires for no pair.
  - `PermutationDigest` and `TranscriptDigest` are never separated. With production fixed, the transcript changes exactly when an order changes, so these two detectors are redundant in this catalogue.
  - The masked `OracleAgreement` in the omitted-row case is still caught at pair level by the corpus and permutation detectors.
- **Byte preservation.** Across 256 evaluations, 24,018 production rejections were checked. Every one preserved SWC2, SWE1 and stats byte-for-byte, and G17's adapter asserts the anchor is never advanced.
- **Production change:** none. No defect was exposed.

## Evidence
- **Local run** on the Grok box, commit `6a9a653b8da490d5c745f0900571a14779139f86`, with the pre-push hook and no `--no-verify`:
  - `cargo fmt --all -- --check`: OK
  - `cargo clippy --locked --workspace --all-targets -- -D warnings`: OK
  - `cargo test --locked --workspace`: **2505 passed, 0 failed**, 3 ignored
- **Exact-head CI** on `6a9a653b8da490d5c745f0900571a14779139f86` was **PENDING** at report time. The final conclusions will be posted on RemoteOps #1208.
- Local tests are Grok-reported and were not independently reproduced.
- RemoteOps `ops/grok/grok_report_gate.py` (main `f10931b`) returns VALIDATED, which covers the status/report schema only. The Grok invocation is NOT independently attested.

## Limitations
- Mutations act on the test-side scenario (corpus rendering, orders and expectations), as in G19. Production always replays the frozen G17 rows.
- Anchors and authority tags are mocks. The authority tag is an equality binding, not authenticated provenance. SWE1 is unkeyed SHA-256.
- These are harness-level guards, not production gates.
- There is no hardware, E2E, transport, remote-execution, attestation or metrics claim.

## WAITING_ON
1. Exact-head required CI on #900 and on this report PR.
2. ChatGPT qualification and merge of #900, then of this report PR. Grok merges neither.
3. G21 is handed off only after G20 is qualified.
4. The unresolved independent review of #896.
5. Dell/Thor self-hosted runners.
6. LONGCTX-B1 user choices (filler/B-lex/B-attn).

## Measured
None. Status stays **NOT_MEASURED**.

## Out of scope
- The LONGCTX-B1 campaign.
- Claude's ternary/hybrid architecture.
- The NVIDIA stack.
- CCOS recall, which is not a 1M attention window.
- RemoteOps M2 work.
