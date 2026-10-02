# DKV-3a-0 patch consolidation and hardening

This slice treats the two owner-supplied cumulative patches named `dkv-3a-0.patch`.
It is engineering conformance, not a new scientific preregistration, a qualified
context model, or an implementation of SBG/MOR-projected recurrence.

## Source identity and scope

| input | bytes | SHA-256 |
| --- | ---: | --- |
| first upload | 51180 | `df99afa527331be47206b68e034a38e3b983d0c41bced7886fba94a6d696d3c6` |
| second upload | 75271 | `fa90d6a28485dfa11b5aa7a4efe91008c344635e8cd5afd64a172fd2d914b466` |

The second patch subsumes the first; they must not be sequentially applied.
Their identical 460-line core wrapper is NOT imported unchanged. Historical
numbers and their limitations are retained in
[SML-DKV-3A-0-LEGACY-OBSERVATIONS.md](SML-DKV-3A-0-LEGACY-OBSERVATIONS.md).
The uploads contain declarations of preregistration and merge status, but no
independently verifiable chronology binding those declarations to execution.
They are not converted into confirmed preregistrations or merge evidence here.

## Ownership and API migration

The unchanged Q12 numeric rule remains `sml_core::delta_kv::DeltaKvMemory`.
The corrected source/evidence adapter is **`sml_store::delta_kv_context`**, not
`sml_core::delta_kv_context`: SHA-256 and raw-byte verification belong next to
CONTEXT storage, without adding I/O or dependencies to the Boolean core.

`DeltaKvContextMemory::new(stream, key_dim, value_dim, limits)` requires explicit
limits. Writes validate the complete semantic-record/source relation and pin
full immutable ledger descriptors, including content hashes and forward links.
The ledger may only extend the pinned prefix. Query receipts require the exact
current pinned generation; a stale/forked/foreign ledger rejects. Ingest must
refresh admitted memory state before querying a newer ledger.

`query(full_key)` uses a sorted exact-key index and returns a nonempty borrowed
`QueryEvidence`. There is no fingerprint-only membership, no all-history query
scan and no forced value-derived hit. The target FNV fingerprint is diagnostic
only. Complexity is bounded O(key_width * log(write_count)) for key selection,
plus the admitted matching-source work; this is not a claim of O(1) lookup.

`QueryEvidence::verify(ledger, limits, reader)` validates all source/byte budgets
before reading, rereads every unique cited source, and independently checks
length and SHA-256. Only this path constructs private `VerifiedContext` fields.
A receipt is tied to the query and ledger by immutable borrows. It cannot be
minted from `Verified { segments: [] }`, a Boolean, unrelated raw bytes or a
score. All matching historical versions are retained and deduplicated by
segment identity; a latest-only entry cannot silently discard a contradiction.

The reader is a trusted bounded I/O adapter, not a security sandbox. Its own
allocations/side effects must be admitted independently. SHA-256 verifies byte
identity against the admitted ledger, not source authenticity or truth.

There is deliberately **no public automatic Accept/DerivedSemanticOnly gate**.
A receipt certifies byte identity and write provenance, not that a target was
correctly derived, not entailment, confidence, or permission for an external
action. Task-specific resolution must independently inspect verified bytes.
Derived Q12 reads stay available for future separately qualified experiments.

## Bounded storage and transaction contract

Limits cover writes, sources per write, total source links, ledger descriptors,
ledger semantic links, retained element payload and staged-write element payload.
Exhaustion rejects; no source is silently evicted. The conservative ElementPlan
includes matrix coefficients, record/index elements, full keys, source IDs,
ledger descriptors/links and the adapter object. Staging includes the copied
matrix, new ledger snapshot, prepared record and prediction payload.

These are bounded logical ELEMENT payloads, not Vec spare capacity, allocator
bookkeeping, process RSS, page cache, physical storage or caller-retained cloned
objects. Verification has separate source/raw-byte admission. Matrix and ledger
copies are retained as reference semantics and counted, not claimed efficient.
Allocation aborts and process crashes are outside Result-level atomicity; there
is no crash-durable sidecar in this slice. Ordinary returned errors do not
commit matrix, counters, evidence or ledger state.

Ingestion prediction MACs and query prediction MACs are separate. The underlying
numeric `read_macs` includes BOTH. Coefficient copies, ledger-descriptor copies
and shifted index entries are also exposed. No logical count is wall-clock or
RAM evidence.

## Corrected engineering fixtures

The conformance path parses typed raw records:

```text
record_id<TAB>topic<TAB>two_bits<TAB>exact_value<TAB>superseded_record_id_or_dash
```

This is a bounded synthetic fixture grammar, not natural-language understanding.
The resolver reads only receipt bytes, checks the question topic and semantic
record identity, retains conflicting active records and accepts supersession
only as an explicit reference to an earlier active record. Expected answers are
used only AFTER resolution in assertions. Numeric/code checks compare the
entire extracted field, never `contains()`.

Tests cover empty/partial/duplicate/foreign provenance, unreadable or altered
source bytes, stale/forked contexts, exact-key and shape rejection, ordinary
update rollback, source/write/element budgets, incremental ledger generations,
all-version retention/deduplication, typed contradiction/supersession, exact
fields, cost separation and a real serialized CONTEXT artifact roundtrip.
A compile-fail doc test prevents external receipt construction.

Geometry conformance separately preserves counterexamples to the uploaded
harness. Constant-magnitude keys and exactly five distinct, non-antipodal
mask-derived distractors allow exact rational cosine checks across dimensions.
A genuine orthogonal control has dot product exactly zero. These are algebraic
construction tests, NOT a new overlap/recency performance sweep. The old
five-distractor/four-block construction degenerating into two keys is retained
as a regression counterexample. Sign correctness and exact Q12 recovery are
separate: `[4096,4096,4096] -> [4096]` can read 4095 without saturation.

## Promotion is separate from software validity

`PromotionEvidence::decision()` reports Blocked when any evidence requirement is
false or missing. EligibleForIndependentReview is only an evidence summary, not
an authenticated grant, merge permission or runtime action. A fully passing CI
still leaves projected SBG/MOR recurrence unqualified. No successful software
test promotes a memory mechanism.

The original DKV-3a scientific contract remains unchanged. No final/protected
holdout, training campaign, DKV-2c promotion or RSI promotion is performed.

## Handoff reserved for the next owner-supplied tranche

The next independent document must preregister Delta-KV as a bounded SBG
recurrent input using the DKV-2a projector and actual MOR admission. It needs
frozen projected keys per episode, direct-memory/no-DKV controls with matched
information, depth/resource limits, recency/interference controls, independent
raw provenance and complete quality/cost evidence. This correction DOES NOT
wire those mechanisms or fabricate that preregistration.

Before that campaign, freeze a new experimental protocol; do not overwrite old
results with corrected fixtures. End-to-end persistence/crash recovery, actual
memory/time measurements, representative language tasks and learned-key quality
remain open.

Remote execution belongs to Memorithm/RemoteOps. Cross-project reuse candidates
are the tested exact-key/source binding contract (CCOS-Enterprise), fail-closed
resource accounting (ElasticXxx), and conformance controls (TDI/SciRust/Forge).
Destination qualification must precede copying code; this slice introduces no
cross-repository inference dependency and claims no completed external reuse.
