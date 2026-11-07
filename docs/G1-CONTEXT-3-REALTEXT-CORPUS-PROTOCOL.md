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

## Search-key encoder v1 (pre-registered; `g1_realtext_key`, `key=simhash64-sha256-v1`)
Frozen before any real-text number is seen. Changing anything is v2.
1. Per token `t` (from rule 1 above): `h(t) = u64_be(sha256("g1k-tok|"+t)[0..8])`.
2. Key of a token list (segment or query) = SimHash over the token
   **multiset** (repeats count, order ignored): for bit `b` (0 = LSB),
   `c_b = #{h(t) has bit b} - #{h(t) lacks bit b}`; key bit `b = 1` iff
   `c_b > 0` (ties -> 0). Empty list => refusal (no zero key).
3. Segment keys are indexed (one record per segment, segment id = 1-based
   haystack order); query keys are searched. Raw bytes stored in the bench
   are placeholders; only keys are retrieved.
4. Lexical only. No claim that key Hamming distance tracks relevance.

## Flagged real-text arm (`g1_latency_rss --realtext <MANIFEST> <CORPUS>`)
Opt-in only; no default corpus and no pin in the repo. Missing args, file
errors, length/SHA mismatch, near-duplicate or zero-query refusal => exit 2
with `G1_LAT_REFUSE`. **Path confinement (CLI):** both `<MANIFEST>` and
`<CORPUS>` must be relative paths under the process CWD; absolute / `..` /
empty paths are refused (`UnsafePath`). After the lexical check,
`confine_regular_file` walks each component with `symlink_metadata`, refuses
any symlink (including mid-path directory symlinks), requires a regular file,
and checks that the canonical path stays under the canonical CWD
(`SymlinkEscape`). Lexical `Path::components` alone is not filesystem
confinement. Meta: `synthetic=false arm=realtext derivation=v1
corpus_id=.. sha256=.. key=simhash64-sha256-v1`, refused by
`g1_latency_aggregate`. Rows: dist `realtext`, same columns plus `gold@1`
(top-1 segment == derived target). `R@1` remains "top-1 distance == oracle
nearest key distance". **NOT_MEASURED** until the user pins a corpus + SHA.

## CI policy
Only hand-written in-repo fixtures (`crates/sml-store/tests/fixtures/g1_realtext/`)
are used in CI. No corpus is downloaded in CI.

- `tiny.tsv` (459 bytes): loader/split tests; yields `NoQueries` under `PARAMS_V1`.
- `smoke.tsv` (3344 bytes, sha256
  `6d8212da1faeb7269eaf7caf0ee207cc6653d9a0fa2805cc56d5d45193e15b65`, 8 docs,
  `split_mod 2 eval_residue 0` -> 5 eval / 3 train): **smoke fixture only**.
  `tests/g1_realtext_smoke.rs` checks `load_pinned` -> `build_eval_set(PARAMS_V1)`
  -> `simhash_v1` and that the meta is refused by the synthetic aggregator.
  `--realtext smoke.manifest smoke.tsv` exits 0 locally. Any numbers it prints
  (5 segments, 5 queries) are **not measurements** and must not be cited.

CI: `.github/workflows/g1-realtext-smoke.yml` (GitHub-hosted `ubuntu-24.04`) runs this
smoke and asserts exit 0 + meta substrings only; CI smoke != validation.

Fixture smoke != validation. Every real corpus remains **NOT_MEASURED** until
the user chooses a corpus and pins its SHA-256.

## Pinned corpora
Committed manifests live in `docs/g1-realtext/` (see its README). First pin:
`g1-pes2o7-v1` (sha256 `90268073…8f8d7`, 14 419 687 bytes), built on the Dell
by RemoteOps #1227. Still **NOT_MEASURED** until a `--realtext` run on main.

## Search-key encoder v2 (pre-registered after the v1 run)
See `docs/G1-CONTEXT-3-KEY-ENCODER-V2-PREREGISTRATION.md`: windowed SimHash
(8-token windows, stride 1 or 8), opt-in with `--key v2s1|v2s8`. v1 stays the
default and is unchanged.
