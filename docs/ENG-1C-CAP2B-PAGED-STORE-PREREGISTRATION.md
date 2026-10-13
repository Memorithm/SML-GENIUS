# ENG-1c/CAP2b/1 — exact paged Witten–Bell count store

**Frozen 2026-10-13 BEFORE implementation, tests, training or measurement.**
Source lineage: CAP2a PR #539, merge 6a0f7e20c980abb6a2f8c3530722cae33308e180; CAP2b issue #540.
This is an opt-in Rust research store, not SML-25B runtime promotion.

## Model semantics and source

The independent Gutenberg Dumas FR #17989 and Austen EN #1342 source/body
SHA256 hashes, LF normalization, TRAIN prefix, 64 independent contiguous
lanes and t>=1 denominators are unchanged from ENG-1c/0 and CAP2a.
Only TRAIN bytes enter fit or structural probing. No VAL/TEST scoring.

The store is an exact Witten–Bell 256-way estimator at a caller-declared
fixed depth 0..32. Its reference is unmodified ENG-1b:
start P(y)=1/256, for each present suffix depth in increasing order
let n=sum_y count(h,y), T=number of distinct observed followers,
then P(y)=P(y)*T/(n+T) + count(h,y)/(n+T), applying all 256 scale
operations before sorted ascending follower additions. Missing contexts
retain previous backoff. Count records and total accumulation are exact
checked nonnegative integers. Any depth change may change model
semantics and is not a lossless memory optimization.

## Physical representation, quotas and publication

- Input stream emits the exact 34-byte CAP2a record:
  depth u8, 32 ordered chronological suffix bytes left-aligned and
  zero-padded, next-byte label u8. Equality is full-byte equality,
  not hashing; no shortcut for repeated or all-zero contexts.
- Sorted training runs hold up to 262144 records each, at most 96 runs,
  aggregate scratch <=1073741824 bytes. One bounded k-way merge
  aggregates identical 34-byte records into a persistent ordered
  38-byte entry: 34-byte key plus little-endian checked u32 count.
  The resulting file is queryable and immutable.
- A sparse index stores one exact first-key plus u64 row offset for
  every 256 counted entries; sorted lookup reads only bounded
  pages and can scan over a page boundary for a context with
  several followers. No full in-RAM index of millions of contexts.
  Sparse index hard bound <=100000 entries.
- New model research ceilings, distinct from and WITHOUT modifying
  ENG-1b: TRAIN events <=650000; distinct contexts <=16000000;
  follower pairs <=16000000; counted store bytes <=805306368
  (768 MiB); total sorted scratch + final counted file + sparse
  index <=2147483648 (2 GiB); max 262144 sort records,
  96 runs, 32768-byte read buffers per run. If a quota is
  exceeded, return a typed STOP, never publish a partial model
  or silently downsample or lower depth.
- The hosted action enforces 262144 KiB virtual address space
  with ulimit -v and a 900s timeout per candidate. These are
  virtual-address and wall ceilings, NOT evidence of peak RSS.
  Process VmHWM, temporary sort bytes, published counted bytes
  and sparse index bytes must be measured separately.
- Source identity is the SHA256 of exact TRAIN bytes calculated
  by the Rust builder, included in a versioned manifest. Hosted
  corpus downloader independently verifies the previously pinned
  raw source and body SHA256. The store opener requires the
  caller-expected TRAIN SHA256 and rejects mismatches.
- Record files and sparse index receive SHA256 digests, exact
  lengths, cardinalities, schema/version and depth in an
  immutable manifest. The opener independently hashes both
  files, validates index ordering/row offsets and refuses
  truncation or corruption before serving probabilities.
- Atomic publication is in one local filesystem: create-new
  destination lock and private staging directory; write,
  flush and fsync contents and directory, validate staged
  store, then rename staging directory to the absent target
  and fsync parent. On in-process build failure/staged corrupt
  data/STOP, cleanup staging and leave any previous complete
  destination unchanged. A stale staging directory after crash
  is a fail-closed restart condition; unattended crash recovery,
  concurrent hostile writers and remote filesystem guarantees
  are NOT established in this slice.
- The published store is read-only, and exposed scoring methods
  operate on a single mutable file cursor in an opt-in owner.
  Multiple simultaneous readers, mmap/GPU, hard page-cache RSS,
  crash-durable transaction across multiple hosts, and
  filesystem adversary resistance are unqualified.

## Frozen experiments and negative controls

1. Unit fixtures: 64 lanes x40 patterned bytes plus a separate
   deterministic 64-lane independent probe, fixed depths
   [0,4,7,12,32]. Compare EVERY 256 f64::to_bits()
   class probability on all probe contexts against both
   immutable ENG-1b and ENG-1a, exact bpb and argmax score
   at the same depth. Validate positive finite normalization
   and immutable reopen/cold lookup.
2. Two 12-byte histories with equal newest 7 bytes, distinct
   oldest byte and next labels 200/201: depth-7 identical
   and depth-12 separation >0.9 after store reopen.
3. Force two or more sorted runs using a small test run length;
   exact count and predictions equal a large-single-run store.
   Root context including byte label 0 and 255, all-zero
   padding, equal suffixes with different depth, lane reset.
4. Fail closed for invalid query depth, missing source, altered
   expected SHA, wrong train length, corrupted data, truncated
   index, modified manifest, oversized event count, invalid
   caps, disk quota, destination already published, deliberate
   failure before rename. No partial target published. A
   previously published target is preserved on a failed
   attempt to republish.
5. Full real-book physical qualification on separate GitHub
   hosted FR and EN jobs at frozen depths [4,7,12,32] using
   exact TRAIN only. **Do not** score VAL/TEST. Expected
   source event counts FR 598208 and EN 590272.
   At each depth, expected distinct contexts/pairs and
   record-stream sizes are the previously published CAP2a
   values. Any cardinality mismatch is CI failure, not a
   model-quality negative. Sample a fixed 64 synthetic
   in-TRAIN histories only for 256-way positive finite
   normalization and equality to a reopened store; no
   heldout loss, calibration, accuracy or adaptive profile.
6. A structurally valid full-model build is considered a
   model STORAGE pass, not a quality pass or automatic
   low-latency inference readiness. Separately capture
   p50/p95 on a fixed 256-query TRAIN-only probe after
   64 warmups; do not change model parameters on the basis
   of this measurement. Slow queries are a valid negative
   performance observation and block production promotion
   without causing incorrect science.

## Release and scientific interpretation

One preregistration commit must precede all model code. Dedicated
hosted Rust 1.89 fmt/check/clippy -D warnings and tests on exact
PR head, plus general repository CI. Native Thor only after
a separately validated trusted-main workflow (not included here).
No NVIDIA proprietary software. No change to existing ENG-0,
ENG-1a/b, CAP1/CAP2a or A6-E4/E5 frozen code.

Real heldout quality requires a distinct future preregistration,
then fixed-depth selection on VAL and one protected TEST evaluation
without reusing its contents in fitting or admission. The new
resource quotas authorize only this research store; old model
ceilings remain in force. No ElasticXxx generic physical actuation
or SML-25B default route is authorized.
