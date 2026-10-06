# PB-7 — Pascal utility-surface generalization preregistration

Status: preregistered Development-only utility-generalization protocol.

PB-7 follows retained PB-6 task-utility evidence. It does not reopen the closed
PGF2 representation-mixing branch and it does not test language-model quality.
Its purpose is to determine where the exact subset-zeta/Pascal advantage
survives changes in logical width, query load and query-address structure.

## Scientific question

For exact subset-parity queries over power-of-two Boolean domains, how does the
utility of one packed Pascal/subset-zeta precompute vary with:

- state width K;
- number of queries Q;
- the popcount distribution of queried addresses?

PB-7 is an attribution/generalization study. A positive result establishes a
bounded utility surface for fast subset-zeta semantics. It does not establish
that the algorithm is novel, universally optimal, or beneficial for SML model
quality.

## Candidate and controls

For every cell all arms receive the same source state and exact address list.

### A — PACKED_PASCAL_ZETA

Apply `pascal_gf2_in_place` once to the K-bit source and answer each query by
one transformed-state bit read.

Structural preprocessing work is frozen as:

    zeta_xor(K) = (K / 2) * log2(K)

plus Q transformed-state reads.

### B — DIRECT_ON_DEMAND

For each queried address n, enumerate all submasks k subseteq n and XOR the
corresponding source bits directly.

Exact source-read count is:

    direct_reads(addresses) = sum_n 2^popcount(n)

No transformed answer vector is retained.

### C — GENERIC_FULL_PRECOMPUTE

Compute every one of the K subset-parity answers by direct submask enumeration,
store one K-bit answer vector, and answer queries by direct reads.

Exact preprocessing source-read count is:

    full_direct_reads(K) = 3^log2(K)

A and C therefore retain the same logical answer-vector size K/8 bytes.

### Attribution reference — LOGICAL_FAST_ZETA

The implementation must also contain an independently written logical-bit fast
subset-zeta reference used for semantic equivalence and operation-count
attribution. It is not a primary latency control: it computes the same fast
zeta transform and therefore cannot establish Pascal-specific novelty merely by
being slower or faster in one representation.

## Width ladder

Frozen widths:

    K = 64, 128, 256, 512, 1024, 2048, 4096

All widths are powers of two and exactly representable by the current packed
u64 primitive.

## Deterministic source corpus

For every width:

- 64 deterministic source states;
- seed family must bind both width and sample index;
- source collisions within a width must be zero;
- no source may be selected from benchmark outcomes.

## Address schedules

For each width construct exactly three complete permutations of 0..K-1.

### S0 — AFFINE_UNIFORM

    address_i = (405 * i + 17) mod K

405 is odd, therefore this is a permutation for every power-of-two K.

### S1 — LOW_POPCOUNT

Stable sort all addresses by:

    (popcount(address), address)

ascending.

### S2 — HIGH_POPCOUNT

Stable sort all addresses by:

    (-popcount(address), address)

with address ascending inside equal-popcount bands.

LOW_POPCOUNT is the cheapest possible prefix ordering for direct enumeration;
HIGH_POPCOUNT is the most expensive possible prefix ordering. AFFINE_UNIFORM is
the frozen mixed reference.

## Query loads

For each width, form the sorted unique set:

    Q = {1, max(1,K/64), K/16, K/4, K/2, K}

Any duplicate created at K=64 is represented once.

Every arm receives the identical prefix of the selected schedule.

## Deterministic evidence

For every width × schedule × Q cell report:

- answer count;
- A/B/C/oracle checksums;
- A/B/C mismatches;
- packed Pascal versus logical-fast-zeta mismatches;
- exact direct source reads;
- exact direct XOR reductions;
- packed-zeta logical XOR updates;
- packed-zeta query reads;
- C full-precompute source reads;
- A and C retained answer bytes;
- source-collision count;
- earliest structural crossover Q for each width/schedule.

CI must execute deterministic mode twice under `--locked --release` and require
byte-identical stdout.

## Frozen deterministic gates

### G1 — exactness

Across every deterministic cell:

- A, B and C answers equal the independent direct Boolean oracle;
- packed Pascal equals LOGICAL_FAST_ZETA;
- all mismatch counters are zero.

### G2 — storage parity

For every width:

    A_answer_bytes = C_answer_bytes = K / 8

No dense K×K relation/membership table may be attributed to A.

### G3 — full-domain structural advantage

At Q=K for every frozen width:

    zeta_xor(K) + K < full_direct_reads(K) + K

and the ratio:

    full_direct_reads(K) / zeta_xor(K)

must increase monotonically over the frozen width ladder.

Use integer comparisons for all gates.

### G4 — address-popcount bounds

For every width and common Q:

    reads_LOW_POPCOUNT <= reads_AFFINE_UNIFORM <= reads_HIGH_POPCOUNT

This gate validates that the schedule family really spans favorable and
unfavorable direct-query populations.

## Physical benchmark

Physical timing is separate from deterministic replay.

Frozen environment:

- runner: `tarek-scirust-arm64-01`;
- machine: `tarek`;
- target: `aarch64-unknown-linux-gnu`;
- Rust: 1.89.0;
- 8 deterministic physical source states per cell;
- two warmup repetitions;
- 12 measured repetitions per arm;
- rotating A/B/C arm order by repetition;
- `std::hint::black_box` on inputs and accumulated output;
- no file I/O in timed regions;
- checksums must match deterministic semantics.

Two physical panels are required.

### P0 — full-domain width sweep

For every frozen width:

- schedule S0 AFFINE_UNIFORM;
- Q=K;
- benchmark A, B and C.

### P1 — K=2048 load/schedule surface

At K=2048:

- schedules S0, S1 and S2;
- Q in {32, 128, 512, 1024, 2048};
- benchmark A, B and C.

For every physical cell report all 12 elapsed-ns values, median, p95, derived
states/s and checksum.

## Frozen physical gates

### G5 — PB-6 signal replication

At K=2048, S0, Q=2048:

    10 * median_A <= 9 * min(median_B, median_C)

and A must beat the faster same-index control in at least 9 of 12 measured
repetitions.

This is a new execution and not a reuse of PB-6 timing evidence.

### G6 — width generalization

At full-domain Q=K for each width in:

    512, 1024, 2048, 4096

A must be at least 10% faster than the faster matched B/C median:

    10 * median_A <= 9 * min(median_B, median_C)

A failure at 64/128/256 is retained but does not by itself refute G6 because
those widths are explicitly the small-state crossover controls.

### G7 — benchmark sanity

The physical evidence is INVALID if any required cell has:

- checksum mismatch;
- wrong runner/architecture/toolchain;
- missing or extra measured repetitions;
- zero elapsed time;
- wrong source/query count;
- semantics differing from deterministic mode.

No timing gate is allowed to fail CI merely because Pascal loses. Negative
performance evidence is a valid scientific result.

## Decision

If G1..G6 hold and G7 is valid:

    PB7_DECISION,RETAIN_GENERALIZED_UTILITY_SURFACE

If G1..G4 hold and G7 is valid but G5 or G6 fails:

    PB7_DECISION,RETAIN_MIXED_UTILITY_SURFACE

If deterministic semantics, required fixtures or physical sanity are invalid:

    PB7_DECISION,INVALID

A mixed result must be retained rather than repaired by changing widths,
schedules, query loads or thresholds after observation.

## Promotion boundary

PB-7 does not authorize SBG, MOR, Delta-KV, context or model integration.

If PB-7 retains generalized utility, the next SML-side Pascal experiment may be
a separately preregistered model-aligned task study. TDI remains the primary
large-scale exploration bench and ITD the independent replication/integrity
bench; evidence from either repository does not silently change this protocol.
