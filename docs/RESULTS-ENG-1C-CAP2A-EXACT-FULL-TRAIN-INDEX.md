# ENG-1c/CAP2a — exact external-memory suffix counts on complete frozen TRAIN

**Measured 2026-10-13 UTC, after scientific preregistration and after
implementation was committed.** This is a successful *external index
cardinality* qualification and simultaneously an explicit
**capacity STOP for all current fitted ElasticNGramTrie configurations**.
It is **not** a fitted probabilistic model or a real-text quality result.

## Provenance: source and evidence

- Frozen pre-code protocol:
  [ENG-1C-CAP2A-EXTERNAL-SORT-PREREGISTRATION.md](ENG-1C-CAP2A-EXTERNAL-SORT-PREREGISTRATION.md),
  committed first as 38aac3c5ffe7b0a342e3a9872732960f42641067.
- Exact measured code HEAD:
  8ec4047cc916c14060a5e33e55c9076df7b888a6.
- Hosted independent Linux jobs (Ubuntu 24.04, Rust 1.89):
  [dedicated Actions run 38356769038](https://github.com/Memorithm/SML-GENIUS/actions/runs/38356769038).
  EN job 115161382146 and FR job 115161382364
  both concluded SUCCESS after source SHA, fmt/check/Clippy,
  CAP2a full-key/oracle/integrity tests, release build
  and eight full-TRAIN scans per corpus.
- Only small manifests (no book texts or 34-byte sorted key files):
  [FR artifact 11792759706](https://github.com/Memorithm/SML-GENIUS/actions/runs/38356769038/artifacts/11792759706)
  and [EN artifact 11792079967](https://github.com/Memorithm/SML-GENIUS/actions/runs/38356769038/artifacts/11792079967).
  Reported GitHub compressed sizes 4,248 and 4,253 bytes
  respectively. Artifact bytes have **not** been independently
  re-downloaded for cryptographic archive rehashing.
- Input identity: [ENG-1c frozen source/body SHA256 manifest](ENG-1C-FROZEN-CORPORA.md),
  independently preregistered and verified before indexation.
  FR Gutenberg #17989 Dumas/Maquet TRAIN 598284 bytes,
  598208 next-byte events.
  EN Gutenberg #1342 Austen TRAIN 590360 bytes,
  590272 next-byte events.
- Input to the binary is **only** the exact TRAIN prefix
  over 64 disjoint contiguous lanes, t>=1. The VAL and
  TEST contents were not loaded into the indexer or scored.

## The 16 exact, whole-TRAIN results

These are **cumulative** counts of all distinct exact suffix
contexts of depths 0..d and all distinct (context, next-byte)
follower pairs, not counts of only the depth-d level.

Every row has status **FULL_SCAN_COMPLETE**, and every row's
multiset checksums and each sorted-run checksum agreed on the
full record population. All 16 used at most 96 sorted runs,
1 GiB of sorted scratch and a hosted 256 MiB **virtual
address-space** OS ceiling (not 256 MiB observed RSS).

| Book | Max depth | Exact distinct contexts | Exact context/next-byte pairs | Emitted 34B rows | Sorted runs | Peak scratch bytes | VmHWM process KiB |
|---|---:|---:|---:|---:|---:|---:|---:|
| FR | 0 | 1 | 94 | 598,208 | 3 | 20,339,072 | 11,388 |
| FR | 2 | 1,368 | 9,480 | 1,794,560 | 7 | 61,015,040 | 11,448 |
| FR | 4 | 40,329 | 117,282 | 2,990,656 | 12 | 101,682,304 | 11,692 |
| FR | 7 | 491,517 | 799,782 | 4,784,320 | 19 | 162,666,880 | 11,768 |
| FR | 12 | 2,597,815 | 3,131,101 | 7,772,480 | 30 | 264,264,320 | 12,188 |
| FR | 16 | 4,816,161 | 5,395,227 | 10,161,856 | 39 | 345,503,104 | 12,364 |
| FR | 24 | 9,522,629 | 10,117,697 | 14,937,536 | 57 | 507,876,224 | 13,096 |
| **FR** | **32** | **14,281,533** | **14,878,280** | **19,709,120** | **76** | **670,110,080** | **13,592** |
| EN | 0 | 1 | 96 | 590,272 | 3 | 20,069,248 | 11,364 |
| EN | 2 | 1,529 | 10,305 | 1,770,752 | 7 | 60,205,568 | 11,296 |
| EN | 4 | 41,802 | 118,117 | 2,950,976 | 12 | 100,333,184 | 11,588 |
| EN | 7 | 471,036 | 759,560 | 4,720,832 | 19 | 160,508,288 | 11,716 |
| EN | 12 | 2,477,266 | 2,995,126 | 7,669,312 | 30 | 260,756,608 | 12,200 |
| EN | 16 | 4,641,299 | 5,208,546 | 10,026,944 | 39 | 340,916,096 | 12,320 |
| EN | 24 | 9,257,014 | 9,841,222 | 14,739,136 | 57 | 501,130,624 | 13,024 |
| **EN** | **32** | **13,932,625** | **14,519,708** | **19,447,232** | **75** | **661,205,888** | **13,680** |

The FULL-TRAIN exact counts at depths 0, 2 and 4
**match CAP1's independent previously frozen complete
context/follower oracles**:
FR (1/94, 1368/9480, 40329/117282);
EN (1/96, 1529/10305, 41802/118117).
This is a scientifically useful deterministic reference,
but one input per language does not establish generalization.

At depth 32, a complete sorted record stream corresponds to
**670.1 MB** FR / **661.2 MB** EN of ephemeral scratch
files. Observed process VmHWM is **13,592 KiB** FR /
**13,680 KiB** EN. This VmHWM is the external cardinality
scanner's peak, **not** the memory required to fit a
14-million-context histogram model. The actual source
TRAIN byte array, sort buffers and Linux/runtime overhead
are included in the process sample.

## External-memory integrity conditions

Each 34-byte fixed binary record contains the exact
depth-tagged 0..32-byte chronological suffix, zero-filled
unused bytes, and full next-byte symbol. There is no
hash-based equality or approximate cardinality estimator.

Runs hold at most 262144 sorted records each; one
single-pass bounded k-way merge reads at most 96 run files
with 32768-byte buffered readers. Before-sort and
after-merge double, commutative 64-bit multiset digests
agree, as do all per-run order-sensitive rolling digests,
sortedness and exact record/byte-length totals.

**Checksum caveat:** these record digests are explicitly
noncryptographic integrity aids; a maliciously constructed
collision is possible. Source corpus identities, by
contrast, were verified with source/body SHA256 before
scanning. Quota/invalid input/tampering negative controls
were exercised in Rust unit tests. There was no physical
resource/quality mutation.

## What this does NOT qualify

At max depth 32 the fitted ENG-1b engine's current hard
ceilings are still only 250000 contexts, 500000 follower
pairs and **500000** next-byte TRAIN events. The exact
complete counts therefore exceed fitted-model context
capacity by roughly **57x (FR)** and **56x (EN)**, and
follower-pair capacity by about **30x/29x**. Even at
depth 0 or 4, both complete TRAIN streams exceed the
frozen 500000 training-event maximum.

Every measured full scan reported the existing
MODEL_EVENT_GATE=STOP; depths >=7 additionally report
MODEL_NODE_GATE=STOP and MODEL_PAIR_GATE=STOP.
The external scanner is a *counting instrument*, not an
admitted distribution estimator or compact queryable
probabilistic cache.

It would be wrong to promote the 13–14 MiB observed scanner
VmHWM as a model-memory saving, to assume a source-bound
sorted-record representation supports low-latency 256-way
queries, or to train a depth-32 model without a separately
qualified new storage/runtime regime. The temporary
reconstruction scratch was removed after each process.

No heldout log-loss, calibration, accuracy, CTW, PPM,
ANF augmentation, physical ElasticXxx GPU/RSS policy or
SML-25B quality claim is derived from this study.

## Decision

**CAP2A_EXACT_FULL_TRAIN_CARDINALITIES=QUALIFIED**;
**CURRENT_ENG1B_FULL_CORPUS_MODEL=CAPACITY_STOP**.

Next CAP2b must preregister a genuine bounded-memory,
queryable Witten–Bell count-store architecture on
complete TRAIN with source identity, hard transient
resource budgets, deterministic node/follower compression
or paged external counts, and equality to the original
ENG-1b reference on smaller frozen fixtures.
Only once fitted storage is admissible can a
separate ENG-1c quality PR choose candidate depth
via VAL log-loss and score protected TEST exactly once.

Science issues:
- https://github.com/Memorithm/SML-GENIUS/issues/538
- https://github.com/Memorithm/SML-GENIUS/issues/530
- https://github.com/Memorithm/ElasticXxx/issues/244
