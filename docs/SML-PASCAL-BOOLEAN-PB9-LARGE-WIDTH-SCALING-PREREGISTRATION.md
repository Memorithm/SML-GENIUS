# PB-9 — large-width Pascal scaling preregistration

Status: preregistered Development-only scaling protocol.

PB-9 is independent of the closed PGF2 representation-mixing objective and of
the PB-8 model-aligned gate-bank claim. It extends the retained PB-7 exact
subset-zeta utility surface into widths where exhaustive direct full-domain
controls become computationally unreasonable.

The primary user-requested target is K=1,048,576 bits. A bounded conditional
extension probes substantially larger widths when the same implementation
remains within frozen time and memory budgets.

## Questions

PB-9 asks four separate questions.

1. Does the current packed-u64 Pascal/subset-zeta implementation remain exactly
   correct through K=1,048,576?
2. How do kernel latency, materialization latency and memory footprint scale
   across the large-width ladder?
3. Does the PB-7 query-utility advantage survive at K=1,048,576 for a frozen
   affine query population?
4. How far beyond K=1,048,576 can the same implementation execute safely before
   a preregistered resource stop is reached?

No result from PB-9 establishes language-model quality, attention replacement,
learned routing quality, or model-level promotion.

## Width ladders

### Mandatory ladder

Every mandatory cell MUST execute:

    K = 8,192
        16,384
        32,768
        65,536
        131,072
        262,144
        524,288
        1,048,576

These are 2^13 through 2^20.

### Extended ladder

After the mandatory ladder has completed, transform-only correctness and
physical scaling continue in order through:

    K = 2,097,152
        4,194,304
        8,388,608
        16,777,216
        33,554,432
        67,108,864

These are 2^21 through 2^26.

### Deep extension

If the extended ladder reaches K=67,108,864 without a resource stop, the same
transform-only protocol may continue to:

    K = 134,217,728
        268,435,456

These are 2^27 and 2^28. The K=2^28 packed state is exactly 32 MiB.

No width may be inserted, removed or reordered after observing PB-9 results.

## Frozen structural accounting

For K=2^m:

    stages              = m
    logical_xor_updates = (K / 2) * m
    state_bytes         = K / 8
    scratch_words       = 0

The harness must compare these expected values with the
`PascalGf2Stats` returned by `pascal_gf2_in_place`.

The direct full-domain subset oracle would require:

    full_direct_reads = 3^m

source-bit reads. This quantity is reported structurally only once it becomes
too large to execute. PB-9 MUST NOT attempt billions or trillions of direct
reads merely to preserve a control that is already computationally dominated.

For reference, at K=1,048,576 (m=20):

    logical_xor_updates = 10,485,760
    state_bytes         = 131,072
    full_direct_reads   = 3,486,784,401

The last quantity is accounting evidence, not an executed benchmark.

## Correctness programme

Correctness is deliberately tiered so that stronger small/medium-width oracles
are not confused with impractical exhaustive large-width work.

### C0 — analytic exact fixtures at every executed width

At every mandatory, extended and deep width, execute all of:

- all-zero input: transformed output must remain all zero;
- all-one input: only output address zero may be one;
- basis bit k=0: every output address must be one;
- basis bit k=K/2: output is one iff that basis index is a bit-submask of the
  output address;
- basis bit k=K-1: only output address K-1 may be one.

The expected outputs are generated analytically from the submask identity,
without calling the Pascal implementation.

### C1 — full-state self-inversion

For deterministic dense states:

    P(P(x)) == x

must hold byte-for-byte.

Mandatory widths use 8 distinct dense sources each.
Extended/deep widths use 2 distinct dense sources each.

Source seeds bind the width and sample index and must be collision-checked
within each width.

### C2 — sampled independent direct subset oracle

For every mandatory width and four dense sources, compare packed Pascal output
against direct submask enumeration at exactly 64 deterministic addresses whose
popcount is bounded by 12.

The address generator and selected addresses are printed in deterministic
evidence. Duplicate addresses are forbidden.

Additionally, for one designated dense source per mandatory width, address
K-1 is checked directly even though it requires K source reads. This provides
one full-popcount exact oracle point through K=1,048,576.

Direct sampled oracle checks are not extended above K=1,048,576.

### C3 — independent logical fast-zeta reference

For one designated dense source at every mandatory width, an independently
written logical-bit fast subset-zeta implementation must match the packed
output at every bit.

The reference must not call `pascal_gf2_in_place`, share its packed-stage
implementation, or use its output as an intermediate.

C3 ends at K=1,048,576. Above that point C0+C1 remain the exact full-state
contracts.

## Deterministic replay

Deterministic mode reports, for every width:

- width and log2 width;
- source count and source collisions;
- expected and observed stages;
- expected and observed logical XOR updates;
- expected and observed packed-word updates;
- expected and observed state bytes;
- scratch words;
- analytic-fixture mismatches;
- self-inverse mismatches;
- sampled-direct mismatches where applicable;
- K-1 direct-oracle mismatch where applicable;
- logical-fast-zeta mismatches where applicable;
- input and transformed checksums.

CI executes deterministic mandatory mode twice under `--locked --release`
and requires byte-identical stdout.

Extended/deep deterministic transform evidence may be executed in the physical
step because its resource-stop status is machine-dependent.

## Physical kernel scaling

Pinned environment:

- runner: `tarek-scirust-arm64-01`;
- machine: `tarek`;
- target: `aarch64-unknown-linux-gnu`;
- Rust: 1.89.0;
- no file I/O inside timed regions;
- `std::hint::black_box` on timed input/output checksums;
- buffers are allocated before kernel-only timing.

Two latency quantities are kept separate.

### P0 — in-place kernel

Time only:

    pascal_gf2_in_place(buffer)

after the source has been restored outside the timed region.

### P1 — materialize path

Time:

    clone/copy source into destination
    + pascal_gf2_in_place(destination)

This measures the cost relevant to materializing a transformed bank from an
unchanged source.

For mandatory widths:

- 2 warmups;
- 12 measured repetitions;
- report every elapsed-ns value, median and p95.

For extended widths:

- 1 warmup;
- 7 measured repetitions.

For deep-extension widths:

- 1 warmup;
- 5 measured repetitions.

Every timed output must have a deterministic checksum consistent with the
correctness path.

For each width also report:

- bits/s from the median;
- logical-XOR/s from the median;
- packed-word-updates/s from the median;
- state bytes;
- Linux VmRSS and VmHWM before/after the cell when available;
- whether the state fits the frozen semantic-buffer cap.

Cache sizes may be reported from the execution environment when available, but
cache-boundary interpretations are descriptive and are not acceptance gates.

## Resource-stop contract for widths above K=1,048,576

The mandatory ladder MUST NOT be skipped because of these extension rules.

Before each extended/deep width, perform one untimed allocation/probe.

The next width is recorded as `STOP_RESOURCE` and later widths are not
attempted if any of the following is true:

- required semantic buffers would exceed 256 MiB in aggregate;
- allocation fails;
- the untimed single transform exceeds 2 seconds;
- the previous executed width has median P1 materialization latency above
  1 second;
- arithmetic/accounting overflows;
- the process cannot obtain the required source/destination buffers without
  violating the cap.

A resource stop is valid evidence, not a CI failure and not a scientific
refutation.

The stop record must name the first unexecuted width and exact reason.

## Large-width query-utility panel

Task utility is tested only on the mandatory ladder at three frozen widths:

    K = 8,192
        65,536
        1,048,576

The frozen address schedule is:

    address_i = (405 * i + 17) mod K

with query prefixes:

    Q = 128, 512, 2,048

Arms:

### A — P_ZETA_PRECOMPUTE

Clone the source, perform one packed Pascal transform, then answer Q addresses
by transformed-bit reads.

### B — DIRECT_ON_DEMAND

For each query enumerate all submasks of the address and XOR source bits
directly.

### C — COPY_ONLY diagnostic

Copy the same K-bit source bytes and execute the same checksum/query loop
without subset aggregation. C is a memory-traffic lower-bound diagnostic only;
it is NOT a semantically equivalent control and cannot be used to claim
algorithmic superiority.

Physical query panel:

- 4 deterministic source states;
- 2 warmups;
- 12 measured repetitions per equivalent arm A/B;
- rotating A/B execution order;
- C is measured separately;
- exact A/B answer checksum equality is mandatory.

The panel reports exact DIRECT_ON_DEMAND source-read counts for every K/Q cell.

## Frozen gates

### L1 — mandatory exactness

At every mandatory width:

- C0 analytic mismatches = 0;
- C1 self-inverse mismatches = 0;
- C2 sampled-direct mismatches = 0;
- designated K-1 direct mismatch = 0;
- C3 logical-reference mismatches = 0;
- source collisions = 0.

### L2 — accounting exactness

At every executed width, observed width/stages/logical-XOR/state-bytes/scratch
must equal the frozen formulas.

### L3 — mandatory physical sanity

Every mandatory P0/P1 cell must:

- run on the pinned aarch64 runner;
- have exactly 12 measured repetitions;
- have nonzero elapsed times;
- produce stable expected checksums;
- report finite nonzero throughput.

### L4 — K=1,048,576 execution gate

K=1,048,576 must complete C0-C3 and both P0/P1 physical panels. It may not be
silently replaced by a smaller width.

### L5 — one-million-bit task utility

At K=1,048,576 and Q=2,048:

- A/B checksums must match;
- median A must be at least 10% lower than median B:

    10 * median_A <= 9 * median_B

- A must win at least 9 of 12 paired measured repetitions.

A failure of L5 is a valid negative task-utility result and does not invalidate
the kernel-scaling evidence.

### L6 — extension integrity

Every width actually executed above K=1,048,576 must satisfy C0, C1, L2 and its
declared repetition/checksum contract. A preregistered `STOP_RESOURCE` also
satisfies L6 if its reason is correctly recorded.

## Decision

If L1-L4 and L6 hold and L5 holds:

    PB9_DECISION,RETAIN_LARGE_WIDTH_SCALING_AND_1M_UTILITY

If L1-L4 and L6 hold but L5 fails:

    PB9_DECISION,RETAIN_LARGE_WIDTH_SCALING_ONLY

If a mandatory width fails exactness, accounting or physical sanity:

    PB9_DECISION,INVALID

The furthest successfully executed extension width and any resource stop are
reported separately from the decision.

## Interpretation boundary

PB-9 can establish that the current packed Pascal primitive remains exact and
physically tractable at large Boolean widths, and whether one frozen
subset-query workload still benefits at one million bits.

PB-9 does NOT establish:

- model quality;
- general replacement of attention;
- optimal bank width;
- optimal ANF architecture;
- learned usefulness;
- SBG/MOR/Delta-KV integration;
- hardware-independent speedup;
- superiority over every possible fast-zeta implementation.

Any model-side use after PB-9 still requires a separate prospective integration
protocol.
