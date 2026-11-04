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
- Query/target derivation, segmentation and near-dup refusal: see
  "Derivation rules v1" below (`sml_store::g1_realtext_eval`).

## Derivation rules v1 (pre-registered; `g1_realtext_eval::PARAMS_V1`)
Frozen before any real-text number is seen. Changing any value is v2.
1. **Normalization:** Unicode lowercase; every non-alphanumeric char is a
   separator; tokens = maximal alphanumeric runs.
2. **Segmentation:** per document, non-overlapping windows of
   `segment_tokens=64` tokens; a trailing window with < `min_tail_tokens=16`
   tokens is dropped. Segments never cross documents. Id `<doc_id>#<k>`.
3. **Haystack:** segments of **eval** documents only. Train docs are never
   queried or indexed by the eval arm.
4. **Query/target:** one query per eval doc with >= 1 segment. With
   `H(x) = u64_be(sha256(x)[0..8])`: target `k = H("g1q-target|"+doc_id) mod n_segs`;
   offset `= H("g1q-offset|"+doc_id) mod (len(target) - query_tokens + 1)`;
   query = the contiguous `query_tokens=8`-token window at that offset. The
   target segment is the only gold answer. This is a **lexical span-recall
   probe**, not a semantic-retrieval test. Zero queries => refusal.
5. **Near-duplicate refusal (PROVISIONAL, unvalidated):** for every
   (train, eval) doc pair, refuse if the normalized token sequences are equal,
   or if `shingle=5`-token shingle-set Jaccard >= 0.8. Threshold is a
   placeholder, not calibrated on labelled data; pairwise O(train x eval),
   suitable only for small corpora. Runs before segmentation.
6. **Meta:** a real-text arm prints
   `synthetic=false arm=realtext derivation=v1 corpus_id=<id> sha256=<hex>`
   (`RealTextEvalSet::meta_fields`). `g1_latency_aggregate` refuses any run
   without `synthetic=true`, so real-text runs never aggregate with synthetic.

### Not yet wired (next hook)
`g1_latency_rss` still runs the synthetic arm only. The real-text arm needs a
pre-registered text -> 64-bit retrieval-bitfield encoder (undefined; the
synthetic fixture draws words directly). Next step: pre-register that encoder,
then add an opt-in flag that calls `load_pinned` + `build_eval_set` and prints
the meta above. No corpus is pinned until the user chooses one and its SHA.

## CI policy
Only the 459-byte hand-written fixture
(`crates/sml-store/tests/fixtures/g1_realtext/`) is used in CI. No corpus is
downloaded in CI.
