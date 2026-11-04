# G1 / CONTEXT-3 — real-text corpus fixture protocol (pre-registration scaffold)

Status: **PROTOCOL ONLY — NOT_MEASURED.** No real-text corpus has been pinned,
no run has been made, no score exists. Nothing here is validation.

## Purpose
Admit a real-text corpus to G1 retrieval runs beyond the synthetic fixture
(`examples/support/g1_fixture.rs`) under rules fixed *before* any number is seen.

## Corpus admission (enforced by `sml_store::g1_realtext_corpus::load_pinned`)
1. **Manifest** `G1_REALTEXT_MANIFEST v1` with exactly once each:
   `corpus_id`, `source` (URL/citation), `license`, `sha256`, `byte_len`,
   `split_mod`, `eval_residue`. Unknown/duplicate keys are refused.
2. **SHA pin:** `sha256` must be 64 lowercase hex of the exact corpus file
   bytes. `UNPINNED`, missing, wrong length, or mismatch => refusal (fail-closed).
   The manifest is committed and reviewed before the corpus is used for any run.
3. **Format:** UTF-8, one `doc_id<TAB>text` per `\n`-terminated line; unique ids,
   non-empty text, no tab/CR in text.
4. **Split by document:** `u64_be(sha256(doc_id)[0..8]) mod split_mod == eval_residue`
   => eval, else train. Never split inside a document. Both splits non-empty.
5. **Leakage:** identical text in train and eval => refusal. (Exact-duplicate
   check only; near-duplicate detection is a documented gap, to be added before
   any reported result.)

## Harness entry points
- Loader: `load_pinned(manifest_text, corpus_bytes) -> PinnedRealTextCorpus`
  (`train()`, `eval()`, `manifest()`); no unchecked constructor.
- The synthetic path (`g1_latency_rss`, `context_retrieve_*` tests,
  `g1_latency_aggregate`) is unchanged and remains the default arm. A real-text
  arm must be a separate, explicitly flagged arm whose meta line records
  `corpus_id` and `sha256`; it must never be aggregated with synthetic runs.
- Queries/targets derivation from eval docs and the segmenter for real text are
  **not yet defined**; they must be pre-registered in a follow-up before wiring.

## CI policy
Only the 459-byte hand-written fixture
(`crates/sml-store/tests/fixtures/g1_realtext/`) is used in CI. No corpus is
downloaded in CI.
