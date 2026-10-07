# PVP6 CPU activation-plus-query timing protocol v1

Date: 2026-10-07. Prospective diagnostic protocol, frozen before task timing.
Fixture: [PVP6 ANF v1](SML-PVP6-ANF-FIXTURE-CONTRACT.md), SHA-256
`feb1a144e23d9a29439b7cad6704f56637a7f7ead1a9b9abb39ad9c0ac5325fd`.
This is synthetic PB8 reference work, not trained-model architecture selection.

## Endpoint

The new `pvp6-task-benchmark` mode times exactly the call to `task_words`:
input-contract validation, output allocation/initialization, per-gate Pascal or
generic materialization where applicable, per-gate temporary destruction, all
Q*R queries, and packing 16 gate values into each returned u64.
A compiler barrier observes the returned vector before stopping the clock.
Fixture generation, monomial-list preparation, the initial resident truth,
schedule generation, oracle computation, full output verification, CSV printing
and returned-output destruction are outside the timer.
No checksum loop substitutes for the declared full-output endpoint.
R means unchanged-coefficient reuse; update/rebuild workloads are not measured.

All five arms return the same allocated Q*R-word output:
Pascal materialization, sparse monomial scan, submask coefficient query,
generic materialization, resident lookup. Resident lookup has prepaid activation
and is reported separately; it is not a primary activation-plus-query control.
The original PB8 benchmark remains unchanged.

## Frozen 20 unique cells

K=2048, G=16, original fixture generator. Enumeration order is:

1. Densities 4,32,256,1024: AFFINE_UNIFORM, Q=512, R=1.
2. Density 256: AFFINE_UNIFORM; Q=128,512,2048 then R=1,4,16.
   The Q=512/R=1 cell already in group 1 is included only once.
3. Density 256: LOW_POPCOUNT then HIGH_POPCOUNT; Q=128,512, R=1.
4. Density 4: all three schedules; Q=32,R=1.
5. Density 1024: AFFINE_UNIFORM,Q=2048,R=16.

`pvp6-task-plan` emits this exact enumeration without timing.
Five warmups and twenty measured repetitions per arm/cell/process.
Ten sequential fresh processes: IDs 1..10, no replacement of failed processes.
Each process has 500 warmup and 2000 measured samples, plus 100 summaries.
Total planned campaign: 5000 warmup and 20000 measured samples.
For repetition r, offset=(process-1+cell+r%5)%5; direction alternates
between forward/reverse each block of five repetitions.
Every arm appears at every position four times among measured repetitions.
Cell order is fixed; this does not establish absence of cache or thermal effects.

## Provenance, execution and resources

Require PVP6_PROCESS_ID=1..10 and PVP6_SOURCE_REV=40 lowercase hex digits.
These environment labels alone do not attest the source: RemoteOps must verify
a clean checkout at that revision and record compiler, executable SHA-256 and
control revision before running it.
Use native Rust 1.89.0 release, no target-cpu=native override.
The first campaign targets Thor ARM64 CPU only, with shared-host diagnostic
admission. GPU/SIMD and Dell are separate future cohorts.
RemoteOps must enforce at most 1 GiB and 100% CPU per measured process,
120 seconds maximum per process, sequential execution and an aggregate
30-minute campaign limit. Do not stop existing services.
Use an unprivileged supervised process; record the concrete enforced properties
without upgrading them to a hostile-code sandbox or exclusive-host admission.
Record exit status, cgroup settings and accounting, raw stdout/stderr, host
identity and load observation for every process, including failures.
Do not run task timing until source/protocol are committed and the RemoteOps
control is reviewed and qualified. Compilation and untimed correctness checks
may precede the campaign.

## Verification and analysis

Verify every warmup and measured full vector against direct monomial evaluation
outside the timer. Emit raw sample metadata before failing on mismatch or a
non-positive elapsed time. Preserve the incomplete process and mark the campaign
invalid; never silently replace it. A complete end marker is mandatory.

Raw sample keys are process/cell/warmup/repetition/arm; require exact unique coverage,
declared order, positive nanoseconds, Q*R words, zero mismatches and exact=1.
Each summary uses the arithmetic median of twenty samples and nearest-rank p95
(rank ceil(0.95*N), one based). Independently recompute summaries from raw rows.
For descriptive comparisons keep each process separate, use the competent
sparse/submask controls and report neutral/slower cases, not only generic wins.
Resident lookup is a separate lower-cost lifecycle reference.
Do not pool host cohorts or claim a p-value, confirmed gain, default route,
autotuning, learned utility, model quality or tokens/s from this diagnostic.

## Qualification gates

Tests cover cell uniqueness and retained sparse-low-query/dense-reuse regimes,
balanced arm positions, complete output equality, length/padding corruption and
coefficient-change invalidation. CI regenerates the original fixture/hash and
the timing plan twice; it does not select architecture using hosted timing.
SML owns semantic and performance interpretation. RemoteOps owns host enforcement.
SciRust/FLAT/NNIS consume this contract only after separate native qualification.
