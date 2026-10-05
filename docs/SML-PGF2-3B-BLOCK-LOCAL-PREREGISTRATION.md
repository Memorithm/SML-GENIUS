# PGF2-3B — K=2048 block-local locality-rescue preregistration

Status: preregistered follow-up candidate. No result is recorded in this document.

PGF2-3 retained a negative result for the **global** K=2048 Pascal-GF(2)
representation transform. PGF2-3B asks one narrower question: does restricting
the same qualified transform to independent fixed-size blocks recover enough
SBG Hamming geometry and noise robustness to justify any further
representation-level work?

This is a distinct candidate. It does not reopen PGF2-3, authorize PGF2-4, or
change any MOR, Delta-KV, context or model contract.

## Frozen total width and arms

The total logical state width remains **K=2048 bits**.

Matched reference arms:

1. **IDENTITY** — unchanged field.
2. **PERMUTE** — the same fixed reversible rotation from PGF2-3, shift 683.
3. **XOR_PERMUTE** — the same existing qualified `semantic::bind` control and
   the same fixed role field used by PGF2-3.
4. **PGF2_GLOBAL** — the retained global K=2048 PGF2 transform from PGF2-3,
   included as the failed candidate reference.

New block-local arms:

5. **PGF2_B64**
6. **PGF2_B128**
7. **PGF2_B256**
8. **PGF2_B512**

For `PGF2_BN`, the 2048-bit state is partitioned into contiguous,
non-overlapping N-bit blocks in ascending coordinate order. The already
qualified `pascal_gf2_in_place` transform is applied independently to every
block. There is:

- no coordinate reordering;
- no overlap;
- no cross-block XOR;
- no learned block selection;
- no data-dependent block size;
- no post-hoc choice of orientation.

Applying the same block-local transform twice must recover the original K=2048
state exactly.

## Frozen fixtures

PGF2-3B reuses the PGF2-3 fixtures **without modification**:

- `STRUCTURED_LOCAL`: 32 classes and 8 routes;
- `BALANCED_RANDOM`: 32 classes and 8 routes;
- the exact xorshift64 generator, constants and seed derivation;
- uniform input-noise levels `h={0,1,4,16,64}`, seeds `0..15`;
- symmetric class-boundary fractions `{1/4,1/2,3/4}`;
- ties count as failures;
- the complete 2048-coordinate single-bit stability panel.

The authoritative fixture mechanics are
`SML-PGF2-3-SBG-ABLATION-PREREGISTRATION.md`. PGF2-3B must consume those
mechanics directly or reproduce them byte-for-byte; it may not tune fixtures to
a block size.

## Frozen structural accounting

For block width `B = 2^m`:

```text
logical_xor_ops = (2048 / 2) * m
packed_word_updates = 192 + 16 * (m - 6)
state_bytes = 256
scratch_words = 0
```

Therefore the required exact values are:

| arm | stages/block | logical XOR updates | packed-word updates |
|---|---:|---:|---:|
| PGF2_B64 | 6 | 6,144 | 192 |
| PGF2_B128 | 7 | 7,168 | 208 |
| PGF2_B256 | 8 | 8,192 | 224 |
| PGF2_B512 | 9 | 9,216 | 240 |
| PGF2_GLOBAL | 11 | 11,264 | 272 |

The implementation may iterate over slices but must not allocate an
algorithmic scratch state beyond the caller-provided/output 256-byte field.

## Frozen measurements

For every arm and both fixture families, report the same PGF2-3 metrics:

1. full-state class and route collision excess;
2. low-12 class-address collision excess;
3. nearest-wrong class separation: minimum, median, exact mean;
4. nearest-wrong route separation: minimum, median, exact mean;
5. unique nearest-class retrieval for every uniform-noise level;
6. unique nearest-route retrieval for every uniform-noise level;
7. pair-boundary unique class retrieval at 1/4, 1/2 and 3/4;
8. complete one-bit amplification distribution:
   minimum, median, exact mean, p95, maximum;
9. exact structural cost.

No wall-clock latency is measured in PGF2-3B.

## Frozen hypotheses

All comparison thresholds are evaluated against the contemporaneous matched
controls in this run, not copied from a hand-entered expected result.

### H1 — exactness

For every block-local arm:

- all inverse/self-inverse checks pass;
- full-state collision excess does not exceed the best matched control.

### H2 — structured geometry rescue

At least one block-local width must satisfy **both**:

```text
candidate class median * 10 >= best-control class median * 9
candidate route median * 10 >= best-control route median * 9
```

on STRUCTURED_LOCAL.

This allows at most 10% median separation loss relative to the reversible
Hamming-isometric controls.

### H3 — low-noise parity with controls

The **same** candidate width selected by H2 must be within **10,000 ppm** of the
best matched control for all four STRUCTURED_LOCAL cells:

- class retrieval at h=1;
- class retrieval at h=4;
- route accuracy at h=1;
- route accuracy at h=4.

No cell may exceed the control and thereby compensate for a different cell.

### H4 — moderate-noise viability

The same candidate must be within **50,000 ppm** of the best matched control at
h=16 for both:

- class retrieval;
- route accuracy.

### H5 — material improvement over failed global PGF2

At h=64 on STRUCTURED_LOCAL, the same candidate must improve over
`PGF2_GLOBAL` by at least **100,000 ppm** for both class retrieval and route
accuracy.

This gate measures whether locality actually addresses the failure mechanism;
it is not sufficient for selection by itself.

### H6 — random-control sanity

On BALANCED_RANDOM:

- h=0 class retrieval and route accuracy must equal the best matched control;
- clean class and route median nearest-wrong separation may differ from the best
  matched control by at most **20,000 ppm of K=2048**.

A larger clean random movement is treated as a fixture/implementation anomaly,
not evidence.

### H7 — bounded single-bit amplification

The same selected block-local candidate must satisfy:

- mean single-bit amplification <= **32 output bits**;
- p95 single-bit amplification <= **64 output bits**;
- maximum amplification <= its declared block width.

The complete distribution is still reported.

### H8 — structural-cost reduction

The candidate must use:

- fewer than 11,264 logical XOR updates;
- fewer than 272 packed-word updates;
- exactly 256 state bytes;
- zero algorithmic scratch words.

## Selection and stop rule

A block-local width is **RETAINED_FOR_COMPONENT_RESEARCH** only if one and the
same width satisfies H2, H3, H4, H5, H7 and H8, while H1 and H6 also hold.

If multiple widths pass, select the smallest block width; this deterministic
tie-break is frozen before execution.

If no width passes:

```text
PGF2 representation mixing = CLOSE
PGF2-4 MOR/Delta-KV handoff = BLOCKED
```

The negative global PGF2-3 result remains authoritative regardless of the
PGF2-3B outcome.

A failure here ends the current Pascal-as-SBG-representation-mixer branch.
Future Pascal research would require a different declared objective, such as a
task explicitly aligned with subset/parity semantics, rather than another
post-hoc representation orientation.

## Reproducibility

The execution must:

- be Rust-only;
- reuse the merged PGF2 core;
- print deterministic integer/hex evidence;
- execute twice in CI with byte-identical stdout;
- retain raw replay, manifest and observation;
- leave MOR, Delta-KV, context and model promotion blocked.
