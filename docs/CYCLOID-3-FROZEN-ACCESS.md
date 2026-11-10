# CYCLOID-3 — frozen costed access to SML weight pages

**Status:** opt-in Rust runtime implementation. No default SML routing
change, no model-quality or physical-speed claim, no FLAT dependency.

## Executable connection between CYCLOID-1 and CYCLOID-2

The code in \`crates/sml-runtime/src/cycloid_dispatch.rs\` connects two
already merged and independently tested modules:

- CYCLOID-1 \`sml_core::cycloid_route\`: offline bounded search of
  control/relay paths to an immutable MOR-selected page.
- CYCLOID-2 \`sml_runtime::cycloid_access\`: verified, byte-exact comparison
  of the same versioned SMLWGT01 page loaded from file and from committed RAM.

\`FrozenCycloidAccess::freeze\` accepts 5–256 profile observations for
a *single exact page/version/precision/length*, along with the caller's
explicit measured/estimated relay lookup and dispatch costs. The code
rejects inconsistent or incomplete observations. It computes
deterministic median and p95 evidence, re-verifies actual file/RAM byte
equality, creates a three-node Cycloid access graph and runs the
bounded planner **once**.

The two exact access modes modeled are:

- **Direct:** entry → page, with the file-read/verification median,
  including the versioned artifact header in logical bytes read.
- **Indirect:** entry → RAM lookup relay → same page, with an explicit
  positive caller-supplied lookup cost plus resident-copy/CRC median.
  The planner additionally accounts for caller-supplied dispatch cost.

A longer path can be preferred only if its total declared cost is
**strictly lower** under the same budget. No arbitrary "speed field",
curvature, neural shortcut, or invented 1 ns transition is assumed.

The optional \`FrozenCycloidAccess::read\` runs **no graph search**. It
checks the current MOR selection, unchanged committed RAM generation,
page header, representation version and CRC, then performs the
specified actual file or resident RAM read. A changed/wrong version,
corrupted file, stale runtime, or newly deselected expert fails closed.

These are actual **source byte reads**, not yet the execution of a
Boolean neural page or a production model decode. The implementation
does not change the canonical MOR decision, page residency, model state,
or memory ownership. Its result carries payload and logical file/RAM
copy counters plus the host duration of this one access.

## Limitations that prevent premature performance claims

1. **Calibration is advisory.** Per-sample timing values and the two
   additional route/dispatch costs are supplied by a caller. Only
   byte identity is reverified; measurements are not cryptographically
   attested by a host. The unit tests use explicit *synthetic* timings
   while executing real file/RAM reads, so they prove behavior rather
   than any speedup.
2. **Read semantics differ.** File-access timing includes filesystem
   read and full artifact parsing; resident access times a payload
   copy/CRC. Lookup, allocation, buffer residency and OS page cache can
   materially influence the comparison. A frozen graph does not imply
   an end-to-end advantage.
3. **Logical bytes are not DRAM transactions.** Counter values
   reflect explicit file bytes read or resident bytes copied only.
   No claim about physical I/O, cache misses, CPU cycles, page faults,
   energy or global model memory usage follows from those counters.
4. **CRC is not authentication.** CRC32C is used for accidental
   corruption detection, not authenticating a malicious publisher. A
   caller must supply an independently authorized artifact/path. On
   Unix, the opened file identity is checked against the path's
   pre-open inode/device metadata; concurrent in-place mutation and
   adversarial CRC collisions are not covered.
5. **Insufficiently qualified cost profiles cannot be promoted.**
   Minimum sample count and deterministic medians are software
   validity checks, not statistical uncertainty bounds or a measured
   confidence interval. Repeated independent randomized AB/BA runs,
   cold/warm-cache labels, measured lookup/dispatch overhead, pinned
   workload/host and held-out model-quality parity are still required.

## Evidence and qualification

Unit tests execute an actual, committed RAM page and a real temporary
SMLWGT01 file under explicit synthetic cost controls:

- A longer relay/RAM path is frozen and actually returns original bytes.
- A cheaper direct-file path wins and detects file payload corruption.
- Forged/non-matching evidence and MOR reselection fail closed.
- A later RAM-planner generation invalidates the frozen table.
- Missing samples and zero/unknown lookup cost are rejected.

Run exact-head CI gates before merge:
\`cargo fmt --all -- --check\`;
\`cargo clippy --locked --workspace --all-targets -- -D warnings\`;
\`cargo test --locked --workspace\`.

The next **physical** qualification should be a read-only,
resource-bounded and authorized RemoteOps experiment on Dell T430,
with pinned SHA, pinned same-page identity, AB/BA randomized access
order and p50/p95 end-to-end latency including freeze amortization.
Only after model-quality and whole-workload gains are demonstrated may
a default routing change be proposed.

Reference: [CYCLOID-1](CYCLOID-1-COSTED-ACCESS-ROUTING.md),
[CYCLOID-2](CYCLOID-2-PHYSICAL-PAGE-ACCESS.md).
