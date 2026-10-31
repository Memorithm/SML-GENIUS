# SML-LONGCTX-B1 — Native long-context evaluation (preregistration)

Status: **PREREGISTERED, NOT EXECUTED.** This document is frozen before any
long-context run. It contains **no measured numbers** and makes **no capability
claim**. All result tables are `TBD` and may only be filled by a separate,
later observation document that cites this file's merge SHA.

Track: **Voie B** (native long-context architecture inside SML-GENIUS).
Roadmap slot: `SML-CONTEXT` → CONTEXT-7 "evidence-gated span ladder"
([`ROADMAP.md`](ROADMAP.md)). Protocol parent:
[`SML-CONTEXT-BOOTSTRAP.md`](SML-CONTEXT-BOOTSTRAP.md).

## 1. Claim boundary and non-goals

- Passing a gate at N tokens is **not** an "N-token context window" product
  claim. It only states: *on the frozen task family below, under the frozen
  tokenizer and budget, arm X met gate G at span N.*
- A mapped archive is **not** a context window. Bytes ingested, written to
  disk or memory-mapped do not count as context (CONTEXT-BOOTSTRAP, ROADMAP
  CONTEXT-7).
- **Voie A (CCOS governed external memory, CCOS-Enterprise #278,
  `docs/MODEL_RUNTIME_INTEGRATION.md`) is out of scope.** No Voie A result may
  be cited as Voie B evidence and vice-versa. Product-level "useful unlimited
  context" remains a Voie A concern.
- Non-goals: training a new model in this slice; importing Claude ternary/hybrid
  (HYB-*) or GPT skyrmion (SKY-*) designs as the system under test; any
  mandatory NVIDIA software stack (CUDA/NVRTC/cuDNN/TensorRT/NVML/CUTLASS).

## 2. Tokenizer contract (frozen)

SML does not yet define a learned tokenizer (CONTEXT-0: token counts are
caller-supplied). For B1 the caller tokenizer is frozen as:

- **`sml-byte-v0`: one token = one UTF-8 byte** of the raw segment payload.
  Deterministic, dependency-free, identical across arms.
- All span lengths below are in `sml-byte-v0` tokens. Reports must write
  "N byte-tokens", never bare "N tokens" or "N-token context".
- Baselines that use their own tokenizer receive the **same raw bytes**; their
  native token count is recorded as a secondary column only.
- Changing the tokenizer requires a new preregistration.

## 3. Span ladder

`L ∈ {4096, 16384, 65536, 262144, 1048576}` byte-tokens. A rung may run only
after the previous rung's gate has been evaluated (pass **or** recorded fail).
A failed rung does not block running the next rung for diagnostics, but blocks
any wording of success at that or larger rungs.

## 4. Hypotheses (falsifiable)

For each rung L and each arm:

- **H1 (exact single needle).** Exact-match retrieval accuracy ≥ 0.95 averaged
  over depths, with no depth bucket < 0.85. Falsified if either fails.
- **H2 (multi-needle / set membership).** Macro-F1 ≥ 0.90 for k ∈ {2, 4, 8}
  needles, and membership negatives (absent keys) false-positive rate ≤ 0.02.
- **H3 (multi-hop comprehension).** 2-hop key→value→value chain accuracy ≥ 0.80
  where both hops are separated by ≥ L/2 byte-tokens.
- **H4 (sublinear work).** For the SML CONTEXT arm, segments decoded/read per
  query grow sublinearly in L (fit exponent < 0.75 over admitted rungs), while
  H1 holds. Falsified if exhaustive-scan-equivalent work is needed.
- **H0 (null).** The SML arm is not better than the matched lexical/exhaustive
  baseline on H1–H3 at equal budget. Reported regardless of outcome.

Thresholds are frozen here; they are not tuned after seeing DEV/TEST.

## 5. Tasks

All tasks are generated synthetically from a frozen seed and a frozen
open-license filler corpus (to be pinned by SHA-256 in the fixture-freeze note
before run; until pinned, execution is blocked).

1. **Exact needle-in-haystack.** One `KEY=<16 hex> VALUE=<32 hex>` line inserted
   at depth `d ∈ {0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0}` of L. Distractors: 16
   near-miss keys (Hamming-1/2 hex) with different values. Answer must be the
   exact value bytes (source-exact → raw reread required).
2. **Multi-needle / set membership.** k ∈ {2, 4, 8} needles at stratified
   depths; queries: "value of key K_i" and "is key K present" with 50% absent
   keys.
3. **Multi-hop.** `A→B` at depth d1, `B→C` at depth d2, |d1−d2|·L ≥ L/2; ask
   for C given A. Distractor chains share prefixes.
4. **Deferred (explicitly not in B1):** natural-language reading comprehension,
   summarisation, code-repository QA, long-generation tasks. These need a
   learned decoder not qualified today; a later prereg must add them.

Per rung and seed: 7 depths × 20 instances for task 1; 3 k-values × 20 for
task 2; 20 for task 3.

## 6. Architecture under test (Grok/SML independent design space)

What the tree supports **today** vs future arms:

| Arm | Status on `main` | Role |
|---|---|---|
| `sml-ctx-raw-scan` — CONTEXT-0 ledger + CONTEXT-1a/1b/1c raw segments, exhaustive exact scan | implemented (substrate only) | SML control / upper-work bound |
| `sml-ctx-sbg-mor-topk` — CONTEXT-3 SBG/MOR shortlist + bounded top-k raw reread | **not implemented** (CONTEXT-3 open) | **primary arm, blocked until CONTEXT-3 merges** |
| `sml-ctx-recurrent` — CONTEXT-2 bounded recurrent SBG working state (+ DKV-2b frozen-key derived memory) | **not implemented** (CONTEXT-2 / DKV-3a open) | future arm |
| `sml-ctx-compacted` — CONTEXT-4 tiers / lossy compaction with provenance | not implemented | future arm, needs no-compaction control |

Rules: Delta-KV reads are derived semantic memory only; source-exact answers
must come from a raw reread with verified SHA-256 provenance (DKV-3a). The
B1 campaign cannot start until at least `sml-ctx-sbg-mor-topk` exists; running
only `sml-ctx-raw-scan` is a harness smoke test, not a B1 result.

## 7. Baselines (same bytes, same budgets)

- **B-lex:** BM25 over the same raw segments + exact substring verification
  (CPU). Reference implementation to be fixed before run (candidate: the CCOS
  #217 harness BM25, used read-only as a baseline, not as Voie A evidence).
- **B-exhaustive:** `sml-ctx-raw-scan` (oracle-like work upper bound).
- **B-attn:** one open-weight dense/long-context transformer runnable on a
  portable GPU path (Vulkan/ROCm/Metal/CPU via llama.cpp-class runtime).
  **Model, weights SHA and context setting: TBD — must be fixed before run.**
  Rungs beyond its native window are reported as `not applicable`, not as fail.
- **B-hyb (competitor only):** Claude ternary/hybrid line (HYB-H3 native
  engine) if it can answer the task at all; reported as a competitor, never as
  the system under test, and no design is copied from it.

Budget matching: same wall-clock cap per query, same RAM cap, same hardware
class, same number of seeds. A baseline that cannot fit the budget is recorded
as `budget-fail` (a negative, kept).

## 8. Hardware and budget

- Hosts: Thor (14 cores) and Dell, dispatched via RemoteOps. GPU preferred via
  portable paths only; CPU path mandatory as fallback.
- Fail-closed: OOM, timeout, provenance mismatch or hash mismatch abort the
  cell and record it as failure; no silent retry with a larger budget.
- Caps (frozen): per-query wall clock 60 s; process RSS 16 GiB; per-rung total
  6 h. Exceeding a cap = `budget-fail`.
- Recorded per cell (CONTEXT-BOOTSTRAP list): bytes ingested, byte-tokens,
  resident C0 bytes, segments shortlisted/decoded, raw bytes reread, RSS peak,
  mapped bytes, disk bytes, VRAM peak (if GPU), latency p50/p95/p99, host,
  commit SHA.

Result table (to be filled only after run):

| Rung | Arm | H1 | H2 F1 | H2 FPR | H3 | segs read/query | RSS peak | VRAM peak | p95 latency | verdict |
|---|---|---|---|---|---|---|---|---|---|---|
| 4k…1M | each arm | TBD | TBD | TBD | TBD | TBD | TBD | TBD | TBD | TBD |

## 9. Seeds, splits, leakage control

- Seeds: **5** generation seeds per rung (`0xB1000001`..`0xB1000005`); all arms
  see identical instances. Report mean, min and per-seed values.
- Splits: DEV (seeds 1–2) for harness debugging and any baseline parameter
  choice; TEST (seeds 3–5) read **once** after arms are frozen by commit SHA.
- Leakage: filler corpus, keys and values are disjoint between DEV and TEST
  (separate key namespaces); no arm may be trained or tuned on TEST; needle
  strings never appear in filler (checked by exact scan, abort on hit).
- Any change to an arm after TEST is read invalidates the campaign.

## 10. Stop rules and promotion gates

- Stop a rung for an arm after a `budget-fail` on ≥ 2 of 5 seeds.
- Stop the ladder if H1 fails at a rung on TEST (later rungs may run as
  diagnostics only, labelled so).
- **Wording gate.** "Supports L byte-token dependency spans on B1 tasks" is
  allowed only if, at that rung and **all smaller rungs**, on TEST, all 5
  seeds: H1 and H2 pass, H3 passes, budgets held, provenance verified, and the
  result is reproduced on a second host. "1M" wording additionally requires
  H4 pass and an independent re-run from a clean checkout.
- Even when gated, the wording is scoped to B1 synthetic tasks; it is not a
  general "1M context window" claim.
- Negatives (failed hypotheses, budget-fails, H0 holding) are published in the
  observation document and never deleted.

## 11. Relationship to CONTEXT-0/1 and DKV

- CONTEXT-0 ledger and CONTEXT-1 raw/SBG/provenance artifacts are the
  **storage substrate**; they are not attention and not evidence of usable
  context.
- Mapped store ≠ window: capacity of the raw ledger is irrelevant to B1 gates.
- DKV-2b may be used inside `sml-ctx-recurrent` only as derived memory per
  DKV-3a; a DKV hit without raw verification counts as a wrong answer on
  source-exact tasks.

## 12. Gaps before execution (blocking)

1. CONTEXT-3 shortlist arm not implemented.
2. Filler corpus not pinned (SHA-256 fixture-freeze note required).
3. B-attn model/weights/runtime not selected.
4. B-lex reference implementation not pinned.
5. B1 harness (generator + scorer + accounting) not written; it must be merged
   and unit-tested on hand-written fixtures before any campaign run.
