# CTX2-INFER: CONTEXT-2 in a real inference loop (preregistration and integration design)

Status: **PREREGISTERED, NOT EXECUTED, NOT_MEASURED.** This document is frozen before any
code or run for this track exists. It contains **no measured numbers** and makes
**no capability claim**. Every result cell is `TBD`. Results may only appear in a later
observation document that cites this file's merge SHA. Any change to a frozen item below
needs a dated amendment document, not an in-place edit. Before merge, the document is
revised in place and every revision is listed in section 13 (pre-freeze revision log).

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
  **token-id array**, so the token budget is controlled exactly; section 4.1). The context
  length the runtime reports must be at least `max |P| + n_predict` over the plan (section
  4.1); otherwise the run fails closed before the first test call.
- Decoding: greedy (`temperature 0`), `n_predict 8`, fixed `seed 42`, fixed thread count,
  fixed `n_ctx 4096`, one server slot (no batching across queries), no prompt cache reuse
  between queries (`cache_prompt false`), chat template of the GGUF applied identically in
  every arm as fixed token-ID pieces (section 4.1).

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
- Turns are numbered `1..T`. If the latest assignment of the queried key is at turn `j`, the
  query distance is **one-based**: `d = T − j + 1` (`d = 1` is the last turn, `d = T = 48` the
  first). A segment window of `k` turns keeps turns `T−k+1..T`, so the answer is
  **retained iff `d ≤ k`** and **evicted iff `d > k`**. Every rule below (H3, E1, leakage
  audit, buckets, tests) uses exactly this convention. Example: `T = 48`, `k = 4` keeps turns
  45–48 (`d ∈ {1,2,3,4}`); turn 44 has `d = 5 > 4` and is evicted.
- Queries are stratified so each session covers buckets
  `d ∈ {1–4, 5–8, 9–16, 17–32, 33–48}` (one bucket drawn twice, chosen by the seed). Bucket
  edges coincide with `k ∈ {4, 8, 16}`: a bucket is either wholly retained or wholly evicted
  for each `k`.
- Each query is evaluated independently as a fresh stateless prefill: history (as selected by
  the arm) + query. No KV-cache is carried across queries in any arm.
- Vocabulary: entity names and attribute names come from fixed lists committed with the
  generator, split into **disjoint dev and test lists**.

Secondary real-text track CTX2-DOC (long-document next-token loss): **operator must choose**
the corpus, its revision and SHA-256. It is `NOT_PINNED` and blocked; it is not needed for the
primary bench and will get its own amendment once chosen.

## 4. Arms (frozen)

Let `k ∈ {4, 8, 16}` be the segment budget. `B(s, k)` is the exact history token count of
A3 for session `s` (section 4.1). All queries of a session share the same history end, so the
budget depends on `(s, k)` only; earlier drafts wrote it `B(q, k)`.

| Arm | History given to the model | Budget |
| --- | --- | --- |
| A0 `no-history` | none (query only) | 0 |
| A1 `token-truncation` | full 48-turn history tokenized as one string, keep the **last `B(s, k)` token IDs** (may cut mid-turn or mid-word); no CONTEXT-2 | exactly `B(s, k)` |
| A2 `sliding-window` | last `k` turns, concatenated in order; no CONTEXT-2 | `B(s, k)` (audited) |
| A3 `context2` | `WorkingContext` with `CapacityBounded{capacity: k}`; every turn admitted; prompt = bytes of `retained_identities()` resolved through the raw store and hash-verified | `B(s, k)` by definition |
| A3r `context2-restart` | as A3, but at turns `{12, 24, 36}` the state is sealed (`seal_context`, SWE1), the process exits, a new process opens it and restores with `restore_into_anchored` against a **mock** monotonic anchor, then continues | `B(s, k)` (audited) |
| R `full-reference` | all 48 turns; not budget-matched; capability reference only; independent of `k` | full |

A1, A2 and A3 use the same model, same runtime, same decoding and the same per-query token
budget. A0 and R bound the task from below and above. `Fixed{k}` is not a separate arm: it
retains the same set as `CapacityBounded{k}` and differs only in fate labels, which the unit
tests cover.

A0 and R do not depend on `k` (their prompt token IDs are the same for every `k`). They are
therefore run **once per (test seed, query)** and the same outcome is paired with every `k`
in section 7 (deduplication; section 8.1).

### 4.1 Exact token-budget accounting (frozen)

Every model call receives one token-ID array `P`, and every budget statement refers to IDs in
`P`, never to strings or to a separate tokenization.

- **Pieces.** `P = T_pre ‖ H ‖ T_post`, concatenated as ID arrays (no re-tokenization of the
  joined string), where
  - `T_pre` = IDs of the chat-template text before the user content;
  - `H` = IDs of the history selected by the arm (empty for A0);
  - `T_post` = IDs of the query text followed by the chat-template text after the user
    content up to and including the assistant generation prefix.
- **Template rendering.** The GGUF's embedded chat template is rendered once with a single
  user message equal to a fixed sentinel string; the rendered text is split at the sentinel
  into a prefix and a suffix. Both strings and their SHA-256 are written into the plan and
  must be identical in every arm. No system prompt is added beyond what the template itself
  renders for a single user message.
- **Tokenization calls.** All IDs come from the pinned runtime's `/tokenize`: `T_pre` with
  the tokenizer's own special-token policy (BOS added or not exactly as the GGUF metadata
  says; the choice is recorded in the plan), `H` and `T_post` without adding special tokens.
  Template control tokens must each map to their single special-token ID; this is checked
  by `/detokenize` round-trip of `T_pre` and `T_post` before any test call. A failure stops
  the run (`INVALID`).
- **History IDs per arm.** A2, A3, A3r and R: `H = tokenize(concatenation of the selected turn
  bytes)`, one call per prompt. A1: `H = last B(s, k) IDs of tokenize(all 48 turns as one
  string)`. Boundary effects are fixed by construction: there are no merges across the
  `T_pre | H | T_post` boundaries because pieces are joined as ID arrays, and the remaining
  difference between A1 and A2 is only where the whole-string tokenization places the cut.
  Whether A1's `H` equals A2's `H` is recorded per prompt and reported as a rate, with no
  prediction.
- **Budget definition.** `B(s, k) = |H_A3(s, k)|`, the exact ID count of A3's history.
- **Per-prompt audit record.** `{arm, k, seed, session, query, |T_pre|, |H|, |T_post|, |P|,
  sha256(P as little-endian u32 IDs), sha256(history bytes)}`.
- **Audit invariants (all must hold, else the run is `INVALID`):**
  1. `|H| = B(s, k)` for every A1, A2, A3 and A3r prompt (A1 by construction; A2/A3r by check);
  2. `|T_pre|` and `|T_post|` are identical across all arms for the same query, so the total
     input tokens `|P|` of A1, A2, A3 and A3r are identical per query;
  3. the server-reported prompt token count of each `/completion` response equals `|P|`
     (this catches any BOS or special token the server would add on its own);
  4. `|P| + n_predict ≤ n_ctx` for every planned prompt; this is checked on the full plan
     before the first test call.
- **Per-arm totals.** For every `(arm, k, seed)` the report lists `Σ|P|` and `Σ|H|` over its
  queries. Invariant 2 implies equal `Σ|P|` for A1, A2, A3 and A3r within each `(k, seed)`.
  A0 and R totals are reported once per seed. No token count is estimated in advance; all are
  measured from the plan.

## 5. Hypotheses (falsifiable)

- **H0 (null, predicted true).** A3 and A2 have identical exact-match outcomes on every
  query. CONTEXT-2 brings **no** quality benefit over a sliding window. Confirming H0 is the
  expected result and is reported as such, not as a failure. H0 compares generated text, so
  it is `INDETERMINATE` if the determinism precheck (section 8) fails.
- **H1 (prompt identity).** For every query and every `k`, the prompt bytes of A3 equal the
  prompt bytes of A2 (SHA-256 equality, 100 %). Any mismatch falsifies H1 and is an
  integration defect.
- **H2 (strict restart fidelity).** For every query, A3r produces byte-identical prompts
  **and** byte-identical generated text to A3, and a corrupted SWE1 envelope (single-byte
  flip, fixed position per seed) is refused: the session stops with a recorded refusal and
  never continues with an empty or partial context. The definition of H2 does not change
  with the precheck outcome. Classification:
  - `PASS`: precheck passed, all prompts and all texts identical, every corrupted envelope
    refused;
  - `FAIL`: any prompt mismatch or any accepted corrupted envelope (these do not depend on
    model determinism), or precheck passed and any generated text differs;
  - `INDETERMINATE`: precheck failed and there is no prompt or refusal failure. H2 is then
    reported as not established; it is never reported as passed.
- **H2p (prompt-only restart fidelity, always reported separately).** For every query,
  `sha256(P)` of A3r equals that of A3, and every corrupted envelope is refused. H2p is
  evaluated and reported in every outcome of the precheck. It never replaces H2 and is never
  reported under the name H2.
- **H3a (no leakage in prompts; hard invariant).** For every query with `d > k`, the gold
  value string is absent from the A2, A3 and A3r history bytes (checked on bytes, 100 %).
  Any occurrence falsifies H3a and is an integration defect.
- **H3b (no detectable answer advantage on evicted queries; statistical).** For `d > k`,
  A2 and A3 exact match is not higher than A0 on the same queries. Tested with the
  one-sided exact McNemar test of section 7. Rejection is a statistical finding, not proof
  of a defect; non-rejection is **not** evidence of equivalence or of absence of leakage
  (no equivalence margin is preregistered, so no equivalence claim can be made). H3a is the
  only basis for a "no leakage in prompts" statement.
- **H4 (overhead).** CONTEXT-2 work per query (admit, raw-store load and hash verification,
  prompt assembly; plus seal/open/restore for A3r) has a median of at most 1 % of the
  median model wall time per query (prefill + decode) on the same host. Falsified otherwise;
  a falsification is kept as a negative result.
- **E1 (exploratory, no prediction).** For `d ≤ k`, paired difference in exact match between
  A3 and R (does a short window help or hurt a 135M model?). `d ≤ k` uses the one-based
  distance of section 3, so every E1 query has its answer inside A3's window. Reported with
  its interval only; never promoted to a claim.

## 6. Metrics (exact definitions)

- **EM (exact match):** decode the generated text, take the first maximal run of ASCII
  digits; EM = 1 if it equals the gold 4-digit string exactly, else 0 (no digits → 0).
- **EM(arm, k, bucket):** mean EM over all test queries of that cell, with a Wilson 95 %
  interval.
- **Prompt identity rate:** fraction of queries where `sha256(prompt_A3) == sha256(prompt_A2)`.
- **Restart identity rate (H2):** fraction of queries where both `sha256(P)` and generated
  text of A3r equal those of A3. Reported as `INDETERMINATE` when the precheck failed.
- **Restart prompt identity rate (H2p):** fraction of queries where `sha256(P)` of A3r equals
  that of A3. Always reported.
- **Envelope refusal rate:** fraction of corrupted envelopes refused (must be 100 %).
- **Leakage rate (H3a):** fraction of `d > k` queries whose A2/A3/A3r history bytes contain
  the gold value string.
- **McNemar table (H3b):** per `(X, k)` with `X ∈ {A2, A3}`, the 2×2 counts defined in
  section 7, the one-sided exact p-value and its Holm-adjusted value.
- **Overhead ratio:** `median(t_ctx2) / median(t_model)`, monotonic clock, per query, same
  process, A3 and A3r reported separately.
- **Budget audit:** the four invariants of section 4.1, checked on every prompt; plus the
  per-`(arm, k, seed)` totals `Σ|P|` and `Σ|H|`, and the rate of A1 prompts whose `H` equals
  A2's `H`.

## 7. Seeds, sizes, statistics

- Dev seeds: 2, used only for harness debugging, the determinism precheck and CI smoke. A dev
  seed has the same size as a test seed (64 sessions × 6 queries).
- Test seeds: 5 (≥ 3 required). Seed `i ∈ {0..4}` is the first 8 bytes (little-endian u64)
  of `SHA-256("CTX2-INFER-test-v1" || merge_sha_of_this_prereg || u8(i))`. They cannot be
  known before this document is merged.
- Per test seed: 64 sessions × 6 queries = 384 queries; 1 920 test queries per (arm, k).
- Paired comparisons use a session-clustered bootstrap, 10 000 resamples, fixed resampling
  seed `0xC72_1AFE`, 95 % percentile interval.
- **H3b test (direction and table).** For each `X ∈ {A2, A3}` and each `k ∈ {4, 8, 16}`,
  take all test queries with `d > k`, pooled over the 5 test seeds, and pair `X` with A0 on
  the same query (A0 outcome reused across `k`, section 4). Contingency table:

  | | A0 correct | A0 wrong |
  | --- | --- | --- |
  | **X correct** | `n11` | `b = n10` |
  | **X wrong** | `c = n01` | `n00` |

  Null hypothesis: `P(X correct, A0 wrong) ≤ P(X wrong, A0 correct)` (X has no advantage
  over A0 on evicted queries). Alternative (one-sided): `X` has the advantage, `b > c` in
  expectation. Exact p-value: `p = Σ_{i=b}^{b+c} C(b+c, i) · 2^-(b+c)`; if `b + c = 0`,
  `p = 1`. The family is the 6 tests `{A2, A3} × {4, 8, 16}`, Holm-adjusted at
  family-wise α = 0.05. All four counts, `p` and the adjusted `p` are reported for every
  test. The number of `d > k` queries per `k` is a property of the generated plan and is
  reported, not fixed in advance. McNemar treats pairs as independent although queries of a
  session share a history; the session-clustered bootstrap interval of the EM difference
  `X − A0` on the same queries is reported next to each test as the clustered view. The
  test is only a detector of an advantage; outcomes are worded "advantage detected" or "no
  advantage detected at this sample size", never "equivalent" or "no leakage".
- H0, H1, H2, H2p and H3a are exact-identity or exact-absence hypotheses: no statistical
  tolerance applies.

## 8. Compute and budget comparability

- Same host, backend, binary, thread count, `n_ctx`, decoding and `n_predict` for every arm
  and every `k`. Execution is organised per test seed: within a seed, the 14 arm-`k` pairs
  (A0, R, and A1/A2/A3/A3r × 3 `k`) are interleaved per session, order rotated by session
  index, to spread drift.
- Every query is one prefill + up to 8 decoded tokens in every arm; A3r adds no model forward
  pass (it restores working-context provenance only, not a KV-cache).
- **Determinism precheck** before any test call, on the same admitted host, backend, binary
  and thread count as the test runs: R on dev seed 0 (384 queries) run twice; generated text
  must be byte-identical on every query. If it is not, the backend is declared
  nondeterministic for this run and: H0 and strict H2 are `INDETERMINATE` (section 5), H2p
  and H1 (prompt-level) are evaluated as usual, EM results are still reported, and the
  limitation is printed in every report. Hypotheses are never redefined after the precheck.
  A CI smoke determinism pass on GitHub-hosted CPU does not substitute for this precheck.

### 8.1 Compute envelope (derived, not estimated)

| Item | Derivation | Model calls |
| --- | --- | --- |
| Naive full grid | 6 arms × 3 `k` × 5 seeds × 384 queries | 34 560 |
| A0, deduplicated over `k` | 1 × 5 × 384 | 1 920 |
| R, deduplicated over `k` | 1 × 5 × 384 | 1 920 |
| A1, A2, A3, A3r | 4 × 3 × 5 × 384 | 23 040 |
| **Test calls (planned)** | 1 920 + 1 920 + 23 040 | **26 880** |
| Determinism precheck | 2 runs × 384 dev queries | 768 |
| **Total on the admitted host** | 26 880 + 768 | **27 648** |
| CI smoke per CI run (GitHub-hosted, not the test host) | 6 arms × (2 sessions × 6 queries) at `k = 4`, + 1 repeat of R | 72 + 12 = 84 |

Deduplication removes 34 560 − 26 880 = 7 680 calls, i.e. the 2 × 2 × 5 × 384 repeated A0/R
calls for the second and third `k`. A2 and A3 are **not** deduplicated even though H1
predicts identical prompts: running both is what tests H0 and H1 end to end.

- **Call count is enforced.** The plan lists exactly 27 648 call records for the admitted
  host; the driver refuses to issue a call not in the plan, and a final count different from
  the plan makes the run `INVALID`. A transport-level failure may be retried once with the
  identical token-ID array; retries are counted and reported; a second failure is an
  unrecoverable runner failure.
- **Wall-clock ceiling.** A cell-equivalent is 384 calls (one arm, one `k`, one seed, or one
  precheck pass) with a cap of 60 minutes, i.e. a mean of at most 3 600 s / 384 = 9.375 s
  per call. The earlier draft's 6 × 3 × 5 = 90 cells imply a 90 cell-hour theoretical cap.
  With deduplication there are 14 cell-equivalents per seed (5 seeds → 70) plus 2 for the
  precheck, so the theoretical wall-clock ceiling is **72 hours**. Per seed unit the cap is
  14 hours; the precheck cap is 2 hours. This is a ceiling, not a duration estimate; no
  throughput figure is claimed, and the actual duration is measured and reported.
- **Resource ceilings.** One `llama-server` process with one slot, `n_ctx 4096`, a fixed
  thread count recorded in the plan, and one driver process; no concurrent arms, no
  concurrent jobs started by this track on the admitted host; GPU, if used, is one device
  through a non-CUDA ggml backend. Peak RSS of the server and the driver, and the artifact
  bytes written, are measured and reported. The RemoteOps admission request states the
  27 648-call count and the 72-hour ceiling; numeric memory or disk limits are set by the
  host's admission policy, not invented here.

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

- All preregistered calls (section 8.1: 26 880 test calls covering 6 arms × 3 `k` × 5 test
  seeds, A0/R deduplicated over `k`) run once. No early stop on results, no re-run to get a
  better number.
- Abort and mark the run `INVALID` (kept, with a defect report) on: model or binary SHA
  mismatch, runtime context length too small, any budget audit invariant failure (section
  4.1), call count different from the plan, unrecoverable runner failure, or a wall-clock
  ceiling hit (section 8.1). Do not substitute another host or backend silently, and do not
  derive replacement seeds.
- An H1, H2 `FAIL`, H2p or H3a failure is an integration defect: the run is kept, the defect
  is reported, the fix needs a new commit and a full re-run of every call, both runs
  reported.
- H3b is a statistical result, not an integration invariant. A rejection is reported as
  "advantage detected" with its table, and the prompts concerned are inspected and the
  inspection is published; it does **not** by itself mandate a fix or a re-run, because a
  one-sided test at α = 0.05 can reject by chance and retained examples can affect
  formatting or guessing. Only a defect found by that inspection (for example an H3a
  violation) triggers the defect rule above.
- An H2 `INDETERMINATE`, an H0 `INDETERMINATE`, an H4 failure, an E1 result in either
  direction, and an H0 confirmation are all published as they are. Negative results are
  never deleted.

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
- Distance convention: for every generated query, `d ≤ k` iff the answer turn is in the last
  `k` turns, including the boundary case `d = k` (retained) and `d = k + 1` (evicted).
- Leakage: for `d > k`, gold value absent from A2/A3/A3r history bytes.
- Restart: seal → open → anchored restore mid-session yields the same remaining prompts;
  every single-byte flip of the envelope is refused and leaves the live context unchanged.
- Raw-store tamper: a modified segment file fails prompt assembly closed.
- Token budget: given synthetic token arrays, A1 `|H| = B(s, k)`, `|T_pre|` and `|T_post|`
  equal across arms, `|P| = |T_pre| + |H| + |T_post|`, plan rejected when
  `|P| + n_predict > n_ctx`, and the plan call count equals 27 648 for the full test grid.
- H3b statistics: McNemar p-value and Holm adjustment against hand-computed tables,
  including `b + c = 0`.

### CI-only smoke plan (mechanics, not measurement)

GitHub-hosted `ubuntu-24.04`, CPU backend: build the pinned unpatched llama.cpp
(`llama-server`), download the Q8_0 GGUF and verify its SHA-256, run dev seed 0 with
2 sessions for every arm and `k = 4` (84 calls, section 8.1). Assert only: prompt identity
A3 == A2, restart prompt identity A3r == A3, all budget audit invariants including the
server-reported prompt token count, determinism of R across two runs, corrupted-envelope
refusal. The job prints `NOT_MEASURED` and records no metric; its EM values are not results.

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

| Cell | EM | Prompt identity (H1) | Restart identity (H2) | Restart prompt identity (H2p) | Leakage (H3a) | McNemar (H3b) | Budget audit | Overhead |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| all | TBD | TBD | TBD | TBD | TBD | TBD | TBD | TBD |

## 13. Pre-freeze revision log

Revisions made before merge; the frozen version is the merged one.

- **r0** (2026-11-09, PR #909 head `37e33b38`): initial draft.
- **r1** (2026-11-09): protocol clarifications from ChatGPT review 5827772855 and RemoteOps
  #1208 checkpoint 6743298395, and chatgpt-codex-connector findings 4514172305 and
  4514172332. No measurement was made between r0 and r1.
  1. Exact token-budget accounting on final token-ID arrays, including chat-template pieces
     and boundary handling, with per-prompt and per-arm audits (section 4.1).
  2. One-sided McNemar direction, contingency table and Holm family for H3b; non-rejection
     is not equivalence (sections 5, 7).
  3. Precheck failure makes strict H2 (and H0) `INDETERMINATE`; prompt-only H2p is reported
     separately and H2 is not redefined (sections 5, 8).
  4. Compute envelope: 34 560 naive calls, 26 880 test calls after A0/R deduplication over
     `k`, 27 648 on the admitted host with the precheck, 72-hour theoretical ceiling (90
     cell-hours for the naive grid), resource ceilings (section 8.1).
  5. One-based query distance so that `d ≤ k` means retained and `d > k` evicted; last
     bucket `33–48` (section 3; codex 4514172305).
  6. Leakage split into H3a (byte-level hard invariant) and H3b (statistical, does not by
     itself trigger a fix and re-run) (sections 5, 10; codex 4514172332).
