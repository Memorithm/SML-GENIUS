# G1 / CONTEXT-3 — search-key encoder v2 (pre-registration)

Status: **PREREGISTERED — NOT_MEASURED.** Written after the v1 run on
`g1-pes2o7-v1` and before any v2 number on that corpus. Changing any value
below after a v2 run is v3.

## Why v2
The v1 run (RemoteOps #1230, SML a6e4aaf4) gave `gold@1` = 0.032 for the
exhaustive scan: one SimHash over a 64-token segment is dominated by the 56
tokens a query does not contain, so the target segment is not the nearest key.
The index itself works (R@1 vs exhaustive 0.798 at about 10x lower p50).
v2 changes only what is indexed.

## Encoder v2 (`g1_realtext_key::simhash_windows_v2`, `key=simhash64-sha256-v2-w8s<S>`)
1. Token rule, token hash and per-window SimHash are exactly v1.
2. Each haystack segment is indexed as one record per window of
   `WINDOW_V2 = 8` tokens (= `PARAMS_V1.query_tokens`) at offsets
   `0, S, 2S, …` while a full window fits; a segment shorter than 8 tokens
   gives one record over all its tokens.
3. Strides pre-registered: `S = 1` (primary) and `S = 8` (secondary,
   non-overlapping windows, about 8x fewer records).
4. Queries keep the v1 key (one window of 8 tokens). Derivation v1 (segments,
   queries, targets, split, near-dup refusal) is unchanged.
5. `gold@1` maps the top-1 record back to its parent segment; it is a hit iff
   that parent is the derived target. `N` in the rows is the record count.

## Run (Dell, merged SML main, corpus `g1-pes2o7-v1` as pinned)
- `g1_latency_rss --realtext <manifest> <corpus> --key v2s1`: 2 processes.
- `g1_latency_rss --realtext <manifest> <corpus> --key v2s8`: 5 processes.
- Same cells as v1 (exhaustive; neighbor and global_wfull, radius 1–2,
  caps 32–256), same host class as the v1 run.

## Criteria (primary arm `v2s1`, first process; later processes must give identical quality columns)
- **H1 quality:** exhaustive `gold@1 >= 0.90`. Expected below 1.0 only where
  the same 8-token window occurs in several eval segments.
- **H2 quality at bounded cost:** `global_wfull` radius 2 cap 32
  `gold@1 >= 0.80` **and** p95 `<= 2000 µs` (the v1 exhaustive p95 on the
  Dell was 1978 µs).
- Verdict: `KEY_V2_GO` if H1 and H2; `KEY_V2_QUALITY_ONLY` if H1 only;
  `KEY_V2_NO_GO` otherwise. `v2s8` is reported descriptively, without a
  verdict.

## Known limits
- Stride 1 is close to an exact 8-gram index; a GO shows the bench can do
  lexical span recall at this scale, not semantic retrieval.
- Index size grows about 57x (stride 1); build time and RSS are reported,
  not judged.
- Near-duplicate refusal stays provisional (protocol rule 5).
