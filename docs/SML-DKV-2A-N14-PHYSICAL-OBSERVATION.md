# DKV-2a-N14 — physical resource qualification observation

Status: retained physical-resource observation under
`SML-DKV-2A-N14-PHYSICAL-PREREGISTRATION.md`.

## Execution identity

- protocol PR: #206;
- protocol merge: `ece963595218998f227a2b41bd8838b41dba73a1`;
- implementation PR: #207;
- executed branch head: `c5ef293f0ddcd67b2752207d98c4b857cee0aa11`;
- pull-request test-merge SHA exposed as `GITHUB_SHA`:
  `f6d88be9a0bd5075946adc64cc7ebe48f252c1cb`;
- implementation merge: `b3318b9e9a8c2941d57ddacfe6961d78bf039731`;
- GitHub Actions run: `37423200253`;
- job: `112136946390`;
- runner: `tarek-scirust-arm64-01`;
- Rust 1.89.0;
- architecture: aarch64;
- kernel: `6.8.12-tegra`;
- 14 logical CPUs visible, affinity `0-13`;
- governor: `schedutil`;
- CPU model string: unavailable through the frozen unprivileged probe;
- retained output: `research/dkv-2a-n14/CURRENT-REPLAY.csv`;
- output SHA-256:
  `5b614d4889ac26d3dc4d1a172dc4f98d5e920f9729357d544b6f20decb0eaed4`;
- manifest: `research/dkv-2a-n14/MANIFEST.json`.

The run contains 24 cells:
2 widths x 4 arms x 3 page populations. Each cell covers eight seeds,
144 records/seed, two warmup passes and eight measured passes, for 9,216 timed
queries per cell.

The DIRECT_PRIMARY and DIRECT_MIXED arms are explicitly non-authoritative
numeric diagnostics. They do not mint VerifiedPresent provenance.

## H1-H5 physical and semantic invariants

Across all 24 cells:

- H1 semantics — HOLDS;
- H2 Linux instrumentation — HOLDS;
- H3 measured block-device read/write bytes are zero — HOLDS;
- H4 measured major page faults are zero — HOLDS;
- H5 exactly one selected-bank matrix read per query — HOLDS.

The verified PRIMARY/MIXED paths retain the current N12 source/record/subject
preflight. The direct controls omit that preflight only for cost decomposition.

No physical counter changes bank selection, provenance or answer
interpretation.

## Verified K=1024 latency and resident-memory observations

All values below are from the single retained runner/run. They are not
cross-machine guarantees.

| Workload | Population | p50 | p95 | p99 | measured exact | measured RSS max |
|---|---|---:|---:|---:|---:|---:|
| PRIMARY | BALANCED | 13.602 us | 14.231 us | 14.842 us | 99.22% | 3.000 MiB |
| PRIMARY | N13A_EMPIRICAL | 13.537 us | 14.186 us | 14.730 us | 98.44% | 3.000 MiB |
| PRIMARY | HEAVY_SKEW | 13.639 us | 15.397 us | 16.370 us | 96.96% | 3.000 MiB |
| MIXED | BALANCED | 13.592 us | 14.258 us | 14.778 us | 99.39% | 3.375 MiB |
| MIXED | N13A_EMPIRICAL | 13.703 us | 16.250 us | 17.703 us | 98.52% | 3.250 MiB |
| MIXED | HEAVY_SKEW | 13.657 us | 16.824 us | 18.314 us | 97.66% | 3.250 MiB |

The measured-exact counts reproduce the corresponding warmup rates exactly
after accounting for the 4x pass-count difference required by H1. N14 does not
turn those synthetic Development recall rates into a model-quality claim.

## H6 — K1024 versus K256 width scaling

The frozen protocol sets no pass threshold for width scaling.

For PRIMARY, K1024/K256 ratios are:

| Population | p50 | p95 | p99 | CPU runtime | process RSS max |
|---|---:|---:|---:|---:|---:|
| BALANCED | 2.256x | 1.702x | 1.687x | 2.496x | 1.200x |
| N13A_EMPIRICAL | 2.246x | 1.723x | 1.660x | 2.513x | 1.200x |
| HEAVY_SKEW | 2.270x | 1.840x | 1.865x | 2.740x | 1.200x |

For MIXED:

| Population | p50 | p95 | p99 | CPU runtime | process RSS max |
|---|---:|---:|---:|---:|---:|
| BALANCED | 2.255x | 1.944x | 1.644x | 2.330x | 1.350x |
| N13A_EMPIRICAL | 2.288x | 2.619x | 2.704x | 2.891x | 1.300x |
| HEAVY_SKEW | 2.252x | 2.021x | 2.104x | 2.078x | 1.238x |

The numeric matrix itself has exactly 4x as many coefficient bytes at K1024:
4,096 bytes per bank versus 1,024 bytes at K256 for value_dim=2. Including the
fixed DeltaKvMemory semantic metadata, `semantic_bits()` is 32,960 bits versus
8,384 bits, a 3.931x ratio.

Process RSS does not scale by that logical matrix ratio because it includes
runtime, allocator, provenance, directory, history and process overhead.

## H7 — verified path versus matched numeric-read diagnostic

The verified/direct p50 ratio at K1024 is narrow across the three populations:

- PRIMARY / DIRECT_PRIMARY: 1.458x to 1.466x;
- MIXED / DIRECT_MIXED: 1.462x to 1.473x.

K1024 p95 verified/direct ratios range from 1.151x to 1.736x and p99 from
1.160x to 1.754x. The corresponding CPU-runtime ratios range from 1.476x to
1.732x.

At K256 the verification fraction is larger relative to the cheaper matrix
read:

- p50 ratios: 1.803x to 1.897x;
- p95 ratios: 1.376x to 2.605x;
- p99 ratios: 1.365x to 2.716x;
- CPU-runtime ratios: 1.905x to 3.637x.

These are whole-path ratios on one unpinned self-hosted run. They do not isolate
individual verification sub-operations and must not be generalized as fixed
overhead constants.

## Process I/O and page faults

Every measured cell reports:

- `read_bytes = 0`;
- `write_bytes = 0`;
- major page faults = 0.

Minor page faults are nonzero and retained in the raw evidence. `rchar`,
`wchar`, syscall counts and minor faults remain process/kernel telemetry, not
DRAM traffic.

A zero `read_bytes` value means the kernel did not charge block-device reads
to the process during the measured hot interval. It does not prove zero page
cache activity or zero memory-bus traffic.

## H8 — energy availability

H8 is REFUTED on every one of the 24 cells:

`N14_ENERGY_STATUS,UNAVAILABLE,batches=0,domains=0`.

The runner exposes no readable top-level monotonic powercap `energy_uj`
domain under the frozen unprivileged probe. No energy value is estimated from
CPU time, utilization, TDP or an instantaneous power reading.

This is an instrumentation-capability negative result, not evidence of zero
energy consumption.

## Decision

N14 retains valid physical latency/RSS/process-I/O/page-fault/CPU-runtime
evidence for this exact runner and candidate because H1-H7 are satisfied or
measured as preregistered.

The **full physical-resource gate remains open** because H8 is unavailable.
No PageSeeded/two-bank core API promotion follows from this run.

The next qualifying action is to replay the unchanged frozen N14 candidate and
measurement protocol on a pinned host exposing a valid monotonic energy
counter. No key, route, workload, threshold or semantic rule may be changed to
obtain that measurement.

Context/model promotion remains BLOCKED.
