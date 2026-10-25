# BAF-H4 — native Boolean ANF routing against a trained neural head (pre-registration)

**Status: protocol frozen before implementation and before any fresh text evaluation.**
This is a model-level **incremental routing** experiment, **not** evidence that a stand-alone Boolean LLM exceeds a fully trained neural LLM. Prior H-1d/H-2/H-3 results are historical context, never reused for a fresh holdout verdict.

## Existing result and question

The H-3 lossless compressor is P1 (Boolean count-bank model) + an optional frozen H-1d ternary neural head (1,536 weights) + an exact binary arithmetic coder. On EVAL5 (40 peS2o articles, 33,609,917 bytes) P1 alone achieved 2.5924 container BPC; the single H-1d head reached 2.4324/2.4403 BPC for seeds 11/23 with bounded runtime. This is a **positive neural contribution**, not a BAF victory. H-2's previous Hamming/MOR four-expert route did not meet its gain gate.

**H4 falsifiable question:** Can a natively Boolean, GF(2) ANF condition on the *current decoder-visible recurrent/context and P1 bank state* selectively enable the exact same frozen H-1d neural head, reducing actual code length versus **always enabling that neural head** on previously unread data, with acceptable runtime and genuine gate specificity?

The branch implementing the experiment must not read or tune on the future sealed panel prior to successful CAL-SEL qualification.

## Pre-fixed competing complete engines

All arms share the **identical P1 weights** SHA-256 `7f4c233d6c4d67e1d300068799f4653cfcaedda4158587478c99ec1d31c3c418`, integer coder, document order, and H-1d heads SHA-256 `b77e3f1301c414ff21246b3c5ad600b07e2adf44633a671ecd90fd1160e5e46e` (seed 11), `3c6192e8a248fb47680d941866e6cf22d6483636f539fa2aa4564430127c990f` (seed 23).

- **A0**: P1 only; no neural correction.
- **A1**: P1 + H-1d head at each byte, neural control from H-3, no routing.
- **A2**: P1 + the *same H-1d head*, applied only when a learned ANF Boolean predicate fires. Head values and total model P1 weights are frozen; only Boolean gate coefficients are trained. The gate is chosen once per byte before bit 0 and held for all eight decisions to preserve causal decoding.
- **A3**: P1 + the same head through a *non-learned* deterministic context-hash gate of predeclared mean activity matched to A2 on CAL-FIT only; its activity must not be calibrated on CAL-SEL or TEST. This controls the potential benefit of simply reducing neural-head invocations.

All arms must be implemented in the same real H-3 encoder/decoder and verified against independently calculated per-decision probabilities, not trace-only proxies.

## Native BAF features and search (no Q/K/V)

Feature extraction is exactly 8 Boolean inputs available **before** decision 0:

- `event`: low 4 bits of frozen P1 context word `record.context`;
- `state`: bits 0, 6, 12, 18 of the **depth-0** P1 bank thermometer `bank_word(record.banks.stretch[0])`, packed as four independent Boolean bits.

The activation is `bias XOR monomial_1 XOR ...` over event/state bit tests; degree ≤2, at most 8 present terms; no embeddings, similarity, softmax or scores between tokens. Initial activation is `bias=true` (A1, always on). Coordinate fitting flips a single coefficient of the constant or deterministic degree-1/2 bank at a time, up to 6 rounds. Every round picks the lowest loss *strict* improvement, in fixed basis order; no test selection or resampling. Worst case 6×37=222 candidate evaluations per seed. A maximally 8-term rule or a null improvement must stop deterministically.

TRAIN objective: minimize the actual integer-model trace-derived per-byte difference in arithmetic coding loss between A1 and A2 on CAL-FIT, computed from the exact P1 and H-1d predicted bit probabilities with the same H-3 coder table. Scores may be summed in fixed-point microbits to avoid host rounding-dependent search decisions. Neither gate labels nor the true upcoming byte are an input feature; the byte is consulted only when computing TRAIN outcome loss.

CAL-FIT and CAL-SEL are the **existing document-disjoint** H-1d splits `doc_index % 32 == 7` and `== 23`. The gate optimizer sees CAL-FIT loss/labels only. CAL-SEL is opened only once when the candidate is frozen. The optimizer cannot iteratively tune on CAL-SEL.

## Advancement gates and sealed evaluation

Before opening a *new* real-text evaluation sample:

1. Reproduce exact A0/A1 H-3 probabilities and frozen P1/heads on synthetic byte strings; 0 mismatches, 0 bad encode/decode round trips; no reading of future-byte bits as gate input.
2. On **both** H-1d seeds 11 and 23, candidate A2 must improve **CAL-SEL over A1 by at least 0.010 BPC** and be better than A3 by at least 0.005 BPC. The negative/null-label control must not be admitted. Otherwise `NO_GO_CAL_GATE`: do not open the fresh evaluation panel.
3. If admitted, prepare one **new, disjoint, rights-cleared** sample not present in S5h TRAIN/DEV, CAL, EVAL2, EVAL3 or EVAL5. Do not recycle EVAL5 or interpret EVAL4 (previously sealed for a different question) as an automatically available fresh sample. Prefer a new peS2o shard if verified and rights cleared; never assume one exists.
4. Full coder trial for A0/A1/A2/A3, both seeds, on that one new panel, without tuning. Measure container BPC, independent oracle agreement, 100% round-trip, time and RSS and exact asset/checkpoint hashes on x86-64 Dell and ARM64 Thor if accessible.

**GO routing contribution** iff, for both seeds, A2 − A1 container BPC ≤ −0.010, A2 − A3 ≤ −0.005, improvements >2× between-seed dispersion, 0 probability mismatches and 0 round-trip errors, and compression/decompression cost A2/A1 ≤1.50 on matched hardware. Report *every* negative regardless of verdict. An improved trace-only score without coder proof = `DIAGNOSTIC`, not GO.

**NOT a proof of Boolean supremacy:** A2 still uses a neural H-1d head. A pure Boolean model beating an independently trained complete neural network at matched training FLOPs, model bytes, latency and held-out language loss is a separate experiment, with its own proper preregistration. A2 GO only demonstrates that **native Boolean selection can improve this existing neural hybrid**, not that BAF dominates all neural models.

## Reproducibility and blocking conditions

- Code/plan/SHA pinned before CAL execution; no changing feature positions, search type, thresholds or seed after seeing CAL-SEL.
- H-4 code must expose separate functions for train-only fit, final validation veto, and causal coder routing. Report per-seed: train/val BPC, selected fraction, candidate count, ANF coefficients, baseline/head/gated scores, total resident bytes and exact checksum of gate.
- Fixed H1d head never retrained. Null control labels are independently permuted by a fixed seed and must fail to show a spurious gain.
- Any missing artifact / checksum mismatch / split overlap / overflow / incomplete report or unverified source yields `BLOCKED`. Never silently substitute synthetic for real evaluation or trust old DEV/TEST.
- No production default change, and no claim of neural-network-wide supremacy from toy Boolean transition benches. No NVIDIA software stack required.
