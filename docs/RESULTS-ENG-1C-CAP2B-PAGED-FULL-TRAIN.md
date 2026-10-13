# ENG-1c/CAP2b/1 — exact, queryable paged Witten–Bell stores on full TRAIN

**Observed 2026-10-13 UTC.** This report records measurements from the
first successful job only, after pre-registration
`26eaf0a8cd83ee7610c21e87732ffec615a41276` and implemented source
HEAD `78af17b57149031d6ba478fe5588f99db39d408a` were committed.
It does not retroactively change any quota, normalization or quality gate.

## Provenance and test evidence

- SML-GENIUS PR #541; standalone, opt-in model module
  `crates/sml-core/src/elastic_ngram_paged.rs`, independent pure Rust
  `elastic_ngram_paged_sha.rs` and CLI `sml-eng1c-cap2b-paged`.
- First complete GitHub-hosted Ubuntu 24.04 / Rust 1.89 run:
  [#38361124262](https://github.com/Memorithm/SML-GENIUS/actions/runs/38361124262),
  FR job `115175309994` **success** and EN job `115175309759`
  **success**. The Rust fmt/check/Clippy with -D warnings,
  standard SHA256 vectors, exact small-fixture 256-way probability
  and score oracles against ENG-1a/ENG-1b, same-depth 12-byte
  distinct-context witness, source/probe/manifest corruption,
  quota and atomic-publication failure tests all passed.
- Exact raw diagnostic archives:
  [FR artifact #11795421170](https://github.com/Memorithm/SML-GENIUS/actions/runs/38361124262/artifacts/11795421170)
  (4,445 GitHub-reported compressed bytes) and
  [EN artifact #11794413930](https://github.com/Memorithm/SML-GENIUS/actions/runs/38361124262/artifacts/11794413930)
  (4,432 compressed bytes). GitHub artifact bytes have not
  been independently downloaded for archive SHA256 rehashing.
- The complete Gutenberg FR #17989 and EN #1342 raw/body source SHA256
  from [ENG-1c frozen corpora](ENG-1C-FROZEN-CORPORA.md)
  were independently verified by the public source shell before
  selecting and passing only TRAIN byte prefixes.
- No VAL or TEST bytes were passed into a model; no natural-language
  log-loss, calibration, accuracy or model-depth selection occurred.
- Each of four predeclared depths [4,7,12,32] was built as a
  private local on-disk counted store and then cold-reopened
  from an independent process. Every source/depth pair was
  bitwise probe-checksum identical after reopening.
- All observed exact total contexts and follower pairs match
  the independent pre-frozen CAP2a count-only results.

## Full-TRAIN model construction and fixed TRAIN-derived lookup measurements

All counts and sizes below are **exact reported values**.
p95 is the latency of 256 full 256-class TRAIN-history consultations
after 64 warmups in the hosted release binary, **not** throughput
or out-of-sample log-loss. Linux VmHWM is peak RSS of the
entire process during build; the cold reopen VmHWM excludes the
build-time sorted buffer and is a separate process.

| Corpus | Depth | Full exact contexts | Full distinct follower pairs | Counted store bytes | Sparse index bytes | Temp sorted records bytes | Build-process VmHWM KiB | Query p95 ns | Cold reopen VmHWM KiB |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| FR | 4 | 40329 | 117282 | 4456716 | 19278 | 101682304 | 12136 | 586005 | 3148 |
| FR | 7 | 491517 | 799782 | 30391716 | 131250 | 162666880 | 12336 | 883343 | 3180 |
| FR | 12 | 2597815 | 3131101 | 118981838 | 513702 | 264264320 | 12680 | 1355393 | 3676 |
| FR | 32 | 14281533 | 14878280 | 565374640 | 2440998 | 670110080 | 15544 | 3090109 | 5700 |
| EN | 4 | 41802 | 118117 | 4488446 | 19404 | 100333184 | 12084 | 583900 | 3108 |
| EN | 7 | 471036 | 759560 | 28863280 | 124656 | 160508288 | 12328 | 894971 | 3164 |
| EN | 12 | 2477266 | 2995126 | 113814788 | 491400 | 260756608 | 12780 | 1373633 | 3632 |
| EN | 32 | 13932625 | 14519708 | 551748904 | 2382156 | 661205888 | 15496 | 3125691 | 5712 |

At depth32:
- **FR:** 14,281,533 contexts, 14,878,280 distinct follower
  pairs, persisted counted file **565,374,640 bytes**, sparse index
  **2,440,998 bytes**. The exact TRAIN contains 598,208 next-byte
  target events; emitted CAP2a-style raw observations total
  19,709,120. Build-process peak RSS **15,544 KiB**; fixed
  TRAIN-derived query p95 **3,090,109 ns**.
- **EN:** 13,932,625 contexts, 14,519,708 follower pairs,
  persistent counted file **551,748,904 bytes**, sparse index
  **2,382,156 bytes**. Frozen TRAIN target events 590,272;
  raw observations 19,447,232. Build-process peak RSS
  **15,496 KiB**; fixed query p95 **3,125,691 ns**.

The 256-MiB operating bound was an OS-enforced **virtual address-space**
limit (`ulimit -v 262144`), not a 256-MiB RSS or page-cache cap.
Build-time sorted scratch separately respected 1 GiB, and
the simultaneous sorted+counted+sparse upper bound was 2 GiB.
The observed build peak RSS does NOT include the complete
file cache, persistent disk, a live multi-reader inference
service or GPU memory. The disk footprint far exceeds the
physical byte budget of the small ENG-1b model and must be
admitted separately in a real deployment.

## Source and identical cold-open evidence

The 64-hex `train_sha256` below is SHA256 of the exact TRAIN prefix
measured in pure Rust. It is not the SHA256 of the original entire
Gutenberg file or normalized complete book.

| Corpus | Depth | TRAIN SHA256 | Build-probe checksum | Reopen-probe checksum |
|---|---:|---|---|---|
| FR | 4 | ec538157302ad0d39668288c8109779b70492a93041d432f7f97b2dc96e535cd | 63a68b86256c7851 | 63a68b86256c7851 |
| FR | 7 | ec538157302ad0d39668288c8109779b70492a93041d432f7f97b2dc96e535cd | 48190df4afb8bb16 | 48190df4afb8bb16 |
| FR | 12 | ec538157302ad0d39668288c8109779b70492a93041d432f7f97b2dc96e535cd | 7efee0ea81442ef0 | 7efee0ea81442ef0 |
| FR | 32 | ec538157302ad0d39668288c8109779b70492a93041d432f7f97b2dc96e535cd | 7298be696b3ec59b | 7298be696b3ec59b |
| EN | 4 | 43553238be6ec4a88e01a679a53381cd89aca8374d815e021e92317f71cb47d0 | 4d36f727187c3d44 | 4d36f727187c3d44 |
| EN | 7 | 43553238be6ec4a88e01a679a53381cd89aca8374d815e021e92317f71cb47d0 | 89483cb812d72195 | 89483cb812d72195 |
| EN | 12 | 43553238be6ec4a88e01a679a53381cd89aca8374d815e021e92317f71cb47d0 | 8fbf711d0ae564b9 | 8fbf711d0ae564b9 |
| EN | 32 | 43553238be6ec4a88e01a679a53381cd89aca8374d815e021e92317f71cb47d0 | c944ee637e09eedf | c944ee637e09eedf |

These probe digests are noncryptographic rolling witnesses over
256 predicted 256-class distributions, not universal proofs for
all possible 32-byte histories. The cryptographic counted-file,
sparse-file and source SHA256 checks are enforced on model opening;
published data are immutable and the source identity is supplied
by the trusted model caller.

## Scientific verdict and limitations

**CAP2B_1_PAGED_FULL_TRAIN_MODEL_BUILD=PASS** for all eight
declared corpus/depth configurations on the first hosted run,
including stage publication and independent reopen.
**CAP2B_1_PROBABILITY_PARITY=PASS** on the frozen synthetic
small-fit oracles at all depths [0,4,7,12,32].
**MODEL_QUALITY_AND_SML_25B_PROMOTION=NOT_EVALUATED**.

This is a genuine, persistent, queryable *count store*, unlike
CAP2a's pure cardinality scanner. It is not a high-performance
production inference engine yet: at depth32, one 256-way lookup
on these fixed in-TRAIN histories required p95 ~3 ms on this
hosted CPU, with cold file I/O and page cache potentially changing
under workload and concurrency. A page-index lookup is slower
than a fully resident hash/tree for sufficiently hot queries,
and its disk size, page-cache sensitivity and physical IO
require further independent measurements.

The atomic replacement claim is strictly a single-filesystem
staged directory rename with checksums and cleanup. Crash-durable
multi-process transactions, hostile filesystem writers,
concurrent lock-free readers, persistent host-level memory
governance, SSD wear and actual ElasticXxx ACT/ROLLBACK are
**not** established. Existing ENG-1b event/node/pair caps and
default SML inference remain unchanged; new independently
bounded CAP2b research quotas are not retroactive relaxations.

A later preregistered science experiment may measure depth
selection on VAL log-loss, with a single frozen TEST run only
AFTER admitting model quality and independent resource budgets.
CTW/PPM are distinct estimators and remain separate.
Tracking: [ENG-1c parent #530](https://github.com/Memorithm/SML-GENIUS/issues/530),
[CAP2b #540](https://github.com/Memorithm/SML-GENIUS/issues/540),
[ElasticXxx #244](https://github.com/Memorithm/ElasticXxx/issues/244).
