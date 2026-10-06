# PB-8 — model-aligned Boolean/ANF gate-bank preregistration

Status: preregistered Development-only model-aligned task protocol.

PB-8 is permitted by retained PB-7 generalized utility evidence. It tests a
narrow model-aligned use case without changing the current SBG -> MOR router,
the native decision engine, Delta-KV, or any production/model path.

## Scientific question

When a bank of Boolean admission/decision predicates is stored as ANF
coefficients over 11 declared Boolean features, can packed Pascal-GF(2)
materialization produce an exact truth-table execution surface with useful
activation-plus-batch cost relative to competent task-native controls?

The candidate semantics are exactly:

    f(x) = XOR_{S subseteq x} a[S]

for 11-bit feature state x and ANF coefficient bits a[S].

This is directly relevant to future Boolean admission, applicability, routing
and decision gates because it represents multi-feature Boolean predicates. It
does not imply that the current two-input `TruthTable2` heads or the current
XNOR/POPCOUNT MOR router should be replaced.

## Frozen domain

- variables: 11;
- input domain: K=2048 feature states;
- gate bank: 16 predicates per density regime;
- all masks fit in u16;
- all implementations are integer/Boolean only;
- no learned model, language data or protected population is used.

## ANF density regimes

Each bank contains 16 deterministic gates with exactly one of these active
coefficient counts per gate:

    D4    = 4
    D32   = 32
    D256  = 256
    D1024 = 1024

For every gate/density pair:

- active monomial masks are distinct;
- mask 0 is permitted and is the ANF constant coefficient;
- the generator seed binds protocol version, density and gate index;
- a generated bank is frozen before timing;
- no coefficient may be added/removed in response to benchmark results.

The explicit sorted monomial list and the packed 2048-bit coefficient bitmap
must encode exactly the same polynomial.

## Record populations

The same complete K=2048 state schedules used by PB-7 are frozen:

### S0 — AFFINE_UNIFORM

    x_i = (405*i + 17) mod 2048

### S1 — LOW_POPCOUNT

Stable sort all x by:

    (popcount(x), x)

ascending.

### S2 — HIGH_POPCOUNT

Stable sort all x by:

    (-popcount(x), x)

with x ascending inside each popcount band.

Frozen record-prefix sizes:

    Q = 32, 128, 512, 2048

Frozen reuse counts per one bank activation:

    R = 1, 4, 16

A cell evaluates every one of the 16 gates for every selected record and repeats
the same frozen record batch R times after the arm's declared activation step.

## Arms

Every arm must return the same 16 Boolean gate results per record.

### A — PASCAL_ANF_MATERIALIZE

For each gate:

1. clone/load its 2048-bit packed ANF coefficient bitmap;
2. apply `pascal_gf2_in_place` once;
3. interpret the transformed 2048 bits as the exact truth table;
4. evaluate records by direct truth-table bit lookup.

The transform is in-place. Logical active table storage is 256 bytes/gate.

### B1 — SPARSE_ANF_SCAN

Use the exact sorted active monomial list for each gate.

For each record x:

    result = XOR(mask subseteq x for mask in active_monomials)

where `mask subseteq x` is tested exactly by `mask & x == mask`.

This is the competent sparse-polynomial control and is expected to be strong
for D4/D32.

### B2 — SUBMASK_COEFFICIENT_QUERY

Keep the packed coefficient bitmap.

For each record x, enumerate every binary submask S subseteq x and XOR a[S].
This is the competent low-popcount/control-path evaluator and must preserve the
PB-7 workload dependence rather than forcing every direct arm to scan all
monomials.

### C — GENERIC_TRUTH_MATERIALIZE

Starting from the same packed coefficient bitmap as A, build all 2048 truth
values by independent direct subset enumeration and then use direct truth-table
lookup for the records.

C is the matched same-source materialization control.

### D — RESIDENT_TRUTH_TABLE

Use an already materialized exact 2048-bit truth table and direct lookup only.

D is a hot-resident latency reference, NOT a primary activation control. It
starts from a different storage/lifecycle contract because the materialization
result is already resident. A is not required to beat D. D exists to quantify
the value of keeping compiled truth tables resident when the extra lifecycle
state is acceptable.

## Independent oracle

The oracle evaluates the explicit active monomial list with a deliberately
simple scalar subset test. The oracle implementation must be independent of
packed Pascal and of the generic truth-table materializer.

## Deterministic matrix

Run all:

    4 densities
  x 3 record populations
  x 4 Q values
  x 3 reuse values
  = 144 cells

over one frozen 16-gate bank per density.

For every cell report:

- total gate evaluations = 16 * Q * R;
- A/B1/B2/C/D mismatch counts against the independent oracle;
- per-arm checksum;
- A/C truth-table materialization mismatch count;
- active monomial count;
- B1 subset-test count;
- B2 coefficient-read count;
- A packed-zeta logical XOR count;
- C direct materialization coefficient-read count;
- A/C/D logical truth-table bytes;
- B1 logical monomial-list bytes (u16 mask payload only);
- activation count and lookup count.

CI must execute deterministic mode twice with `--locked --release` and require
byte-identical stdout.

## Frozen deterministic gates

### M1 — exact model-aligned semantics

Across all 144 cells:

- all A/B1/B2/C/D outputs equal the independent ANF oracle;
- A and C materialized truth tables are byte-equivalent;
- all mismatch counters are zero.

### M2 — representation accounting

For every gate:

- A packed coefficient/truth payload = 256 bytes in-place;
- C packed truth payload = 256 bytes;
- D resident truth payload = 256 bytes;
- B1 monomial mask payload = 2 * active_coefficients bytes;
- no hidden 2048 x 2048 matrix or membership table is charged to A.

Metadata required by every arm must be separately reported if added.

### M3 — competent sparse-control boundary

For D4 at Q=32,R=1, SPARSE_ANF_SCAN must have fewer declared logical predicate
checks than A has preprocessing XOR updates plus lookups.

For D1024 at Q=2048,R=16, A must have fewer declared logical preprocessing +
lookup operations than SPARSE_ANF_SCAN subset tests.

This gate validates that the frozen matrix genuinely contains both a sparse
direct-favorable structural regime and a dense/amortized Pascal-favorable
structural regime.

### M4 — submask population ordering

For every common density/Q/R cell:

    B2_reads_LOW_POPCOUNT
      <= B2_reads_AFFINE_UNIFORM
      <= B2_reads_HIGH_POPCOUNT

## Physical benchmark

Physical timing is separate from deterministic replay.

Frozen environment:

- runner: `tarek-scirust-arm64-01`;
- machine: `tarek`;
- Rust 1.89.0;
- target: `aarch64-unknown-linux-gnu`;
- two warmup repetitions;
- 12 measured repetitions;
- arm order rotates by repetition;
- `black_box` protects inputs/checksums;
- no file I/O in timed regions;
- each timed primary arm starts from the lifecycle state declared above.

To keep CI bounded, benchmark these 17 frozen cells.

### P0 — density sweep

S0 AFFINE_UNIFORM, Q=512, R=1 for:

    D4, D32, D256, D1024

### P1 — amortization surface

D256, S0 AFFINE_UNIFORM for:

    Q in {128,512,2048}
    R in {1,4,16}

Nine cells.

### P2 — population stress

D256, R=1 for:

    S1 LOW_POPCOUNT  at Q in {128,512}
    S2 HIGH_POPCOUNT at Q in {128,512}

Four cells.

For each cell report A/B1/B2/C/D raw 12 elapsed-ns values, median, p95,
gate-evaluations/s and exact checksum.

## Frozen physical gates

### M5 — same-source materialization utility

In all four P0 density cells:

    10 * median_A <= 9 * median_C

This tests Pascal as an ANF-to-truth materializer against the matched generic
same-source materializer.

### M6 — dense gate-bank task utility

For P0 D256 and D1024, A must be at least 10% faster than the faster
activation-compatible direct control B1/B2/C:

    10 * median_A <= 9 * min(median_B1, median_B2, median_C)

A is not compared to D for this gate because D starts pre-materialized.

### M7 — amortized task utility

For D256/S0/Q=128/R=16 and D256/S0/Q=2048/R=1, A must be at least 10% faster
than the faster B1/B2/C median.

These two cells distinguish reuse amortization from one large batch.

### M8 — benchmark sanity

Physical evidence is INVALID if any required cell has:

- checksum mismatch;
- wrong runner/architecture/toolchain;
- missing or extra repetitions;
- zero elapsed time;
- wrong gate count, Q, R or schedule;
- semantics differing from deterministic mode.

Performance losses do not fail CI. They produce mixed/negative scientific
evidence.

## Required interpretation

The report must separately state:

- where sparse ANF direct evaluation wins;
- where submask query evaluation wins;
- where Pascal materialization wins;
- the cost of keeping D resident;
- whether Pascal's activation cost amortizes with Q and R;
- that D defines the hot-resident lookup ceiling and may remain preferable when
  a truth table can simply stay resident.

No claim may silently compare A's activation-plus-query timing against D's
lookup-only timing as though they had the same lifecycle state.

## Decision

If M1..M8 all hold:

    PB8_DECISION,RETAIN_MODEL_ALIGNED_GATE_UTILITY

If M1..M4 and M8 hold but one or more M5..M7 performance gates fail:

    PB8_DECISION,RETAIN_MIXED_MODEL_ALIGNED_GATE_SURFACE

If exact semantics, required fixtures or benchmark sanity fail:

    PB8_DECISION,INVALID

## Promotion boundary

Even `RETAIN_MODEL_ALIGNED_GATE_UTILITY` does not authorize replacement of
current MOR XNOR/POPCOUNT routing, current two-input decision heads, SBG
semantics, Delta-KV, or production inference.

A positive PB-8 result would justify only a separately preregistered integration
study in which Pascal-derived multi-feature gates are introduced as an explicit
new candidate arm against the existing native routing/decision path.
