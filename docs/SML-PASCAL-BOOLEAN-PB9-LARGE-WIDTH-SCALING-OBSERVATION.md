# PB-9 — large-width Pascal scaling observation

Status: retained positive large-width scaling evidence under
`SML-PASCAL-BOOLEAN-PB9-LARGE-WIDTH-SCALING-PREREGISTRATION.md`.

PB-9 extends the retained PB-7 subset-zeta utility surface from K=4096 into
large Boolean states. It is a scaling and task-utility study, not a model
architecture qualification.

## Execution identity

- protocol PR: #224;
- protocol merge: `6c376b197eede8e8c99b8b9d912a59071a8a5f0e`;
- implementation PR: #225;
- executed head: `b74392ecd22c7045679555a0c352bd5b28a1c527`;
- implementation merge: `78964bb670952aceaf41f9020485b5a32adcc5db`;
- GitHub Actions run: `37463073880`;
- job: `112267220560`;
- runner: `tarek-scirust-arm64-01`, machine `tarek`;
- Rust 1.89.0, target `aarch64-unknown-linux-gnu`;
- deterministic replay:
  `research/pascal-boolean-pb9/DETERMINISTIC-REPLAY.csv`;
- deterministic SHA-256:
  `2780d2e82c18ddadfbe2564152a05dc4532e85f47ff055a8186cbe2fa7e2becf`;
- physical replay:
  `research/pascal-boolean-pb9/PHYSICAL-REPLAY.csv`;
- physical SHA-256:
  `a91a9ac01d0c932c1eca754d792d839902d2ea2e6b81d1ad739ff97121ae8dae`.

The mandatory deterministic harness was executed twice in release mode and CI
required byte-identical stdout.

## Mandatory exactness through one million bits

All mandatory widths from K=8,192 through K=1,048,576 satisfy every exactness
contract:

- zero source collisions;
- zero analytic-fixture mismatches;
- zero self-inversion mismatches;
- zero sampled direct-submask oracle mismatches;
- zero designated K-1 direct-oracle mismatches;
- zero independent logical-fast-zeta mismatches;
- exact stage/logical-XOR/packed-update/state-byte/scratch accounting.

At K=1,048,576:

- stages: 20;
- logical XOR updates: 10,485,760;
- packed word updates: 212,992;
- packed state: 131,072 bytes = 128 KiB;
- algorithmic scratch: zero;
- structural full-domain direct reads: 3,486,784,401.

The 3.486-billion-read figure is structural accounting only; PB-9 correctly does
not execute that impractical full-domain direct control.

## Kernel scaling

Mandatory in-place kernel medians:

| K | packed state | kernel median |
| ---: | ---: | ---: |
| 8,192 | 1 KiB | 0.963 us |
| 16,384 | 2 KiB | 2.037 us |
| 32,768 | 4 KiB | 4.324 us |
| 65,536 | 8 KiB | 9.185 us |
| 131,072 | 16 KiB | 19.490 us |
| 262,144 | 32 KiB | 41.254 us |
| 524,288 | 64 KiB | 87.171 us |
| 1,048,576 | 128 KiB | 183.634 us |

The one-million-bit kernel reports about 5.71 billion logical state bits/s on
the frozen ARM64 runner. Clone/copy plus materialization is 185.676 us median,
showing little additional cost at this width.

## One-million-bit query utility

For the frozen affine query schedule at K=1,048,576:

| Q | Pascal median | direct median | interpretation |
| ---: | ---: | ---: | --- |
| 128 | 744.872 us | 233.869 us | direct is 3.185x faster |
| 512 | 761.778 us | 2.037735 ms | Pascal is 2.675x faster |
| 2,048 | 903.755 us | 17.606782 ms | Pascal is 19.482x faster |

At the preregistered L5 cell K=1,048,576/Q=2,048:

- exact A/B checksum equality holds;
- Pascal wins all 12/12 paired repetitions;
- median speedup is 19.4818x;
- median elapsed-time reduction is about 94.867%.

This preserves the PB-7 workload boundary: precomputation is not automatically
better for small query counts. At one million bits, Q=128 still favors direct
evaluation, while Q=512 and Q=2,048 favor Pascal strongly.

## Extension beyond one million bits

No `STOP_RESOURCE` condition fired. Exact analytic fixtures, full-state
self-inversion and accounting remained valid through every preregistered
extension width:

    2,097,152
    4,194,304
    8,388,608
    16,777,216
    33,554,432
    67,108,864
    134,217,728
    268,435,456

The deepest cell, K=268,435,456, has:

- packed state: 33,554,432 bytes = 32 MiB;
- semantic buffers: 96 MiB under the frozen 256 MiB cap;
- kernel median: 81.195066 ms;
- materialization median: 82.536261 ms;
- kernel throughput: about 3.31 billion state bits/s;
- observed VmHWM after the cell: 223,840 KiB;
- stable checksum and exact extension accounting.

The furthest executed width is therefore:

`PB9_FURTHEST_EXECUTED,bits=268435456`

## Gates and decision

- L1 mandatory exactness: HOLDS.
- L2 accounting exactness: HOLDS.
- L3 mandatory physical sanity: HOLDS.
- L4 one-million-bit execution: HOLDS.
- L5 one-million-bit task utility: HOLDS.
- L6 extension integrity: HOLDS.

The retained decision is:

`PB9_DECISION,RETAIN_LARGE_WIDTH_SCALING_AND_1M_UTILITY`

## Interpretation

PB-9 establishes that the current packed-u64 Pascal/subset-zeta primitive is not
limited to the earlier K<=4096 regime. On this runner and implementation it
remains exact and physically tractable through at least 268 million logical
bits, while the one-million-bit query workload preserves a strong high-query
advantage.

The result does not establish that larger is always better. The K=1,048,576
Q=128 cell is an explicit negative boundary where direct evaluation wins.
Likewise the decrease in raw bit throughput from the smallest states to the
deepest extension is expected physical evidence of larger working sets, not a
failure of the transform.

PB-9 still does not establish language quality, learned routing quality,
attention replacement, optimal ANF-bank width, or SBG/MOR/Delta-KV/model
promotion. Those require separate prospective integration protocols.
