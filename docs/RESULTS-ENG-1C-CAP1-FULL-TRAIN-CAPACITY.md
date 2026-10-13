# ENG-1c/CAP1 — full-TRAIN exact context capacity on independent FR/EN text

**Measured 2026-10-13 UTC; this is a negative CAPACITY result, NOT a
model-quality result.** All figures were copied from the raw immutable
GitHub-hosted logs after the capacity protocol and code had already
been preregistered and committed.

## Exact qualification provenance

- Source repo: Memorithm/SML-GENIUS.
- Frozen-first science protocol commit:
  d3aaa138d7344d557bcbe4428b396818a337b028.
- Source HEAD actually measured:
  e82652a80d8f8102d987f377e7ed1475e3c014f3.
- Hosted [workflow run 38354832307](https://github.com/Memorithm/SML-GENIUS/actions/runs/38354832307),
  [job 115155168607](https://github.com/Memorithm/SML-GENIUS/actions/runs/38354832307)
  completed SUCCESS: source hashes, Rust 1.89 fmt/check/Clippy,
  exact ENG-1b count oracles, release build and all 16 real-book
  capacity scans.
- [Original unmodified evidence artifact 11792736149](https://github.com/Memorithm/SML-GENIUS/actions/runs/38354832307/artifacts/11792736149),
  7,059 GitHub-reported compressed bytes, contains only small
  source/environment/scan manifests (not any Gutenberg text).
  Artifact bytes were not independently re-downloaded/hashes checked.
- Corpus source was re-downloaded publicly over HTTPS on GitHub-hosted
  Ubuntu, raw SHA256 and LF-normalized body SHA256 compared to the
  independently frozen [ENG-1c Stage 0](ENG-1C-FROZEN-CORPORA.md).
- The scanner input is exclusively TRAIN prefix;
  VAL/TEST not passed, no model .train(), probability scoring,
  log-loss, accuracy or depth selection.
- One fresh CPU process per profile; sorted full-equality BTreeMap
  (parent, older byte) and BTreeSet (node id, next byte). No
  lossy hashing or truncation. 64 isolated contiguous lanes.
- Limits: 250,000 context nodes including root and 500,000
  distinct context/next-byte pairs; max 650,000 scan events
  **for observation only**. The actual ENG-1b model's separate
  500,000 TRAIN-event hard limit was NOT changed.

## Observed 16 independent scan runs

**Full frozen TRAIN desired target counts:** FR 598,208; EN 590,272.
Statuses:

- SCAN_COMPLETE: all TRAIN positions scanned; exact final
  cardinalities for that depth.
- NODE_CAPACITY_STOP: first attempted new exact context beyond 250,000
  nodes; the event counter shows only the last FULLY PROCESSED TRAIN
  events. Stored node and pair counts are from this PARTIAL prefix and
  cannot be extrapolated to the full text.
- All rows separately report a model-event-limit **STOP** because
  both full TRAIN streams exceed 500,000 scored positions, regardless
  of the context cardinality result.

| Corpus | Depth | Status | Fully completed target events | Stored nodes | Stored (node, next byte) pairs | VmHWM process KiB |
|---|---:|---|---:|---:|---:|---:|
| FR Dumas | 0 | SCAN_COMPLETE | 598,208 | 1 | 94 | 2,616 |
| FR Dumas | 2 | SCAN_COMPLETE | 598,208 | 1,368 | 9,480 | 2,768 |
| FR Dumas | 4 | SCAN_COMPLETE | 598,208 | 40,329 | 117,282 | 5,288 |
| FR Dumas | 7 | NODE_CAPACITY_STOP | 201,178 | 250,000 | 378,114 | 14,736 |
| FR Dumas | 12 | NODE_CAPACITY_STOP | 38,757 | 250,000 | 286,812 | 14,336 |
| FR Dumas | 16 | NODE_CAPACITY_STOP | 23,481 | 250,000 | 273,054 | 14,328 |
| FR Dumas | 24 | NODE_CAPACITY_STOP | 13,090 | 250,000 | 263,048 | 14,448 |
| FR Dumas | 32 | NODE_CAPACITY_STOP | **9,089** | 250,000 | 259,076 | 14,424 |
| EN Austen | 0 | SCAN_COMPLETE | 590,272 | 1 | 96 | 2,588 |
| EN Austen | 2 | SCAN_COMPLETE | 590,272 | 1,529 | 10,305 | 2,772 |
| EN Austen | 4 | SCAN_COMPLETE | 590,272 | 41,802 | 118,117 | 5,336 |
| EN Austen | 7 | NODE_CAPACITY_STOP | 194,828 | 250,000 | 372,880 | 14,560 |
| EN Austen | 12 | NODE_CAPACITY_STOP | 40,660 | 250,000 | 285,104 | 14,440 |
| EN Austen | 16 | NODE_CAPACITY_STOP | 22,767 | 250,000 | 272,076 | 14,400 |
| EN Austen | 24 | NODE_CAPACITY_STOP | 13,288 | 250,000 | 262,935 | 14,612 |
| EN Austen | 32 | NODE_CAPACITY_STOP | **9,482** | 250,000 | 259,323 | 14,564 |

At depth 32, the node cap is reached after only about **1.52%**
(FR) or **1.61%** (EN) of the eventual TRAIN next-byte positions.
This fraction is about completed TRAIN events and may depend on
fixed lane order. It does NOT imply a total-count extrapolation
for the remainder of the book. At depth 7 the respective fractions
are about 33.6% (FR) and 33.0% (EN).

The pair-count threshold of 500,000 never bound FIRST in any
candidate under the existing 250,000-node cap. This does NOT prove
that full TRAIN would fit 500,000 follower pairs, because all
high-depth scans stopped early on the node cap.

## Qualification semantics and memory warning

All scans were performed with an **index-cardinality-only**
temporary BTreeMap/BTreeSet, not an actual immutable full
ElasticNGramTrie model containing per-node Witten-Bell histogram,
updatable stats, or a verified memory-mapped backend.

The reported VmHWM is therefore a physical PROCESS peak at full
scan (depth 0,2,4) or partial scan (>=7). It is **not** the
peak memory of a real ENG-1b model on the entire corpus, nor
proof of a hard RSS memory controller. File downloading and
other environment allocations are separate from each scanner
process; the output is only an observed diagnostic.

A status SCAN_COMPLETE at depth 4 is **NOT a model PASS**:
both books still exceed the existing 500,000-event hard training
bound. A status NODE_CAPACITY_STOP is a valid scientific negative
finding and must be retained rather than fixed by implicit
downsampling, smaller context depth or uncapped allocation.

## Decision / next gate

**CAP1_RESULT=BLOCKED_FULL_CORPUS_MODEL_AT_ALL_FROZEN_DEPTHS**.

- Depths 0/2/4: exact context cardinalities fit the current
  250,000/500,000 node/pair caps, but the 500,000 TRAIN-event
  limit blocks full-book model fitting.
- Depths 7/12/16/24/32: exact context cardinality hits the
  250,000 node limit before TRAIN ends. The full-book total
  remains unknown until a separately admitted bounded **CAP2**
  scan or scalable model design qualifies its memory needs.
- A new CAP2 preregistration must explicitly freeze whether
  to increase event/node/pair limits, use a backed compact or
  sharded exact trie, or train on a deterministic limited
  population. It must control transient RSS and avoid choosing
  model depth based on hidden corpus losses.
- The later actual ENG-1c language-quality trial must retain
  256-way Witten-Bell parity on equal-depth representations,
  select depth only on VAL and score untouched TEST after
  freezing the winner. A depth reduction is NOT a lossless
  representation change.
- ENG-2b/H1 PR #536 addresses a bounded **in-process heap**
  format swap only; it does not remove full-book training
  capacity limits, provide GPU data placement or qualify
  a real ElasticXxx generic production resource adapter.

Reference tracking:
- https://github.com/Memorithm/SML-GENIUS/issues/530
- https://github.com/Memorithm/ElasticXxx/issues/244
