# CYCLOID-8 — physical host-call measurement of zero-copy borrow

**Status:** opt-in synthetic-page benchmark extension. Requires the
CYCLOID-7 verified resident-page borrow interface. No default model
routing is changed.

The existing \`sml-cycloid-physical-probe\` binary now measures the
**actual new operation** \`FrozenCycloidAccess::borrow_committed_resident\`
against the existing \`FrozenCycloidAccess::read\` **on the same
committed SML RAM page and same frozen plan**.

The previous CYCLOID-5 figures (\`resident_*_ns\`) measure a payload
copy and CRC. They are not a measurement of the zero-copy API and
must not be used as its cost estimates.

## Bounded experiment

- Build one deterministic synthetic Boolean weight artifact of
  64–1024 KiB; stage and commit it in the real SML runtime.
- Independently validate its file and RAM payload byte-for-byte and
  compare the borrowed slice's pointer with the committed payload's
  pointer.
- **For the sole purpose of selecting the indirect RAM branch**,
  construct five *explicitly synthetic* cost observations. These
  values are not measured and must never enter any physical
  performance analysis. CYCLOID-3 additionally re-verifies actual
  file and RAM byte equality when freezing.
- Run 3–16 pairs, with two trial batches per pair in opposite
  orders: borrow then copy+CRC, and copy+CRC then borrow. Each
  timed batch calls the operation 128 times, using
  \`std::hint::black_box\` to retain the result, and measures
  total nanoseconds with \`Instant\`.
- Record the actual duration distributions separately for
  \`borrow_batch\`, \`copy_crc_batch\`, and borrowed batches
  when called first versus second. A short JSON record contains
  the batch length, number of observations and an explicit
  \`borrow_mode=synthetic_selection_fixture_only\` provenance flag.
- The original schema and CYCLOID-5 disk/RAM read-order measures
  remain intact for compatibility with the existing RemoteOps
  workflow.

## Interpreting results

The no-copy operation verifies frozen page identity, MOR eligibility
and planner generation, and returns an immutable Rust slice. The
copying comparison also allocates, reads/copies and validates CRC.
Those are **intentionally different work envelopes**; differences
are not DRAM bus telemetry or general inference speedup.

The 128-call batch duration is **not per-call nanoseconds**; the
quantiles must be reported as a *batch* measurement. No source
payload is consumed by an SML boolean neuron or end-to-end model
decode during this probe. The page is synthetic, caches are neither
flushed nor isolated, and AB/BA counterbalancing is deterministic
rather than randomized. Compiler optimization is constrained by
\`black_box\` but CPU frequency, page cache, memory allocation,
clock granularity and host contention still matter.

The next pre-production gate is a paired RemoteOps campaign on a
reviewed SML main SHA, followed by actual model execution using
this borrowed slice rather than an extra temporary buffer. Only
repeated end-to-end workload and correctness parity can justify
claims of improved model latency.
