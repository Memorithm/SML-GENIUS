# CYCLOID-SML-2 — same-page disk/RAM calibration instrument

Status: functional native Rust implementation in \`sml-runtime\`.
This is NOT a model-level routing-speed result or a neural/geometry claim.

## Motivation

CYCLOID-1 plans bounded access paths to the *same* page already selected
by MOR. Caller-supplied edge weights alone cannot establish that a longer
path is physically faster. A real plausible source of savings is a
verified RAM-resident page versus reading its file representation again.

The new \`sml_runtime::cycloid_access::profile_same_mor_page\` executes
and independently verifies both access modes. This is an isolated
measurement/calibration primitive; it does not modify the active set,
MOR decisions, page residency, filesystem content, or inference.

## Exact execution

- Require that the requested page is currently MOR-selected.
- Require a matching page already committed in \`ElasticRamRuntime\`.
- Compare page ID, precision, version and payload length against a
  caller-authorized \`WeightArtifactEntry\`.
- Enforce a 64 MiB absolute upper bound and a smaller caller limit
  **before** any file read.
- Reject symlinks and non-regular files; use bounded read to avoid unbounded
  allocation if the file changes between stat and open.
- Parse and check CRC32C of the complete versioned disk artifact and
  independently CRC-check the copied resident RAM payload.
- Require byte-exact disk/RAM payload equality and identical headers.
- Return the monotonic host time for the bounded file-read+parse+CRC
  operation and for the RAM-copy+CRC operation, including byte-count
  accounting. No telemetry is emitted on errors.

These two timing envelopes are distinct by design. They are NOT normalized
DRAM transaction counts, cold-disk latency, block-I/O bytes, cache misses,
page-fault observations, or a controlled AB/BA benchmark. Filesystem cache,
OS scheduling, verification overhead and copy effects must be reported.
CRC32C is corruption detection, **not source authentication**.

## Required next steps

1. Qualify this code on the exact PR head through fmt, Clippy, full Rust
   workspace tests, and all required CI. The current tests execute a
   real temporary SMLWGT01 disk file and a committed RAM artifact; they
   cover byte equality, identity/budget rejection and corruption rejection.
2. On a trusted Dell T430 runner, invoke an audited read-only measurement
   through Memorithm/RemoteOps. Pin source SHA, hardware, file identity,
   CPU affinity/resource envelope, residency and page-set identity.
   Use paired randomized AB/BA order, warm and cold-cache declarations,
   multiple repetitions and medians/p95. Do NOT run unreviewed PR
   code on privileged production runners.
3. Only after real, reproducible evidence, create a versioned frozen
   access-cost profile consumed by CYCLOID-1. Report calibration/dispatch
   cost separately. Still require a separately qualified executable
   indirect path and integrated SML model task-quality parity.

The development does not add a Transformer KV cache or FLAT dependency.
