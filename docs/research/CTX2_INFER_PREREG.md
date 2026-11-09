# CTX2-INFER: CONTEXT-2 in a real inference loop (preregistration and integration design)

Status: **PREREGISTERED, NOT EXECUTED, NOT_MEASURED.** This document is frozen before any
code or run for this track exists. It contains **no measured numbers** and makes
**no capability claim**. Every result cell is `TBD`. Results may only appear in a later
observation document that cites this file's merge SHA. Any change to a frozen item below
needs a dated amendment document, not an in-place edit.

Owner: Grok (SML-GENIUS). Physical runner admission: RemoteOps. Status flags carried by
every artifact of this track: `NOT_MEASURED`, anchors `NOT_QUALIFIED` (mock anchors only),
RemoteOps loop `NOT_CONNECTED`.

Decision origin: on 2026-11-09 the owner stopped the CONTEXT-2 meta-test series at G23
(no G24) and asked to wire CONTEXT-2 into a real inference loop with a preregistered bench.

## 0. Survey of `main` (state when this was written)

| Area | What exists | What it is not |
| --- | --- | --- |
| CONTEXT-2 `crates/sml-store/src/working_context.rs` (G2 #845, hardening #848/#849) | `WorkingContext::new/admit/snapshot/restore`, policies `Fixed{capacity}`, `Expanding{hard_max}`, `CapacityBounded{capacity}`, per-segment `SegmentAccounting` with fate `Retained/Evicted/Lost`, `retained_identities()` | It stores **identities, content hashes and fingerprints only**, never segment bytes. Retention is FIFO / ring. No content-aware selection. |
| SWC2 wire `working_context_wire.rs` (G4) | `encode_context`, `decode_and_restore`, `restore_into` (reject-without-mutation) | No checksum, no signature. |
| SWE1 envelope `working_context_envelope.rs` (G7 #863) | `seal_context`, `open`; unkeyed SHA-256 | Detects corruption only. Not authentication. |
| Anchor admission `working_context_anchor.rs` (G10 #871), provenance `working_context_anchor_provenance.rs` (G12 #875) | `restore_into_anchored`, `restore_into_anchored_with_versioned_provenance` | Exercised with mock anchors only: `NOT_QUALIFIED`. |
| CONTEXT-1a raw store `raw_context.rs` | `publish_raw_segment` / `load_raw_segment`, SHA-256 over exact raw bytes (`ContextContentHash`) | The natural byte store behind CONTEXT-2 identities. |
| Real-model execution | `research/boolean-posit/bpb3k_full_runtime.rs` + `.github/workflows/bpb3k-full-smollm2-inference.yml` (#887/#894): SHA-pinned SmolLM2-135M-Instruct GGUF executed end to end by llama.cpp pinned at `f7591a6a01f9273de03331732bdc0b39721168be` (CPU, CUDA/Metal/Vulkan OFF in CI), patched for BPB3-K | Research workflow, not a workspace crate. No SML-owned forward pass, tokenizer or generation loop exists in `crates/`. |
| G1 / LONGCTX-B1 | Real-text corpus protocol and B1 prereg | Blocked on owner corpus/SHA and B-lex/B-attn choices. Unrelated to this track. |

Consequence that shapes the whole design: **the smallest honest integration is CONTEXT-2 as
the working-context manager of a stateless prompt-assembly loop around a real model**, with
segment bytes held in the CONTEXT-1a raw store and resolved by content hash. Because every
CONTEXT-2 policy is FIFO/ring, CONTEXT-2 `CapacityBounded{k}` selects **exactly the same
segments** as a naive segment-level sliding window of `k`. This prereg therefore predicts
**no quality gain** from CONTEXT-2 (see H0). What the bench can establish is that CONTEXT-2 is
a correct, auditable, restart-safe context manager in a real loop, at a measured overhead.
A quality hypothesis needs a non-FIFO retention policy, which is out of scope (section 11).

## 1. Claim boundary and non-goals

- Not an effective context window, not CCOS recall, not a 1M-token attention claim.
- No claim that CONTEXT-2 improves answer quality over a sliding window.
- The model is an **external test subject**. SML does not train, fine-tune or modify it.
- BPB3-K is **not** used: the stock (unpatched) llama.cpp at the same pinned commit runs a
  stock GGUF, so this track is independent of the BPB3 research track.
- Grok architecture only; no Claude ternary/hybrid (HYB-*) or other agents' designs as the
  system under test.
- No mandatory NVIDIA stack: CUDA, NVRTC, cuDNN, TensorRT, NVML and CUTLASS are not used.

## 2. Model (frozen)

- `HuggingFaceTB/SmolLM2-135M-Instruct` (Apache-2.0, Llama architecture,
  `max_position_embeddings: 8192` per its `config.json`).
- Weights: `bartowski/SmolLM2-135M-Instruct-GGUF`, file `SmolLM2-135M-Instruct-Q8_0.gguf`,
  SHA-256 `5a1395716f7913741cc51d98581b9b1228d80987a9f7d3664106742eb06bba83` (pin already
  recorded in `research/boolean-posit/SMOLLM2_REAL_MODEL_GATE.md`). Q8_0 is chosen over
  Q3_K_S to keep quantization noise out of the comparison. A SHA mismatch fails closed.
- Runtime: upstream `ggml-org/llama.cpp` at `f7591a6a01f9273de03331732bdc0b39721168be`,
  unpatched, `llama-server` with `/tokenize`, `/detokenize`, `/completion` (prompt passed as a
  **token-id array**, so the token budget is controlled exactly). The context length the
  runtime reports must be at least the largest planned prompt; otherwise the cell fails closed.
- Decoding: greedy (`temperature 0`), `n_predict 8`, fixed `seed 42`, fixed thread count,
  fixed `n_ctx 4096`, no prompt cache reuse between queries (`cache_prompt false`), chat
  template of the GGUF applied identically in every arm.

## 3. Task (frozen): CTX2-FACTS-v1, synthetic multi-turn state tracking

No external dataset is needed for the primary bench, so nothing has to be licensed or
pinned: the generator is SML code and its output is pinned by SHA-256 at generation time.

- A **session** is `T = 48` turns. Each turn is one CONTEXT-2 segment of the form
  `Note <n>: The <attribute> of <entity> is <value>.` Turns either set a key for the first
  time, update an existing key (overwrite), or state an unrelated distractor fact.
- Values are 4-digit strings drawn uniformly without replacement **within a session**, so a
  value string appears in exactly one turn of its session (used by the leakage check).
- After the history, `Q = 6` queries per session: `What is the <attribute> of <entity>?
  Answer with the number only.` Gold = the **latest** value assigned to that key.
- Query distance `d` = number of turns between the latest assignment of the queried key and
  the end of the history. Queries are stratified so each session covers buckets
  `d ∈ {1–4, 5–8, 9–16, 17–32, 33–47}` (one bucket drawn twice, chosen by the seed).
- Each query is evaluated independently as a fresh stateless prefill: history (as selected by
  the arm) + query. No KV-cache is carried across queries in any arm.
- Vocabulary: entity names and attribute names come from fixed lists committed with the
  generator, split into **disjoint dev and test lists**.

Secondary real-text track CTX2-DOC (long-document next-token loss): **operator must choose**
the corpus, its revision and SHA-256. It is `NOT_PINNED` and blocked; it is not needed for the
primary bench and will get its own amendment once chosen.

## 4. Arms (frozen)

Let `k ∈ {4, 8, 16}` be the segment budget. For each query, `B(q, k)` is the model-token
count of the history selected by A3 for that query (measured with the runtime's own
`/tokenize`).

| Arm | History given to the model | Budget |
| --- | --- | --- |
| A0 `no-history` | none (query only) | 0 |
| A1 `token-truncation` | full history tokenized, keep the **last `B(q, k)` tokens** (may cut mid-turn); no CONTEXT-2 | exactly `B(q, k)` |
| A2 `sliding-window` | last `k` turns, concatenated in order; no CONTEXT-2 | `B(q, k)` by construction |
| A3 `context2` | `WorkingContext` with `CapacityBounded{capacity: k}`; every turn admitted; prompt = bytes of `retained_identities()` resolved through the raw store and hash-verified | `B(q, k)` |
| A3r `context2-restart` | as A3, but at turns `{12, 24, 36}` the state is sealed (`seal_context`, SWE1), the process exits, a new process opens it and restores with `restore_into_anchored` against a **mock** monotonic anchor, then continues | `B(q, k)` |
| R `full-reference` | all 48 turns; not budget-matched; capability reference only | full |

A1, A2 and A3 use the same model, same runtime, same decoding and the same per-query token
budget. A0 and R bound the task from below and above. `Fixed{k}` is not a separate arm: it
retains the same set as `CapacityBounded{k}` and differs only in fate labels, which the unit
tests cover.

## 5. Hypotheses (falsifiable)

- **H0 (null, predicted true).** A3 and A2 have identical exact-match outcomes on every
  query. CONTEXT-2 brings **no** quality benefit over a sliding window. Confirming H0 is the
  expected result and is reported as such, not as a failure.
- **H1 (prompt identity).** For every query and every `k`, the prompt bytes of A3 equal the
  prompt bytes of A2 (SHA-256 equality, 100 %). Any mismatch falsifies H1 and is an
  integration defect.
- **H2 (restart fidelity).** For every query, A3r produces byte-identical prompts and
  byte-identical generated text to A3. A corrupted SWE1 envelope (single-byte flip, fixed
  position per seed) must be refused and the run must stop for that session with a recorded
  refusal, never continue with an empty or partial context.
- **H3 (no leakage).** For `d > k`, the gold value string is absent from the A2/A3/A3r
  prompt (checked on bytes, 100 %), and A2/A3 exact match on `d > k` queries is no higher
  than A0 on the same queries (one-sided paired test, section 7).
- **H4 (overhead).** CONTEXT-2 work per query (admit, raw-store load and hash verification,
  prompt assembly; plus seal/open/restore for A3r) has a median of at most 1 % of the
  median model wall time per query (prefill + decode) on the same host. Falsified otherwise;
  a falsification is kept as a negative result.
- **E1 (exploratory, no prediction).** For `d ≤ k`, paired difference in exact match between
  A3 and R (does a short window help or hurt a 135M model?). Reported with its interval only;
  never promoted to a claim.

## 6. Metrics (exact definitions)

- **EM (exact match):** decode the generated text, take the first maximal run of ASCII
  digits; EM = 1 if it equals the gold 4-digit string exactly, else 0 (no digits → 0).
- **EM(arm, k, bucket):** mean EM over all test queries of that cell, with a Wilson 95 %
  interval.
- **Prompt identity rate:** fraction of queries where `sha256(prompt_A3) == sha256(prompt_A2)`.
- **Restart identity rate:** fraction of queries where both prompt SHA-256 and generated text
  of A3r equal those of A3.
- **Leakage rate:** fraction of `d > k` queries whose A2/A3/A3r prompt contains the gold value
  string.
- **Overhead ratio:** `median(t_ctx2) / median(t_model)`, monotonic clock, per query, same
  process, A3 and A3r reported separately.
- **Budget audit:** for every A1 query, its history token count must equal `B(q, k)`.

## 7. Seeds, sizes, statistics

- Dev seeds: 2, used only for harness debugging and CI smoke.
- Test seeds: 5 (≥ 3 required). Seed `i ∈ {0..4}` is the first 8 bytes (little-endian u64)
  of `SHA-256("CTX2-INFER-test-v1" || merge_sha_of_this_prereg || u8(i))`. They cannot be
  known before this document is merged.
- Per test seed: 64 sessions × 6 queries = 384 queries; 1 920 test queries per (arm, k).
- Paired comparisons use a session-clustered bootstrap, 10 000 resamples, fixed resampling
  seed `0xC72_1AFE`, 95 % percentile interval. H3's second clause uses an exact one-sided
  McNemar test at α = 0.05 per `k`, Holm-corrected over the three `k` values.
- H0/H1/H2 are exact-identity hypotheses: no statistical tolerance applies.

## 8. Compute and budget comparability

- Same host, backend, binary, thread count, `n_ctx`, decoding and `n_predict` for every arm
  of a cell. Arms of a cell are interleaved per session (order rotated by session index) to
  spread drift.
- Every query is one prefill + up to 8 decoded tokens in every arm; A3r adds no model forward
  pass (it restores working-context provenance only, not a KV-cache).
- Determinism precheck before test runs: R on dev seed 0 run twice; generated text must be
  byte-identical. If not, the backend is declared nondeterministic, H1/H2 text-identity is
  evaluated on prompts only, and that limitation is printed in every report.

## 9. Splits and leakage control

- Dev and test use disjoint entity and attribute lists and disjoint seeds.
- Generator, arm code, scorer and this prereg are frozen by commit before any test seed is
  derived. No prompt wording, decoding parameter or `k` may be tuned on test data.
- Values are random 4-digit strings unique within a session, so the model's pretraining cannot
  contain them; the gold value appears in exactly one history turn.
- The scorer reads only generated text and gold labels; arm identity is not used in scoring.
- Plan, prompts, raw outputs and scores are content-addressed (SHA-256) and listed in a
  manifest committed with the observation document.

## 10. Stopping rules and negative results

- All preregistered cells (6 arms × 3 `k` × 5 test seeds) run once. No early stop on results,
  no re-run to get a better number.
- Abort and mark the run `INVALID` (kept, with a defect report) on: model or binary SHA
  mismatch, runtime context length too small, budget audit failure, unrecoverable runner
  failure. Do not substitute another host or backend silently.
- An H1/H2/H3 failure is an integration defect: the run is kept, the defect is reported, the
  fix needs a new commit and a full re-run of every cell, both runs reported.
- An H4 failure, an E1 result in either direction, and an H0 confirmation are all published as
  they are. Negative results are never deleted.
- Wall-clock cap per cell: 60 minutes on the admitted host; hitting it is a recorded failure.

## 11. Integration design

### API seams (no change to the frozen CONTEXT-2 contracts)

- Byte store: `raw_context::publish_raw_segment` / `load_raw_segment`, keyed by
  `ContextSegmentId`, verified by `ContextContentHash` (SHA-256 of exact turn bytes).
- Working state: `WorkingContext::new(stream, WorkingPolicy::CapacityBounded{capacity: k})`,
  one `admit(WorkingSegment::new(id, order_index, content_hash, fingerprint))` per turn, with
  `fingerprint` = first 8 bytes of the content hash (forced non-zero per the existing contract).
- Prompt assembly: `retained_identities()` → load bytes → verify hash → concatenate in
  admission order. A missing or mismatching segment fails closed.
- Restart: `working_context_envelope::seal_context` / `open`, then
  `working_context_anchor::restore_into_anchored` with a mock anchor (status `NOT_QUALIFIED`).
- Tokens: CONTEXT-0 keeps token counts caller-supplied; counts and token-level truncation
  for A1 come from the runtime's `/tokenize` and are passed in as data. The Rust side never
  embeds a tokenizer.

### Files to add or touch (step 2, separate PRs)

- `crates/sml-store/src/ctx2_infer.rs` (new, model-free, `#![forbid(unsafe_code)]` crate):
  CTX2-FACTS-v1 generator, arm selection (A0–A3r, R), prompt records
  `{arm, k, session, query, segment_ids, prompt_sha256, token_count}`, scorer.
- `crates/sml-store/src/lib.rs`: module export.
- `crates/sml-store/tests/ctx2_infer.rs`: unit and property tests (below).
- `crates/sml-cli/src/main.rs`: `ctx2-infer plan` / `ctx2-infer score` subcommands
  (no new third-party dependency).
- `research/ctx2-infer/driver.py` (Python standard library only): sends plan prompts as
  token arrays to the pinned `llama-server`, records raw outputs and timings.
- `.github/workflows/ctx2-infer-smoke.yml`: CI-only smoke (below).
- Observation documents later under `docs/research/` and `docs/agent-reports/`.
- Not touched: CONTEXT-2 modules and their tests, BPB3 research files, and the
  `crypto_universe` files present in some local checkouts.

### Test plan (no model needed)

- Generator determinism: fixed dev seed → golden SHA-256 of the session file.
- Value uniqueness per session; bucket coverage per session.
- H1 as a property test: for many synthetic sessions and all `k`, A3 prompt bytes == A2.
- Leakage: for `d > k`, gold value absent from A2/A3 prompts.
- Restart: seal → open → anchored restore mid-session yields the same remaining prompts;
  every single-byte flip of the envelope is refused and leaves the live context unchanged.
- Raw-store tamper: a modified segment file fails prompt assembly closed.
- A1 budget: truncated token count equals `B(q, k)` given synthetic token arrays.

### CI-only smoke plan (mechanics, not measurement)

GitHub-hosted `ubuntu-24.04`, CPU backend: build the pinned unpatched llama.cpp
(`llama-server`), download the Q8_0 GGUF and verify its SHA-256, run dev seed 0 with
2 sessions for every arm and `k = 4`. Assert only: prompt identity A3 == A2, restart identity
A3r == A3, budget audit, determinism of R across two runs, corrupted-envelope refusal. The
job prints `NOT_MEASURED` and records no metric; its EM values are not results.

### Target hardware and what is blocked

- Test runs go through RemoteOps admission on Dell or Thor, GPU first through a non-CUDA ggml
  backend (Vulkan) where the admitted host exposes one; otherwise the run is CPU and labeled
  CPU, with no GPU wording. No interruption of active services.
- **Blocked now:** Thor `thor-ccos-arm64-01` is offline; SML-GENIUS has no registered
  self-hosted runner; the RemoteOps loop is `NOT_CONNECTED`. Steps 2–3 (code, CI smoke) can
  proceed on GitHub-hosted CI; step 4 (test runs) waits for an admitted host.
- **Owner decisions:** (a) confirm the synthetic CTX2-FACTS-v1 task as the primary bench, or
  name a real corpus + revision + SHA-256 for CTX2-DOC; (b) acknowledge that with FIFO
  policies CONTEXT-2 is predicted to give no quality gain, and choose whether a content-aware
  retention policy (e.g. a CONTEXT-4/5 follow-up) should be preregistered next; (c) which host
  (Dell or Thor) to admit for test runs.

## 12. Result tables (TBD)

| Cell | EM | Prompt identity | Restart identity | Leakage | Overhead |
| --- | --- | --- | --- | --- | --- |
| all | TBD | TBD | TBD | TBD | TBD |
