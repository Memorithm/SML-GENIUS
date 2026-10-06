# PB-6 — matched-control Pascal task-utility preregistration

Status: preregistered task-utility protocol.

Prerequisites are the retained PB-1..PB-5 functional contracts. PB-6 does not
reopen the closed PGF2 representation-mixing branch. It asks a narrower question:
does the retained Pascal subset machinery provide measurable operational utility
on a task that actually requires subset-parity queries?

## Question

For exact K=2048 subset-parity queries, does P-ZETA preprocessing plus direct
reads reduce deterministic work and measured wall-clock cost relative to matched
exact controls, while preserving the same answers?

The experiment is prospective. No query schedule, threshold, arm, repetition
count or benchmark criterion may be changed after observing PB-6 output.

## Common pipeline

Logical state width is K=2048 = 2^11.

Every arm receives the same deterministic source state and performs the same
common work:

1. four-round retained P-FEISTEL encode;
2. exact P-FEISTEL decode;
3. the same frozen PB-5 ANF address gate;
4. the same deterministic query addresses;
5. final answer = subset_parity AND ANF_gate.

Only the subset-parity engine differs between arms.

The Feistel and ANF work is therefore matched common overhead. The experiment
must report subset-engine work separately from whole-pipeline physical timing.

## Arms

### A — P_ZETA_PRECOMPUTE

For each decoded K=2048 source:

1. compute the packed P-ZETA transform once;
2. answer every requested address by reading transformed bit z[n].

Required retained structural cost per source before queries:

- 11 stages;
- 11,264 logical XOR updates;
- 272 packed-word updates;
- 256 transformed-state bytes;
- zero algorithmic scratch words.

Each query then consumes one transformed-state bit read plus the matched final
Boolean AND.

### B — DIRECT_ON_DEMAND

For every requested address n:

1. enumerate all submasks k subseteq n;
2. XOR the corresponding decoded source bits directly;
3. apply the same ANF gate.

No transformed answer state is retained.

The harness must count exact source-bit reads and XOR reductions induced by the
frozen query schedule.

### C — GENERIC_FULL_PRECOMPUTE

For every decoded source:

1. compute all 2,048 subset-parity answers by direct submask enumeration;
2. store the resulting 2,048-bit answer vector;
3. answer requested addresses by direct reads;
4. apply the same ANF gate.

This arm has the same 256-byte logical answer-vector storage as P-ZETA but does
not use the Pascal fast transform. It controls for the generic benefit of
precomputation itself.

## Corpus

- 64 deterministic distinct K=2048 source states;
- corrected non-aliasing stream from PB-1..PB-5;
- Feistel depth exactly four rounds;
- frozen PB-5 ANF gate unchanged.

Source uniqueness and decoded equality must be checked.

## Query schedule

The complete address domain is 0..2047.

For each source, build one deterministic permutation of all 2,048 addresses:

    address_i = (405 * i + 17) mod 2048

405 is odd, so this is a permutation modulo 2^11.

Frozen query-batch prefix sizes:

    Q = 1, 16, 64, 128, 256, 512, 1024, 2048

Every arm receives exactly the same prefix for a given Q.

The harness must also report the exact DIRECT_ON_DEMAND source-read count for
each Q so that any nonuniform popcount composition of a prefix is explicit.

No post-hoc address resampling is allowed.

## Deterministic functional harness

The deterministic harness must execute every arm, source and Q and report:

- source collisions;
- Feistel inverse failures;
- ANF-gate mismatches;
- per-arm final answer mismatches;
- per-Q answer counts;
- per-Q exact DIRECT_ON_DEMAND source reads;
- per-Q P-ZETA preprocessing XOR work plus query reads;
- GENERIC_FULL_PRECOMPUTE exact full-domain source reads plus query reads;
- answer-vector storage bytes;
- structural break-even status at each Q;
- checksum of all produced answers for every arm.

CI must execute the deterministic harness twice under `--locked --release` and
require byte-identical stdout.

## Physical benchmark harness

Physical timing is intentionally separate from deterministic replay because
wall-clock values are not byte-stable evidence.

Frozen benchmark workload:

- Q=2048 only;
- all 64 source states;
- 20 measured repetitions per arm after 3 warmup repetitions;
- arm order rotates by repetition:
  - repetition r mod 3 = 0: A,B,C;
  - repetition r mod 3 = 1: B,C,A;
  - repetition r mod 3 = 2: C,A,B;
- every timed arm must produce the same predeclared answer checksum;
- use `std::hint::black_box` on inputs and accumulated output checksum;
- measure complete common pipeline plus subset engine and query evaluation;
- no file I/O inside a timed region.

For each arm, report:

- 20 individual elapsed nanosecond values;
- median elapsed ns;
- p95 elapsed ns;
- derived states/s;
- checksum.

The benchmark runs once in CI after the deterministic double-replay has passed.
A performance REFUTED result does not fail CI; it is valid scientific evidence.

## Frozen structural hypotheses

### U1 — exact task equivalence

For every source, Q and query:

- A, B and C final answers are identical;
- all equal the independent direct Boolean oracle.

Required mismatches: 0.

### U2 — P-ZETA structural utility at moderate/high query load

At Q in {256, 512, 1024, 2048}:

    P_ZETA_work = 11,264 logical XOR updates + Q transformed-bit reads

must be strictly less than the exact DIRECT_ON_DEMAND source-read count for the
same frozen prefix.

This is a deterministic work-count comparison, not a wall-clock claim.

### U3 — fast-precompute advantage

For the complete 2,048-address domain:

- P-ZETA preprocessing logical XOR updates = 11,264;
- GENERIC_FULL_PRECOMPUTE direct source reads = 177,147.

Required ratio:

    generic_full_reads / p_zeta_xor_updates >= 15

Integer cross-multiplication must be used for the gate; no floating-point
rounding may decide it.

### U4 — storage parity

P_ZETA_PRECOMPUTE and GENERIC_FULL_PRECOMPUTE both retain exactly one 2,048-bit
answer vector = 256 logical bytes.

No hidden dense 2048x2048 membership matrix may be counted as part of A.

## Frozen physical hypothesis

### U5 — measured whole-pipeline utility

At Q=2048 on the frozen ARM64 self-hosted runner:

- A checksum must equal B checksum and C checksum;
- median elapsed time of A must be at least 10% lower than the faster matched
  control median.

Gate without floating point:

    10 * median_A <= 9 * min(median_B, median_C)

Additionally, A must be faster than the faster control in at least 16 of the 20
same-index measured repetitions after applying the frozen rotating arm order.

The paired-win criterion is descriptive pairing by repetition number; arm
execution order rotates to reduce systematic ordering bias.

If U5 fails, PB-6 task utility is REFUTED even if structural gates hold.

## U6 — benchmark sanity

The physical benchmark is INVALID rather than REFUTED if any of the following
occurs:

- checksum mismatch;
- fewer or more than 20 measured repetitions per arm;
- zero elapsed time;
- source or answer count mismatch;
- the benchmark does not execute on the expected aarch64 self-hosted runner;
- timed code differs in answer semantics from the deterministic harness.

## Decision

If U1..U5 hold and U6 is valid:

    PB6_DECISION,RETAIN_TASK_UTILITY

If deterministic correctness is valid but U2/U3/U4/U5 fails:

    PB6_DECISION,CLOSE_TASK_UTILITY

If fixtures, checksums, runner identity or shared primitives are invalid:

    PB6_DECISION,INVALID

A RETAIN_TASK_UTILITY result establishes only an exact subset-parity workload
advantage under this frozen synthetic panel. It does not establish language
quality, semantic retrieval benefit, training benefit, or model-level promotion.

## Reproducibility and retention

Implementation must:

- keep the deterministic harness separate from the physical benchmark;
- run deterministic evidence twice and compare stdout byte-for-byte;
- run the physical benchmark once after deterministic success;
- print runner/rust/target identity in physical evidence;
- retain deterministic replay, physical benchmark replay and SHA-256 manifests
  only after a green execution PR;
- preserve negative or null performance evidence.

No SBG/MOR/Delta-KV/context/model integration may begin from PB-6 without a
separate prospective protocol.
