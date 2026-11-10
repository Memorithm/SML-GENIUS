# CYCLOID-7 — zero-copy access to a frozen verified SML resident page

**Status:** experimental native Rust runtime API. No default MOR/inference
change, physical speedup claim, new GPU dependency, or cross-repository code.

## Functional change

\`FrozenCycloidAccess::borrow_committed_resident\` returns a **borrowed
\`&[u8]\` slice** of the *actual committed* RAM-resident page when
CYCLOID-3's frozen plan has chosen \`VerifiedResident\`.

Unlike \`FrozenCycloidAccess::read\`, this method **does not**
allocate a new \`Vec\`, copy the full payload or recompute CRC32C on
every access. The caller sees the same Rust allocation as
\`ElasticRamRuntime::ram_page(page_id).payload()\`, but only after the
frozen access has been checked.

The method enforces:

- Explicit \`VerifiedResident\` mode, never pretending a file read
  can be borrowed without loading it.
- Matching frozen contract and *current* MOR-selected PageId.
- Exact committed planner generation and original weight-page header,
  including precision, representation version and CRC field.
- Exact actual payload length against the versioned header.
- Immutable Rust borrow: the returned slice is tied to
  \`&ElasticRamRuntime\` and cannot outlive that runtime. Because
  \`ElasticRamRuntime::commit\` requires \`&mut self\`, committing new
  memory pages while the borrowed slice remains live is rejected by
  the borrow checker (for normal safe Rust clients).

**Checksum boundary:** both materialization
(\`ElasticRamRuntime::verify_stage\` before \`commit\`) and
\`FrozenCycloidAccess::freeze\` validate the payload CRC/content.
\`RamWeightPage.payload\` is private; safe callers can obtain only
shared immutable slices. For this reason, *recomputing* CRC on every
borrow is redundant in this checked lifecycle. CRC32C remains only
accidental corruption detection, not publisher authentication or
protection against malicious hardware memory corruption.

## Correctness regression

- A frozen indirect RAM path returns exactly the original weight bytes
  and **pointer identity** with the committed memory (no \`Vec\` copy),
  including across repeated borrows.
- A frozen direct-file route cannot call the resident-borrow operation.
  MOR deselection also refuses borrowed access.
- A subsequent commit changes the RAM planner generation and revokes
  future borrows using the old frozen selection.

The existing \`read\` method and byte-verifying disk/CRC diagnostic
remain available and unchanged.

## Benchmark and inference boundaries

**No speedup is established by this implementation alone.** It removes
an explicit allocation and payload copy/CRC from this *one optional
code path*, not all CPU-cache, memory-bus or model work. It is not
wired into default inference.

**Important calibration distinction:** CYCLOID-2/4/5 profile **copy +
CRC** of a resident page. The new borrow has different measured work,
so the prior timing estimates must **not** be silently relabeled as
zero-copy host-access latency. A future pinned, paired RemoteOps probe
must measure this exact borrowed access and its dispatch/memory
ownership boundaries independently, accounting for complete inference
work and retained task quality before promotion.

No vendor SDK or CUDA/NVIDIA integration is introduced.
