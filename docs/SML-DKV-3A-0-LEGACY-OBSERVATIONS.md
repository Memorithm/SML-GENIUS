# DKV-3a-0 uploaded observations: historical evidence boundary

Source: the two owner-supplied `dkv-3a-0.patch` files identified by SHA-256 in
[SML-DKV-3A-0-HARDENING.md](SML-DKV-3A-0-HARDENING.md). These are source-reported
results from the original harness, not results of the corrected conformance
fixtures. Original declarations of preregistration/merge timing are not independently
established by a patch file. No previous protected evaluation is reopened.

## Original nine-fixture panel (both uploads)

| arm | correct incl. correct abstentions | wrong accepted | abstained | raw segments |
| --- | ---: | ---: | ---: | ---: |
| context_no_dkv | 9 | 0 | 3 | 9 |
| context_direct_memory | 8 | 1 | 2 | 4 |
| context_dkv_frozen_unverified | 3 | 3 | 6 | 0 |
| context_dkv_frozen_verified | 6 | 3 | 3 | 6 |

Condition 4 failed. The direct control retained only the latest record and lost a
contradiction. Verified answers used fixture FACTS bits, not an independent text
resolver. The three wrong accepted DKV answers were stable/paraphrase/distractor
fixtures using derived-only answers. The label `false_confident` meant wrong
acceptance; it did not measure calibrated confidence. Exact fields were checked
with substring matching. Named paraphrase/orthogonal controls did not establish
language paraphrase capability or actual orthogonality.

## Second upload's capacity table (verified arm, correct out of nine)

| dimension | seed 0 | seed 1 | seed 2 | matrix logical bits only |
| ---: | ---: | ---: | ---: | ---: |
| 8 | 6 | 8 | 8 | 448 |
| 16 | 7 | 9 | 7 | 704 |
| 32 | 9 | 8 | 8 | 1216 |
| 64 | 9 | 8 | 8 | 2240 |
| 128 | 9 | 8 | 9 | 4288 |

No tested dimension matched the 9/9 control on all three seeds. The earlier
post-hoc dimension-16/32 diagnostic from the first upload was retracted in the
second upload because fixed coordinate flips changed overlap with dimension.
Block flips repair the coordinate FRACTION, not the exact realized cosine of
variable-magnitude keys. Exact cosine is `1 - 2*sum(flipped k_i^2)/sum(all k_i^2)`.
The source's claim that seed 2 fails at dimension 128 contradicts its own 9/9
cell; retain the table, not that sentence. These are the same nine fixtures,
not fifteen independent task panels. Width is not an association-count theorem.

## Strict policy experiment (second upload)

| arm | correct | wrong accepted | raw segments / bytes | aggregate prediction MACs |
| --- | --- | --- | --- | --- |
| context_no_dkv | 9/9 | 0 | 9 / 247 | 0 |
| context_dkv_frozen_verified | 6..9 / 9 | 0..3 | 6 / 155 | 368..5888 |
| context_dkv_frozen_strict | 9/9 | 0 | 9 / 247 | 368..5888 |

The strict result is largely by construction: all accepted answers come from
sidecar lookup plus raw resolution; no accepted answer uses the numeric read.
It establishes no benefit of the matrix in this slice and is not evidence about
its future role in SBG recurrence. `9/9` includes three correct abstentions.
The aggregate numeric `read_macs` includes 14 write-time predictions plus nine
reads: `(14+9)*dimension*2`; query-only MACs are `9*dimension*2`. Sidecar/index
storage, allocator overhead, copies, physical I/O and latency were not counted
by the matrix-bit column.

## Original overlap/recency probe (second upload)

| blocks flipped | D32 target last | D32 target first | D128 target last | D128 target first |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 8/8 | 0/8 | 8/8 | 0/8 |
| 2 | 8/8 | 0/8 | 8/8 | 0/8 |
| 3 | 8/8 | 0/8 | 8/8 | 0/8 |
| 4 | 8/8 | 8/8 | 8/8 | 8/8 |

The original four-block formula `(i + 2*j) % 8` produces only TWO distinct
antipodal distractor keys, not five. Do not attribute the jump solely to overlap.
The test measures two output signs; it does not establish exact Q12 recovery.
Random magnitudes also prevent exact fixed-cosine comparisons. These observations
remain informative counterexamples but are not a controlled general scaling law.

## Disposition

Preserve negative observations; do not import the unchecked public proof enum,
FNV-only exact membership, unbounded sidecar or oracle-fed resolver. The corrected
adapter and engineering tests are separate. Original four-arm scientific
comparisons, controlled resource/quality measurements and projected SBG/MOR
recurrence remain unqualified until separately preregistered and executed.
