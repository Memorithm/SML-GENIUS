# Counterpart C: aliasing comparison and record-source integration

Base: `2d21bf9a500ac53bc1f2071e13e16e0b6cfcec6f` (PR121).
Input: newest cumulative `dkv-3a-0.patch`, 127046 bytes / 3094 lines,
SHA-256 `bb08213f7ca4f573d519b089b99da2164623dba8b55153b88cdeb6939f075c9e`.
Extracted new probe: 387 lines, SHA-256
`5069a1cd7bd1f387a6f617e5ea614185a5df1c4bebe7512ff01ddda9c98e8318`.
Extracted legacy adapter: 513 lines, SHA-256
`3125248a991479d21f133acd57c4c6c55701ac70bff74875f5fdc055e20e2393`.
The upload's preregistration chronology and Rust1.97 execution are author
reports, not independently authenticated chronology. Its results are known.
Do not call the following diagnostic blind confirmation or execute the old
unchecked source gate in the production library. Original input remains an
owner-supplied attachment; these hashes do not claim the whole patch is archived.

## What is new and what already exists

C tests whether projected aliases obtain source evidence through the old strict
gate and adds `evidence_for_key_and_record`. It reports one-bit near misses
69/96 at K16 and39/96 at K64 for key-only lookup; its record filter removes
those attributions on its panel. These remain supplied values until evaluated.
The current library already uses full vectors, private verified receipts,
exact SubjectKey history, complete-version binding and consuming bound screening
(PRs99,116,117,121). Do not regress any of those contracts to FNV/Boolean flags.
A record VERSION and a subject with multiple versions are distinct objects.
A record filter must not conceal another version's conflicting observation.

## Prospective adapted comparison, frozen before new execution

Port C's exact xorshift constants, 12-fact generator, 192-position partial
Fisher-Yates near misses, seeds0..7, K16/64 and h1/2/4/8/16/32. Retain the
independent random query but name it `independent`, not mathematically orthogonal.
The source code does not construct orthogonality. C's record-id surrogate is
FNV over V/M/R only, not A: preserve it as a diagnostic, not a trusted identity.

Use the existing native fixed-page5 MOR control from counterpart_b, unmodified
DKV-2a and its existing A2 complementary V[0..48) projection. A one-page engine
proves execution, not useful competition or a worst-case aliasing theorem.
Use current bounded DeltaKvContextMemory, real ledger/SHA verification, 12 writes
of [+4096,+4096], one immutable source `fact <record_id>` per write, no matrix
query. This is a source-attribution experiment, not memory prediction quality.
Reject zero projections explicitly, without replacing keys or choosing seeds.

Per query report: key-only candidate presence; source-written FNV-record match;
new private verified record view; full observed-state index plus record view;
and full-state index-only lookup as a control. The finite index stores all
four complete V/M/A/R fields and admitted record IDs; its storage and scans are
additional, not free and not a learned semantic index. Its result, not a gold
label, supplies record identity. Do not assert complete-state equality is a
universal definition of a semantic subject. Check input duplicates, surrogate
collisions, exact-vs-fingerprint key membership and actual raw-byte work.

The two projections have 2*8*12*8*2 =3072 paired query rows in total. Every row
contains the binding alternatives, not separate independent tasks. Include
written, all six near misses and independent queries. Log rejected writes/queries,
MOR/projector counts, matrix writes, raw verification reads/bytes and key aliases.
There are no learned parameters, dataset splits, held-out scientific tests,
latency/RSS claims or promotions. Source identities do not certify truth,
entailment or external actions. Preserve all original A/A2/B outputs exactly.

Known outcomes are not a quality target: publish all adapted counts, differences
from the supplied tables and costs even when unfavorable. Summary/replay tests
added after execution are engineering regressions, not preregistered confirmation.
Rust1.89 compatibility is measured, never inferred from integer arithmetic.

## Ten bounded engineering slices

1. Freeze this input identity, comparison and API scope.
2. Port controlled C states with deterministic/geometry conformance tests.
3. Add diagnostic full-state identity and legacy fingerprint controls.
4. Add a bounded borrowed record-source view over current VerifiedContext.
5. Test record aliases, absent IDs, budgets and stream scoping.
6. Implement the adapted native-MOR attribution diagnostic.
7. Execute twice, retain exact identities/output and add replay integrity.
8. Test that a record-version view cannot bypass complete subject conflicts.
9. Test fingerprint assumptions, negative controls and rejected inputs separately.
10. Record observations, limits, exact PR/CI evidence and bootstrap/roadmap handoff.

Each PR needs exact-head CI before merge and before the next PR. Use RemoteOps,
a fresh checkout, Rust1.89, two Cargo/test workers, no GPU or service changes.
No protected-evaluation policy, dependencies or existing numeric core is changed.
The record view does not expose Accept or screen; subject-level answers retain
complete VerifiedHistory and BoundProjectedSubject::screen. A caller-supplied
record ID is an explicit identity claim, not self-authenticating authority.
Cross-repository reuse remains conditional on destination qualification; source
identity contracts are relevant to CCOS and diagnostic controls to TDI/SciRust.
No speculative production dependency or claimed external integration is added.

**Context/model promotion remains BLOCKED.**
