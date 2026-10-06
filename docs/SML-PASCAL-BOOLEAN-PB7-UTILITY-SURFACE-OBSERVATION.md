# PB-7 — Pascal utility-surface generalization observation

Status: retained positive utility-generalization evidence under
`SML-PASCAL-BOOLEAN-PB7-UTILITY-SURFACE-PREREGISTRATION.md`.

PB-7 follows PB-6 and asks where the exact packed Pascal/subset-zeta advantage
survives changes in width, query load and address popcount distribution. It does
not reopen the closed PGF2 representation-mixing branch and does not qualify a
model architecture.

## Execution identity

- protocol PR: #219;
- protocol merge: `4d91b2dd9571052ec0bfc1174a7797611058df07`;
- implementation PR: #220;
- executed head: `350f64e00a6210f6efb4492ecb766503999c714a`;
- implementation merge: `de1dc6265c0cc23e25486f9691e2730d325373e1`;
- GitHub Actions run: `37451150142`;
- job: `112227757121`;
- runner: `tarek-scirust-arm64-01`, machine `tarek`;
- Rust 1.89.0, target `aarch64-unknown-linux-gnu`;
- deterministic replay:
  `research/pascal-boolean-pb7/DETERMINISTIC-REPLAY.csv`;
- deterministic replay SHA-256:
  `3bc01389cc6a21f839726551b365f6a2628ccaacc5b3d3dc31349455a94cc98d`;
- physical replay:
  `research/pascal-boolean-pb7/PHYSICAL-REPLAY.csv`;
- physical replay SHA-256:
  `dde1a3b3e6d7e4f3239d26c8a9215118fd12514c41cc3ebd4e4943dddd5b1cf0`.

CI executed deterministic mode twice under release and required byte-identical
stdout before running the physical panel.

## Deterministic result

G1 through G4 hold.

At every frozen width K=64..4096:

- all 64 deterministic source states are distinct;
- packed Pascal matches the independently written logical fast-zeta reference;
- packed Pascal matches the direct subset oracle;
- A/B/C answer mismatches are zero in every width/schedule/query cell;
- A and C retain exactly K/8 logical answer bytes;
- the full-domain structural advantage holds;
- LOW_POPCOUNT <= AFFINE_UNIFORM <= HIGH_POPCOUNT direct-read ordering holds.

The retained frozen-grid crossover points are:

| K | affine Q | low-popcount Q | high-popcount Q |
| ---: | ---: | ---: | ---: |
| 64 | 32 | 64 | 16 |
| 128 | 32 | 128 | 8 |
| 256 | 64 | 256 | 16 |
| 512 | 128 | 256 | 32 |
| 1024 | 256 | 512 | 16 |
| 2048 | 512 | 1024 | 32 |
| 4096 | 256 | 2048 | 64 |

These are structural crossover points under the frozen logical-work accounting,
not latency thresholds.

## Full-domain physical generalization

The P0 panel benchmarks AFFINE_UNIFORM at Q=K. Every row has exact checksum
agreement and 12/12 paired wins for packed Pascal.

| K | packed Pascal median | faster matched control median | speedup |
| ---: | ---: | ---: | ---: |
| 64 | 1.338 us | 4.495 us | 3.359x |
| 128 | 2.560 us | 13.791 us | 5.387x |
| 256 | 4.986 us | 42.764 us | 8.577x |
| 512 | 9.893 us | 134.966 us | 13.643x |
| 1024 | 20.032 us | 424.581 us | 21.195x |
| 2048 | 57.102 us | 1,290.144 us | 22.594x |
| 4096 | 133.712 us | 3,880.521 us | 29.021x |

The faster control changes from DIRECT_ON_DEMAND at K<=512 to
GENERIC_FULL_PRECOMPUTE at K>=1024. G6 requires only K>=512 and holds at every
required width. The small-width cells also happen to exceed the frozen 10%
threshold, but they were controls and are not retroactively added to G6.

## K=2048 query-population surface

The P1 panel exposes the expected workload dependence rather than a universal
win.

For LOW_POPCOUNT queries:

| Q | packed Pascal median | direct median | result |
| ---: | ---: | ---: | --- |
| 32 | 2.250 us | 0.782 us | direct is 2.877x faster |
| 128 | 4.018 us | 4.222 us | Pascal only 4.83% faster; 10% gate not met |
| 512 | 11.166 us | 34.342 us | Pascal 3.076x faster |
| 1024 | 20.648 us | 134.157 us | Pascal 6.497x faster |
| 2048 | 45.472 us | 1,235.626 us | Pascal 27.173x faster |

For HIGH_POPCOUNT, packed Pascal is already strongly favorable at Q=32:
2.222 us versus 171.087 us for the faster matched control, about 77.0x. At
Q=128 the corresponding ratio is about 102.8x. These very large ratios are
specific to expensive high-popcount direct subset enumeration and must not be
generalized to arbitrary workloads.

AFFINE_UNIFORM is also physically favorable at all P1 loads, including Q=32,
where packed Pascal records 2.269 us versus 21.217 us for direct on demand.
This occurs before the deterministic AFFINE_UNIFORM frozen-grid structural
crossover at Q=512.

The difference is important: the structural counter treats logical XOR/read
work as scalar operations, while the packed implementation performs Boolean
work word-parallel in u64. Structural crossover and measured latency crossover
are therefore distinct quantities. PB-7 does not convert logical operation
counts into a universal timing model.

## Gates and decision

- G1 exactness: HOLDS.
- G2 storage parity: HOLDS.
- G3 full-domain structural advantage: HOLDS.
- G4 address-popcount bounds: HOLDS.
- G5 independent PB-6 signal replication: HOLDS.
- G6 width generalization: HOLDS.
- G7 benchmark sanity: HOLDS.

The preregistered decision is:

`PB7_DECISION,RETAIN_GENERALIZED_UTILITY_SURFACE`

## Interpretation and boundary

PB-7 materially strengthens the Pascal case for exact subset-lattice
aggregation. The result is no longer one K=2048/full-query point: exactness
survives K=64..4096, full-domain physical advantage survives the entire width
ladder, and the K=2048 load/schedule panel identifies both winning and losing
regions.

The result also defines a practical boundary: precompute is not automatically
appropriate for sparse cheap-address workloads. At K=2048 LOW_POPCOUNT/Q=32,
direct evaluation is clearly preferable, and at Q=128 the measured Pascal
advantage is below the frozen 10% threshold.

Absolute PB-7 timings are not directly comparable with PB-6 absolute timings.
PB-6 timed the complete matched P-FEISTEL + ANF + subset-query pipeline over 64
states; PB-7 intentionally isolates the subset engine and uses 8 physical
states per cell.

PB-7 still does not establish:

- language-model quality or perplexity improvement;
- semantic retrieval benefit;
- learned routing benefit;
- SBG/MOR/Delta-KV benefit;
- trainability;
- universal optimality of Pascal or fast zeta;
- a universal speedup independent of workload.

Because the preregistered generalized-utility gate passed, the SML-side track
may now define a separate prospective PB-8 model-aligned task protocol. That
protocol must compare Pascal against competent task-native controls and must not
reuse PB-7 outcomes to tune protected evaluation populations. TDI remains the
primary large-scale exploration bench and ITD the independent replication and
integrity bench.
