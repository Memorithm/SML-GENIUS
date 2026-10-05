# DKV-2a-N12 — two-bank exact identity/provenance requalification

Status: frozen before implementation/execution.

Parent results:
- `SML-DKV-2A-N11-TWO-BANK-OBSERVATION.md`;
- `SML-DKV-3A-0-HARDENING.md`;
- `SML-DKV-C-OBSERVATION.md`.

## Motivation

N11 retained a two-bank Development resolver:

- PRIMARY q=3 for secondary-route continuity;
- ORDERED q=3 for explicit route-specific overrides;
- one exact `(semantic_record_id, ordered_route)` directory probe chooses the
  bank before one matrix read.

N11 intentionally used synthetic record IDs and did not claim that this
directory was authoritative source identity.

N12 requalifies that composition against the source/identity machinery already
owned by `sml-store`.

It does **not** create a new provenance system.

## Authoritative existing primitives

N12 must use the current APIs unchanged:

- `DeltaKvContextMemory` / immutable `WriteEvidence`;
- exact full-key `QueryEvidence`;
- `QueryEvidence::verify` -> private `VerifiedContext`;
- `RecordRequest` + `VerifiedContext::bind_record`;
- complete `VerifiedHistory` from `verify_history`;
- `bind_projected_subject` + `SubjectKey`;
- `screen_candidate` semantics;
- `Dkv4Provenance::{VerifiedPresent, Missing, Conflict, Unverified}`.

A receipt proves raw byte identity/provenance, not latent correctness, truth,
entailment or action permission.

## Frozen N11 memory geometry

- `SemanticState<64>`;
- q=3: 3/4 PAGE_VALUE + 1/4 PAGE;
- current ActiveOnly source projectors;
- PRIMARY_ANCHORED outer domain for PRIMARY;
- ORDERED_ROUTE outer domain for ORDERED;
- K in {256,1024};
- routes R2=[5,7], R3=[5,8], R4=[5,7,9], R6=[7,5];
- 16 deterministic seeds;
- two-bit Q12 values for this provenance panel;
- full-rate updates;
- clear threshold 128.

No projector, MOR, DeltaKvMemory or source-verification implementation changes
are allowed.

## Write locator: stored write identity, not query-key identity

Secondary-route continuity means the **current query key may differ from the
original write key**. Therefore N12 must not require the current projected query
key to mint provenance.

At admission, the harness records a bounded locator derived from the committed
write evidence:

```text
BankWriteLocator {
    bank
    stream_id
    semantic_record_id
    subject_key
    route_fingerprint
    exact_write_key_q12
}
```

For PRIMARY, route_fingerprint records the admission route but PRIMARY lookup is
record-scoped.

For ORDERED, the exact ordered route is part of override lookup.

The locator does not contain a confidence score or matrix value.

At query time:

1. the caller supplies `RecordRequest` and `SubjectKey` from an independent
   semantic/context identity path; neither may be derived from the projected key;
2. exact `(record_id, ordered_route)` override metadata chooses ORDERED if
   present; otherwise the record's PRIMARY locator is selected;
3. the selected locator's **stored write key** is used to obtain
   `QueryEvidence`, verify raw bytes, bind the requested record and bind the
   complete subject history;
4. only after a `VerifiedPresent` preflight may a source-required query perform
   one matrix read using the **current projected query key**;
5. Missing/Conflict/Unverified source-required queries perform no matrix read and
   do not fall back to the other bank.

This preserves N11 route continuity while keeping source identity independent of
the current associative geometry.

## Provenance classification

The harness maps current API outcomes to the existing `Dkv4Provenance` enum.

### VerifiedPresent

Only when all of the following hold:

- selected locator exists;
- exact stored write-key query exists;
- raw source verification succeeds;
- `RecordRequest` binds in the same stream;
- complete subject binding succeeds in the same verified ledger snapshot;
- subject `ObservedState::Agreement`.

Agreement does not validate the latent candidate.

### Missing

- no record locator for the requested identity; or
- verified complete history has no requested subject.

### Conflict

- complete subject binding succeeds but the subject's observed state is
  `Conflict`.

### Unverified

Any declared locator whose source/identity proof cannot be completed, including:

- corrupted/substituted raw bytes;
- stale/forked ledger snapshot;
- foreign stream;
- missing matching write behind a claimed locator;
- incomplete subject candidate sources;
- source/record binding failure;
- verification/binding budget failure.

A declared ORDERED override that is Unverified may **not** silently fall back to
PRIMARY.

## P0 — endpoint and write-evidence compatibility

For both K:

- PRIMARY/ORDERED q=3 keys reproduce N11 endpoint construction;
- `DeltaKvContextMemory::write` returns evidence whose full key equals the
  locator's stored write key;
- repeated key construction is byte-identical;
- complete selected-expert route evidence is retained;
- duplicate selected experts remain rejected.

## P1 — verified secondary continuity

For deterministic clean records:

- one raw `ObservationEvent` per record/subject;
- write its two-bit target to PRIMARY under R2;
- no ORDERED override locator;
- query the same record/state under R2, R3 and R4.

Preflight provenance is verified using the stored R2 write key while the matrix
read uses each current route's PRIMARY query key.

Report:

- provenance state;
- record/subject bind success;
- exact+clear latent retrieval;
- screened-candidate preservation;
- selected bank;
- source bytes/segments verified;
- matrix reads.

## P2 — verified explicit secondary override

For each clean record:

- the same verified raw source/record/subject is admitted;
- PRIMARY/R2 stores latent target A;
- ORDERED/R3 stores independent latent target B;
- an exact R3 override locator is installed only after both writes commit.

Queries:

- R2 -> PRIMARY A;
- R3 -> ORDERED B;
- R4 -> PRIMARY A.

Both bank derivations reference the same verified source identity. The source
receipt must **not** be described as evidence that A or B is semantically
correct.

Report latent exactness separately from provenance state.

## P3 — verified primary-switch override

Repeat P2 with ORDERED/R6.

R2 must use PRIMARY; R6 must use ORDERED.

## P4 — record/stream/subject negative controls

For clean written evidence, freeze these requests:

- nonexistent record ID;
- foreign stream;
- wrong subject;
- valid record view but incomplete subject candidate set;
- exact projected receipt containing unrelated subject sources.

Expected classification and binding errors must be reported without changing
the candidate.

A successful single-record bind must never upgrade an incomplete/conflicting
subject view.

## P5 — complete subject conflict

Create two verified active record versions for the same `SubjectKey` with
different two-bit observations and no supersession.

The selected write-key receipt must contain all subject versions.

Requirements:

- subject binding succeeds;
- observed state is Conflict;
- provenance maps to `Dkv4Provenance::Conflict`;
- source-required resolver performs zero matrix reads;
- screening yields no value for every latent candidate.

## P6 — explicit supersession

Create two verified versions for one subject where the second explicitly
supersedes the first.

Requirements:

- both versions remain present in verified history/source evidence;
- subject binding is complete;
- observed state is Agreement;
- provenance is VerifiedPresent;
- screening preserves the original candidate exactly;
- an intentionally wrong latent candidate is **not repaired** from source bytes.

## P7 — corrupted/stale/forked evidence

For a declared PRIMARY or ORDERED locator, independently test:

- same-length raw corruption;
- source substitution;
- stale ledger;
- same-count forked ledger;
- verification source/byte budget failure.

Every cell maps to Unverified.

For a declared ORDERED override:

- no fallback to PRIMARY;
- source-required matrix reads remain zero.

## P8 — associative alias is never source presence

Retain the current known projected-alias control:

- a verified receipt may contain real bytes belonging to another subject;
- requesting an unwritten subject must remain Missing;
- unrelated alias sources are excluded;
- conflicting versions of the requested subject remain Conflict;
- omitted requested-subject versions remain Incomplete/Unverified.

No projected key, cosine, Q12 magnitude or successful matrix read may create a
record/subject binding.

## P9 — query-state perturbation does not alter source identity

For a valid `RecordRequest`/subject/route, perturb the current V state by one
bit while retaining the independently supplied record identity.

Report separately:

- matrix candidate change;
- provenance classification;
- source identity.

VerifiedPresent is allowed to remain VerifiedPresent because provenance binds
the requested record/source, not the current latent state's correctness.

This panel explicitly demonstrates that source verification is not a latent
correctness oracle.

## P10 — read/accounting invariant

For source-required resolution:

- VerifiedPresent -> exactly one selected-bank matrix read;
- Missing -> zero matrix reads;
- Conflict -> zero matrix reads;
- Unverified -> zero matrix reads;
- no query ever reads both banks;
- counterfactual/diagnostic reads, if any, are separately labelled and excluded
  from resolver cost.

Report source verification bytes/segments, record/subject candidate counts and
matrix counters separately.

Logical counts are not latency/RSS evidence.

## Frozen hypotheses

H1 — clean verified continuity: at K=1024 P1 R2/R3/R4 latent exact+clear recall
is >=95%, every row is VerifiedPresent, and no row selects ORDERED.

H2 — explicit override: at K=1024 P2 R2/R3/R4 latent exact+clear recall is >=95%
with bank selection PRIMARY/ORDERED/PRIMARY respectively and VerifiedPresent
provenance in every row.

H3 — primary-switch override: at K=1024 P3 R2/R6 latent exact+clear recall is
>=95% with PRIMARY/ORDERED selection respectively.

H4 — identity negatives: P4 never produces VerifiedPresent for nonexistent
record, foreign stream, wrong subject or incomplete-subject controls.

H5 — conflict fail-closed: every P5 cell is Conflict, performs zero matrix reads
and screens every candidate to no value.

H6 — supersession boundary: every P6 cell is VerifiedPresent, retains every
version/source, preserves the original candidate, and never repairs an
intentionally wrong candidate.

H7 — unverified fail-closed: every P7 cell is Unverified; a declared ORDERED
override never falls back to PRIMARY and performs zero source-required matrix
reads.

H8 — alias boundary: P8 never converts an unwritten/wrong subject or incomplete
subject into VerifiedPresent.

H9 — provenance/correctness separation: P9 provenance identity is unchanged by
the one-bit latent-state perturbation while latent retrieval is reported
independently.

H10 — one-bank verified read invariant: across P1/P2/P3/P6/P9, every
VerifiedPresent source-required resolution performs exactly one selected-bank
matrix read and every non-VerifiedPresent resolution performs zero.

A failed hypothesis remains failed. Do not retune routes, q, thresholds,
identity rules or provenance classification after first execution.

## Selection rule

If H1-H10 hold, retain N11 two-bank composition with the N12 verified write
locator as the leading **Development identity/provenance semantics**.

This still does not promote a core API or context model.

If a gate fails, keep N11 as a synthetic memory-composition result and report
the failed source/identity integration without fallback heuristics.

## Boundary

N12 does not establish:

- source truth or authenticity;
- semantic entailment;
- correctness of latent A/B values;
- action permission;
- learned/representative model quality;
- physical latency/RSS/energy;
- crash-durable sidecar recovery.

After N12, remaining model-core gates include representative SBG/MOR states,
unbalanced page populations/cross-page sharing and physical resource
qualification.

Context/model promotion remains blocked.
