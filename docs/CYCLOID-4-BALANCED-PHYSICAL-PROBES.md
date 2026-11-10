# CYCLOID-4 — counterbalanced physical access evidence

Status: executable, read-only Rust measurement primitive. Experimental
host-call observations only; not an end-to-end model speedup.

## Functional change

\`sml_runtime::cycloid_access\` now supports two explicit execution
orders when comparing an **identical, verified** SML weight page:

- \`CycloidProbeOrder::FileThenRam\`: bounded file read/parse/CRC
  followed by copy+CRC of the committed RAM page.
- \`CycloidProbeOrder::RamThenFile\`: committed RAM copy+CRC first,
  followed by bounded file read/parse/CRC.

\`profile_same_mor_page_ordered\` returns both actual observed timings
and the read-order marker. It retains all the previous page selection,
version, representation, resource cap, and exact payload agreement
checks. On Unix it additionally checks that the opened file's
device/inode matches the pre-open non-symlink file metadata.

\`collect_balanced_mor_page_profiles\` runs 3–64 pairs. Every pair
contains **one of each read order**, and the first order alternates
between pairs. All samples either succeed and preserve their own
order evidence or fail closed. Each batch has 6–128 actual paired
observations with corresponding page identity.

This is a **deterministic counterbalancing schedule**, not a randomized
experiment and not evidence of independent run-to-run replicability.
OS filesystem cache, memory hierarchy, CPU scheduling, page faults,
power management, artifact lifecycle and repeated-copy overhead remain
experimental variables. A caller must keep the exact file, hardware,
runner, epoch, commit and run provenance.

The existing \`profile_same_mor_page\` public method remains unchanged
as a FileThenRam compatibility wrapper. Existing CYCLOID-3
\`FrozenCycloidAccess::freeze\` accepts the batch's
\`profile_values()\` for offline path construction, but **it cannot
infer that the samples came from a trustworthy physical benchmark**.
To promote a path, measure lookup/dispatch costs independently and
compare *complete* model workloads against the direct control.

## Regression qualification

- The two physical read orders produce identical payload identity.
- Three pairs yield exactly three observations per order, with
  explicit order provenance and all bytes verified.
- Too few or too many pairs fail before any source read.
- Original corruption and MOR-eligibility regression tests remain
  mandatory through the full Rust workspace CI.

No RemoteOps runner or real model workload is used by the unit tests.
The next hardware experiment should run **only from a CI-green,
reviewed main SHA** on the Dell T430 via Memorithm/RemoteOps, with
bounded isolated resource use and explicit operator-approved artifact
sources. Its output should retain medians, p95, page hashes, both
read orders, CPU/cache conditions, and actual model-task parity.
