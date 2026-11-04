# G1 / CONTEXT-3 pinned real-text corpora

Manifests here are committed and reviewed **before** any `--realtext` run, as
`docs/G1-CONTEXT-3-REALTEXT-CORPUS-PROTOCOL.md` requires. The corpus bytes are
not in Git; they stay on the host that built them. Nothing here is a score.

| corpus_id | manifest | corpus bytes | built by |
|---|---|---|---|
| `g1-pes2o7-v1` | `g1-pes2o7-v1.manifest` | 14 419 687 B, sha256 `90268073…8f8d7`, Dell `~/sml-corpora/g1-realtext/g1-pes2o7-v1-170d1164…/corpus.tsv` (named after the PR merge ref of RemoteOps #1227, not its head 536f6c2d) | RemoteOps #1227, run 41864785969 |

## g1-pes2o7-v1
- Source: `peS2o-0007.json.gz` (common-pile/peS2o_filtered, sha256 `453b3115…`).
  No SML run has read this shard (S5h TRAIN ends inside 0001; EVAL2–EVAL5 are
  0002–0005; 0006 is reserved for BAF-H4).
- Admission: same screens as `sml-pes2o-s5h-stage` (shape, CC-BY/CC0 claim under
  the committed operator review, no email, no duplicate). Counts: 2454 rows read,
  43 claim exclusions, 11 email exclusions, 0 duplicates, 2400 kept.
- Each article is flattened to one line and cut at a space to at most 6000
  characters. `split_mod 2`, `eval_residue 0` (about 1200 eval documents).
- Per-article attribution: `attribution.tsv` next to the corpus on the Dell
  (sha256 `7cfc1f96…727d3a`).
- Status: **NOT_MEASURED**. Near-duplicate refusal (rule 5) is still provisional.

## Runs on g1-pes2o7-v1
- **v1 key** (`simhash64-sha256-v1`), RemoteOps #1230, SML a6e4aaf4, Dell, 5
  identical processes: 18 098 segments, 1228 queries. `gold@1` = **0.032** even
  for the exhaustive scan: the v1 key of a 64-token segment does not carry an
  8-token span. Index side: `global_wfull` r2 cap 32 matches the exhaustive
  top-1 distance (R@1) 0.798 at p50 191 µs vs 1948 µs exhaustive. Negative on
  the key, kept as is. Encoder v2 is pre-registered in
  `docs/G1-CONTEXT-3-KEY-ENCODER-V2-PREREGISTRATION.md`.
