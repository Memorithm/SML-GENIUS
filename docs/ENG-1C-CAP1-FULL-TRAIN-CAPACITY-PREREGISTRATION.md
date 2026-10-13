# ENG-1c/CAP1 — frozen full-TRAIN context capacity scan, NO predictive model

**Preregistered 2026-10-13 before any CAP1 code, fixtures, experiments or
capacity measurements.** The corpus identities are already immutably frozen
in docs/ENG-1C-FROZEN-CORPORA.md (PR #533); no language model has yet been
evaluated on these corpora.

## Research question

What is the ACTUAL first capacity violation for a fully exact,
collision-free, 64-lane reverse-suffix trie on each new real-text TRAIN
stream at a fixed depth, under the existing ENG-1b cardinality limits?
This is a TRAIN-only storage feasibility observation, not training a
Witten–Bell model, selecting model depth, reporting validation/test
quality, or claiming a language improvement.

Use both independent frozen Project Gutenberg sources, their already
registered byte sizes and SHA256. A source byte drift is a hard
failure; do not silently update hashes or normalize differently.

## Exactly frozen sources and deterministic split

- French Dumas/Maquet Gutenberg #17989 raw source SHA256
  9ea7a8b127a9dfa36de0b0cf9c2e26293beea5c37a0fc988fb9308b63160f282
  (784416 bytes); Gutenberg LF-body SHA256
  24ebc9a0037fc57bf93bf3ac6788c5ca64a9a6bbaf92b9fdbdfe6ca0f4114dde
  (747855 bytes); TRAIN initial 598284 bytes; 598208 next-byte events.
- English Austen Gutenberg #1342 raw SHA256
  3f6bb9d6f78e0293b56acd4714dd68cb7d6d1d293402031ce9d5a216bcaf9d75
  (772386 bytes); LF-body SHA256
  e79303d04de9260489f9d5a2ad4aae5776680fcec484f2e732d6ded7411bf402
  (737950 bytes); TRAIN initial 590360 bytes; 590272 next-byte events.
- Download public HTTPS only on a disposable hosted Ubuntu VM, with
  bounded time/retries and TLS certificate checks. Reuse the exact
  CR-stripping and marker-exclusive AWK-normalization contract of
  ENG-1c/0, verify raw AND normalized SHA256 and lengths. Write only
  the TRAIN prefix floor(8*body_len/10) into a temporary input file.
  Do NOT pass any VAL or TEST bytes into the scanner and do NOT publish
  Gutenberg text as an artifact.
- The existing static 64 contiguous lane treatment is exact:
  L=floor(TRAIN_len/64), t=1..L−1 per lane, independent reset at
  each lane boundary, residual discarded. No shuffling or
  text-dependent sampling.

## Fixed scan algorithm and maxima

- Candidate depths **[0,2,4,7,12,16,24,32]**, no opportunistic
  depth search or threshold adjustment after any observed result.
- One *fresh Linux process* per corpus × candidate depth, 16 bounded
  executions total. Each process reads only the pinned TRAIN file
  (from the current temporary checkout); frozen book identity,
  train byte length and expected target-event count are checked.
- Exact, collision-free reverse-suffix DAG counting: root ID 0,
  each non-root edge keyed by (parent:u32, older_byte:u8) in an
  ordered map with full key equality; each distinct observed
  (node_id:u32, next_byte:u8) pair in an ordered set. The resulting
  node count includes the root and exactly matches
  ElasticNGramTrie at any fully scanned fixture under the same depth.
  No lossy hash/signature, approximate HLL, stochastic counter or
  truncated suffix is permitted.
- **Capacity caps remain frozen**: max **250000** nodes,
  **500000** distinct node/follower pairs, candidate depth <=32.
  The *non-training scan* may inspect up to 650000 next-byte events
  to observe first node/pair saturation after the preexisting
  500000 TRAIN-event model limit. It does NOT raise the model limit.
- At the FIRST prospective node or follower-pair insertion that
  would exceed the frozen cap, emit a single STOP row with the
  fully completed events so far, current lane/timestamp, currently
  stored node/pair cardinalities and stop kind
  NODE_CAPACITY_STOP or PAIR_CAPACITY_STOP. No silent eviction,
  skipped rows, post-stop downsample or continuation.
  Cardinailities at STOP are exact for the scanned prefix, NOT
  totals over the full frozen TRAIN stream; never extrapolate
  or report them as complete whole-book counts.
- If the entire TRAIN is scanned before a key cap is exhausted,
  status SCAN_COMPLETE. This still does NOT authorize model
  training on that full corpus because 598208/590272 events exceed
  the existing hard 500000 model-event bound. A separate SML
  resource policy must approve any future increase.
- Emit full next-byte structural denominator, events actually
  completed, context nodes, follower pairs, consumed depth,
  status, and Linux VmHWM **process peak KiB**. VmHWM includes
  process/allocator/hash-tree metadata and is not exact
  per-object RAM, GPU memory or a production resource cap.
  At STOP, VmHWM is an early-prefix observation, not a full model
  allocated footprint. Missing/nonpositive metrics fail the
  diagnostic rather than being set to zero.
- No TRAIN target distribution, posterior, log-loss, accuracy,
  model gradient, VAL or TEST evaluation is computed.
- The scientific result can be all STOP; this is a valid negative
  finding and must NOT make engineering CI red. Bad SHA,
  invalid split/lane identity, integer overflow, incorrect
  scanner/oracle agreement, missing rows, missing target labels
  or process crash ARE hard engineering failures.

## Frozen unit oracles

1. Deterministic 64x40 byte fixtures, for depths 0/4/7/12/32:
   same exact node/follower counts as reference
   ElasticNGramTrie::train with identical legal caps.
2. Two 12-byte histories identical in the newest 7 bytes but
   different older symbols remain different at depth 12.
3. Tiny artificially bounded node or pair budgets stop on
   an attempted insertion; no per-lane leakage across 64 lanes,
   no falsely reported SCAN_COMPLETE on a partial prefix.
4. All full TRAIN lengths and target-event counts are precisely
   checked; missing/invalid input, unsupported depth, empty
   corpus, invalid caps, overflow fail closed.
5. SML previous ENG0/1a/1b and ENG2a/2b code, dataset identity
   and all A6-E4/E5 science remain unchanged.

## Admission decision and future gates

CAP1 is strictly observational. At full TRAIN size both books
are already known to violate the 500000-event model ceiling.
The scan answers WHERE the exact-node or follower-pair resource
caps bind under a limited, bounded, TRAIN-only memory footprint;
it cannot certify full-model admissibility from a partial scan.

A later **CAP2** can preregister a new scalable model representation,
a measured memory/latency budget and an exact source-bound full-data
training method. A later model-quality study must freeze candidate
depths, VAL policy selection and protected TEST gate *before* any
language-model scoring. No claim about CTW/PPM, RAM/VRAM or
SML25B default deployment follows from CAP1.

## Pipeline and validation

One hosted Ubuntu 24.04 read-only PR workflow with pinned Rust 1.89
checks fmt/check/strict Clippy, tests cap1 oracles and downloads only
these two exact public corpora. Run all sixteen frozen combinations,
capture their exact output and source compiler/provenance to tiny
text artifacts, NEVER the raw/normalized book bodies. General CI
must be GREEN on the exact PR head for merge.
