# ENG-1c/CAP2a — Exact full-TRAIN suffix cardinality with bounded external sorting

**Preregistered 2026-10-13 BEFORE any CAP2a code, tests, source downloads
for this study, or cardinality measurement.** This CAP2a study follows the
negative CAP1 finding (PR #537) and tracked CAP2 issue #538.

## Question and ownership

Can an exact, collision-free representation count ALL distinct depth-tagged
byte suffixes and (suffix, next-byte) followers for the full immutable TRAIN
sets of two already frozen real-language corpora, without keeping the millions
of keys in an in-memory BTreeMap/Trie?

This is a **TRAIN-only cardinality and physical resource diagnostic**,
not a Witten–Bell learner, CTW/PPM estimator, log-loss benchmark,
automated depth chooser, a source for learned model checkpoints, or
an ElasticXxx physical actuation qualification. Do not access VAL or TEST
bytes in the indexer; no predictive metrics are generated.

The two Gutenberg sources and normalization are immutable under ENG-1c/0:
- FR #17989 (Dumas/Maquet), raw SHA256
  9ea7a8b127a9dfa36de0b0cf9c2e26293beea5c37a0fc988fb9308b63160f282
  and body SHA256
  24ebc9a0037fc57bf93bf3ac6788c5ca64a9a6bbaf92b9fdbdfe6ca0f4114dde;
  body 747855 bytes; TRAIN prefix 598284 bytes; 598208 byte targets.
- EN #1342 (Austen), raw SHA256
  3f6bb9d6f78e0293b56acd4714dd68cb7d6d1d293402031ce9d5a216bcaf9d75
  and body SHA256
  e79303d04de9260489f9d5a2ad4aae5776680fcec484f2e732d6ded7411bf402;
  body 737950 bytes; TRAIN prefix 590360 bytes; 590272 byte targets.

Use unchanged shell helper scripts/eng1c/fetch-cap1-train.sh to verify
raw source AND normalized LF-body SHA256 and write only the TRAIN prefix
to private runner temp; no book text in CI artifacts. Failure to obtain
the byte-exact public source is **PROVENANCE_STOP** and forbids inference.

## Frozen cardinality estimator and exact key

- Eight declared query depths **[0, 2, 4, 7, 12, 16, 24, 32]**;
  one separate binary process per corpus/depth: **16 profiles**.
- Each TRAIN split is partitioned into exactly 64 contiguous lanes;
  L=floor(TRAIN_bytes/64), the per-lane suffix context resets at
  boundaries; for each t=1..L-1, record y=chunk[t] and each
  suffix length d=0..min(t,max_depth).
- Encode every emitted observation as an **exact fixed-width 34-byte
  binary record**: 1 byte depth d, 32 suffix bytes in chronological
  order left-aligned and deterministically zero padded, followed by
  1 byte next-byte label. The full 34-byte equality is EXACT:
  no hash truncation, approximation, missing context-depth tag
  or collision between distinct suffixes. For d=0 the key is empty;
  counts include one root context. A context identity is the
  first 33 bytes; a follower identity is all 34 bytes.
- Sort **262144** records maximum per run with in-memory
  sort_unstable on the complete 34-byte key, then write fixed-width
  binary sorted runs in deterministic run order. Each run is
  independently checked for byte length, monotonic records and
  rolling noncryptographic ordered checksum when reopened.
- Merge a maximum of **96** run streams with a bounded BinaryHeap
  and **32768-byte per-run buffered I/O**, with no full merged file.
  During the final single-pass merged stream count adjacent
  distinct full 33-byte contexts and full 34-byte context/follower
  pairs. Verify global monotonicity and EXACT emitted row count
  equals the sum over all emitted TRAIN observations. No eviction,
  sampling, truncated run, inexact tie or post-hoc depth selection.
- Two independent wrapping, commutative 64-bit multiset checksum
  accumulators over every 34-byte record **before sort** and again
  from the fully merged stream must match, together with every
  per-run deterministic ordered checksum and record count. These
  hashes detect accidental corruptions but **are not cryptographic
  proofs**; source SHA256 remains the authenticated public corpus
  identity. The raw/normalized source bytes never appear in artifacts.
- The sorted run scanner has a hard file-byte budget, run count
  and memory/corpus bound. It stores ONLY sorted runs, never a
  materialized Witten–Bell model or full distinct suffix trie.

## Resource contract, frozen before measurements

- Record buffer maximum **262144 × 34 = 8912896 bytes**,
  at most **96** simultaneous sorted runs, each with 32768 bytes
  read buffer (3 MiB total read buffers maximum).
- Maximum aggregate sorted-run scratch files **1 GiB**
  (1073741824 bytes), checked before writing each run.
  No intermediate merged run file or 2x disk copy.
  Create a **new, private work directory** per process and
  delete only that directory on all return paths.
- **256 MiB virtual address-space limit** (\`ulimit -v 262144\`)
  applied to each standalone release binary by hosted Linux
  shell. This is an OS-enforced address-space ceiling, **not**
  evidence that 256 MiB of process RSS were actually used.
  Record independently measured Linux \`VmHWM\` peak KiB;
  it includes process and allocator overhead.
- Up to **650000** structural TRAIN target events (no model
  event cap increase); reject unsupported depth, invalid corpus,
  >650000 event source and broken lane boundaries.
  The existing fitted-model hard maximum remains 500000 events
  and 250000 stored context nodes. CAP2a does NOT modify them.
- Bound each depth process with a **900-second wall timeout** and
  each two-language job with an explicit 60-minute workflow timeout.
  All 16 profiles must complete or emit a **quota/timeout STOP**;
  no incomplete prefix ever masquerades as whole-book distinct
  cardinality. An unexpected checksum mismatch, truncated run,
  invalid source identity or algorithm/oracle divergence is an
  engineering CI failure, not a scientific STOP.
- A deliberate too-small scratch quota must yield typed
  DISK_QUOTA_STOP with **no** completed-context cardinality
  claim. If a run count cap or virtual memory ceiling is
  exceeded, return explicit RUN_QUOTA_STOP/MEMORY_STOP and
  preserve the negative verdict, not switch to a bigger
  limit or silently reduce depth.
- One fresh hosted Ubuntu 24.04 runner per language, Rust 1.89
  release binary, CPU only. No self-hosted/Thor runner executes
  untrusted PR code. No NVIDIA SDK.

## Frozen independent oracle / tampering tests

1. A deterministic 64×40 TRAIN fixture for each declared depth,
   where new CAP2a cardinality equals the existing **unmodified**
   ElasticNGramTrie exact node/follower accounting.
2. A two-teacher 12-byte fixture sharing its newest seven bytes
   but differing on an older byte must remain distinct at
   depth 12. Zero-padding must not alias different depth/byte
   sequences, including all-zero suffixes.
3. Compare the external-run mode with a direct in-RAM oracle
   over small synthetic data with deliberately tiny sorting
   runs to force >=2 files and k-way merge; counts exactly
   equal at every depth. Confirm the full TRAIN event
   denominator and emitted total observation records.
4. Fail closed on a run with tampered bytes, wrong length,
   unsorted records, checksum mismatch, duplicate/collision
   mislabeling, invalid depth, too-short input, scratch quota
   under the first run and excessive run count. Quota STOP is
   a scientific capacity outcome; tampering/integrity
   discrepancies are hard error.
5. ENG-1c/CAP1, ENG-1b, EL-NG1 and ENG-2a/b frozen code
   and prior research protocols remain unchanged. No new
   predictive-model selection route or physical actuator.

## Output, comparison and negative semantics

A complete report per language/depth MUST include:
- exact source SHA-identifying book label, depth, TRAIN bytes,
  64-lane event denominator, total emitted 34-byte records,
  unique complete contexts, unique distinct followers,
  sorted run count and peak scratch file bytes;
- virtual address-space cap, Linux VmHWM process peak KiB,
  multiset checksums and status **FULL_SCAN_COMPLETE**;
- separate **MODEL_EVENT_GATE=STOP** because existing
  trained-model 500000-event maximum is exceeded, plus
  **MODEL_NODE_GATE=STOP** if exact contexts >250000,
  **MODEL_PAIR_GATE=STOP** if exact follower pairs >500000.
  Full capacity scan does not imply admissible model fit.
- A quota/timeout STOP reports only safely completed
  prefix/generated-run metadata, with unique context/pair
  cardinalities marked unknown and no extrapolation.

Validation: exact context counts at depths 0/2/4 must agree
with already published CAP1 full-TRAIN counts:
- FR d0=1/94; d2=1368/9480; d4=40329/117282.
- EN d0=1/96; d2=1529/10305; d4=41802/118117.

These are independent previous real-data oracle facts **frozen
before implementation**. Any discrepancy is a hard error.

No corpus-size, depth, split, memory, time or success criterion
may be revised after observing CAP2a outcomes without a
**new dated preregistration before a separate run**.
A future CAP2b must design an actually buildable, memory-budgeted
Witten–Bell learner and representation before heldout-quality
tests. CTW/PPM and adaptive-depth changes are separate studies.

## Archiving, qualification and promotion boundary

The PR has a dedicated hosted Rust 1.89 workflow:
fmt --check, check --locked, strict Clippy warnings-as-errors,
all CAP2a unit tests, original ENG-1c CAP1/ENG-1b regression
tests, and 16 full frozen real-TRAIN scans under the limits above.
Evidence artifacts contain **only small CSV/text manifests,
never Gutenberg bytes or suffix run files**. General repository
CI is required GREEN at the same final PR SHA before merge.
A second final-head run is compared against the preregistered
first-run structural rows (physical timings/RSS may differ).

A successful external scan is NOT a fitted Witten–Bell
model, not a quality gain, not a production/GPU/RSS optimizer,
not proof of Tensor-Core compatibility and not an admission
for SML-25B runtime or ElasticXxx physical actuation.
