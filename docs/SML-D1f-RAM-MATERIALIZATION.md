# SML-D1f-a: verified disk-to-RAM Elastic materialization

Implementation: `crates/sml-runtime`.

D1f-a is the first SML slice that physically reads encoded weight-page payloads
from files into process RAM and aligns that staged payload with an existing
`ElasticWeightPlan`.

## Transaction

The supported sequence is:

`PLAN -> artifact preflight -> MATERIALIZE -> VERIFY -> COMMIT`

or:

`PLAN -> artifact preflight -> MATERIALIZE -> VERIFY -> ROLLBACK/DISCARD`.

Staging and verification do not mutate the Elastic planner or committed RAM
cache. Commit first applies the already-verified native Elastic plan and then
replaces the runtime RAM cache with the staged page set. If native planner commit
fails, the committed RAM cache is left unchanged.

## Weight artifact v1

Each artifact is self-describing and includes:

- magic/version/header size;
- page ID;
- precision class;
- representation version;
- logical parameter count;
- encoded payload length;
- CRC32C for payload and header.

Payload length is derived from parameters and bits-per-parameter. Exact file
length is checked from filesystem metadata before `fs::read`, so a catalog
entry cannot silently cause an arbitrarily large file read.

CRC32C is corruption detection only. It is not cryptographic authenticity,
provenance, encryption, or content signing.

## Supported physical boundary

D1f-a supports pages whose Elastic target is **RAM** and whose precision is
unchanged from the current representation.

It deliberately rejects:

- every active VRAM target;
- every precision transition, including Boolean -> Ternary/Residual4.

The runtime therefore never emulates VRAM with host memory and never invents
richer learned values when the required target representation is unavailable.

A later slice must supply a genuine VRAM backend and a separately qualified
representation materializer before those transitions can be committed.

## RAM reuse and replacement

If a selected page is already committed in RAM with exactly matching page ID,
representation version, precision, parameter count, length and CRC32C, staging
reuses its bytes and performs no disk read.

When the active set changes, the next staged set contains only target active RAM
pages. After commit, pages no longer active are removed from the runtime RAM
cache while the native planner returns them to Disk residency.

## Budgets

The materializer bounds:

- files read;
- total staged payload bytes;
- total artifact bytes read.

These are payload/file-transfer limits, not allocator-overhead or whole-process
RSS guarantees. The Elastic planner remains authoritative for RAM bytes and
active-parameter limits.

## Scientific/runtime boundary

D1f-a is real disk-to-process-RAM movement, but still not the complete 25B
runtime. It does not provide:

- GPU/VRAM allocation;
- async prefetch;
- direct I/O;
- cryptographic artefact authentication;
- concurrent cache eviction;
- precision conversion;
- crash-durable transaction journaling.

Those capabilities must be qualified independently rather than inferred from
the reference RAM materializer.
