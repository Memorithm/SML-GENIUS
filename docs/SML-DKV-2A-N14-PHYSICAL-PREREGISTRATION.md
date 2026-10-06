# DKV-2a-N14 — physical resource qualification preregistration

Status: frozen before implementation or execution.

Parent evidence:

- `SML-DKV-2A-N13B-PAGE-SKEW-OBSERVATION.md`;
- `SML-DKV-2A-N12-VERIFIED-IDENTITY-OBSERVATION.md`;
- `SML-DKV-2A-N11-TWO-BANK-OBSERVATION.md`.

## Purpose

N13B retained the q=3 fixed-width PAGE/PAGE_VALUE composition with the N11
PRIMARY/ORDERED two-bank resolver and N12 exact source/record/subject preflight
under balanced, empirically skewed and independently heavy-skewed page
populations.

N14 measures the physical cost of that retained Development composition before
any PageSeeded/two-bank core-API promotion.

This is a systems qualification, not a model-quality experiment. It must not
reinterpret logical operation counts as latency, RSS, physical I/O or energy.

## Frozen candidate

The candidate is unchanged:

- K=1024;
- q=3: 3/4 PAGE_VALUE + 1/4 PAGE;
- ActiveOnly ambiguity projection;
- PRIMARY_ANCHORED outer domain for PRIMARY;
- ORDERED_ROUTE outer domain for explicit overrides;
- exact record/ordered-route directory;
- stored write locator;
- VerifiedContext raw verification;
- RecordRequest binding;
- complete VerifiedHistory;
- bind_projected_subject + candidate screening;
- Missing/Conflict/Unverified remain zero-read fail-closed outcomes;
- exactly one selected-bank matrix read after VerifiedPresent preflight.

No projector, matrix rule, provenance classification, sharing rule or threshold
may change inside N14.

## Frozen fixtures

Reuse the exact N13B population vectors:

- BALANCED = `[15,15,15,15,14,14,14,14,14,14]`;
- N13A_EMPIRICAL = `[2,4,53,2,4,6,2,9,62,0]`;
- HEAVY_SKEW = `[116,4,3,3,3,3,3,3,3,3]`.

Every population contains 144 records. Use deterministic seeds 0..7 for the
physical panel.

The K=256 arm is retained only as a width/resource control. K=1024 is the
candidate. Both use identical fixture identities, q=3 composition, routes and
source-verification semantics.

## Workload arms

For each population/seed/width, construct independent worlds.

### W1 — PRIMARY continuity

- write every record to PRIMARY under BASE(page);
- query every record under SECONDARY(page);
- select PRIMARY for every query.

This isolates the physical cost of verified shared retrieval.

### W2 — MIXED two-bank resolver

- write every record to PRIMARY under BASE(page);
- every fourth canonical record receives a complementary ORDERED override under
  SECONDARY(page);
- query every record under SECONDARY(page);
- bound records select ORDERED; unbound records select PRIMARY.

This is the retained two-bank composition with a deterministic 25% override
rate.

### W3 — direct matrix diagnostic

Use the same already-built numeric bank and projected query keys, but issue the
matrix read without source/record/subject verification.

This arm is a counterfactual cost decomposition only. It is not semantically
equivalent to W1/W2 and can never become the promoted path.

## Warmup and sampling

For W1/W2:

1. build the complete world;
2. execute two unmeasured full query traversals;
3. execute eight measured full traversals;
4. deterministic query order is fixed per seed and reused across widths/arms.

The measured sample count per population/seed/arm is therefore 1,152 resolved
queries. Across eight seeds there are 9,216 samples per population/arm/width.

W3 uses the same number and order of matrix reads.

No cache-dropping, governor change, CPU pinning, privileged perf event or root
operation is permitted by the harness. Those would create a different
experiment.

## Physical measurements

The Rust harness must use the operating-system observations available to the
unprivileged process and report unavailable metrics explicitly.

### Latency

Use `std::time::Instant` around each measured operation.

Report, in integer nanoseconds:

- sample count;
- min;
- p50;
- p95;
- p99;
- max;
- total measured wall time.

Quantiles use the sorted sample array and a frozen nearest-rank integer rule.
No subtraction of timer overhead is allowed.

### Resident memory

On Linux parse `/proc/self/status` and report:

- VmRSS after build;
- VmHWM after build;
- VmRSS after warmup;
- VmHWM after warmup;
- VmRSS after measured workload;
- VmHWM after measured workload.

Values are reported in bytes. VmHWM is a process high-water mark, not isolated
allocation attribution to Delta-KV.

### Process I/O and page faults

On Linux record pre/post snapshots of:

- `/proc/self/io`: rchar, wchar, syscr, syscw, read_bytes, write_bytes,
  cancelled_write_bytes where present;
- `/proc/self/stat`: minor and major page faults.

Report deltas for the measured workload.

These are process/kernel counters. They are not DRAM bus-byte measurements.
A zero `read_bytes` value only means the kernel did not account block-device
reads to the process during the measured interval.

### CPU runtime

On Linux record the first field of `/proc/self/schedstat` when available and
report the measured CPU-runtime delta in nanoseconds. If unavailable, report
`UNAVAILABLE`.

### Energy

Energy is valid only when a monotonic kernel energy counter is readable by the
unprivileged process.

The harness may inspect direct top-level entries below `/sys/class/powercap`
and read their `energy_uj`, `max_energy_range_uj` and `name` files.

For each readable top-level energy domain:

- record before and after values around the measured workload;
- account one counter wrap using `max_energy_range_uj`;
- report the per-domain delta in microjoules.

Do not sum nested/overlapping domains and do not infer joules from an
instantaneous power sensor.

If no valid monotonic energy counter is exposed, emit
`ENERGY_STATUS,UNAVAILABLE`. In that case N14 may retain latency/RSS/I/O
evidence, but the roadmap's full physical-resource gate remains incomplete.

## Hardware/runtime identity

Retained evidence must bind at least:

- exact Git commit;
- Rust version;
- OS/kernel;
- architecture;
- GitHub Actions run/job;
- runner name from the Actions API;
- logical CPU count visible to the process;
- CPU model string when readable;
- `Cpus_allowed_list` when readable;
- CPU frequency governor when readable.

Do not record machine serial numbers, MAC addresses or other unnecessary host
identifiers.

## Correctness invariants

Physical measurement is invalid if functional semantics change.

For every measured W1/W2 world:

- all source-required queries must classify VerifiedPresent;
- every query performs exactly one selected-bank matrix read;
- W1 always selects PRIMARY;
- W2 bound records select ORDERED and unbound records select PRIMARY;
- measured query correctness must equal the corresponding unmeasured warmup
  correctness;
- no timing/RSS/I/O/energy value may alter bank selection, provenance or answer
  interpretation.

## Frozen hypotheses

H1 — semantics preserved: all correctness/provenance/bank/read-count invariants
hold at both K=256 and K=1024.

H2 — instrumentation complete: latency, VmRSS/VmHWM, process-I/O, page-fault and
CPU-runtime measurements are available on the retained Linux run.

H3 — hot in-memory resolution performs zero block-device reads and writes:
`read_bytes == 0` and `write_bytes == 0` during the measured W1/W2 interval.
A failure is retained; it is not hidden by rerunning until zero.

H4 — after two warmup traversals, the measured W1/W2 interval incurs zero major
page faults. Minor faults are reported without a pass threshold.

H5 — two-bank single-read invariant: W2 performs exactly one matrix read per
resolved query and never performs both PRIMARY and ORDERED reads.

H6 — width scaling is measured, not assumed: report K1024/K256 p50, p95, p99,
CPU-runtime, RSS and matrix-storage ratios for W1/W2. No numerical speedup or
linear-scaling threshold is preregistered.

H7 — verification cost is measured, not erased: report W1/W3 and W2/W3 latency
ratios. W3 cannot satisfy source-safe semantics and is not eligible for
promotion regardless of speed.

H8 — energy availability: at least one valid top-level monotonic energy counter
is readable and yields a non-negative wrapped delta for every measured W1/W2
batch.

## Decision rule

If H1-H7 hold, retain N14 as valid physical latency/RSS/I/O evidence for the
current runner and exact candidate.

If H8 also holds, the current runner supplies complete N14
latency/RSS/I/O/energy evidence and the roadmap physical-resource checkbox may
be closed for that measured host.

If H8 is refuted only because the runner exposes no valid energy counter,
retain the other measurements but leave the roadmap physical-resource checkbox
open. A separately pinned energy-capable runner may complete the same frozen
protocol without changing candidate semantics or thresholds.

Any H1 failure invalidates the run. H2-H7 failures remain negative systems
evidence; do not tune the workload or thresholds after observation.

## Boundary

N14 does not establish:

- model-quality superiority;
- representative future 25B traffic;
- disk-backed raw-context deployment cost;
- DRAM bus traffic;
- cold-cache latency after privileged cache eviction;
- GPU/VRAM behavior;
- energy on hosts that expose no monotonic energy counter.

Core API promotion remains a separate decision after the frozen physical gate
and current scientific boundaries are reviewed.
