# DKV-2a-N12 — implementation fixture freeze

Status: frozen before N12 implementation execution.

Parent protocol:
`SML-DKV-2A-N12-VERIFIED-IDENTITY-PREREGISTRATION.md`.

This file fixes deterministic implementation details left open by the parent
protocol. No N12 scientific output existed when this file was committed.

## Common clean grid

- Rust 1.89.0;
- K in {256,1024};
- seeds 0..15;
- 8 clean records per seed for P1/P2/P3/P9;
- one raw segment per semantic record;
- `ContextStreamId(1)`;
- clean subjects `SubjectKey(record_index + 1, 0)`;
- record IDs are `seed * 100 + record_index + 1`;
- segment IDs are monotonically increasing from 1 within each independent
  panel world;
- state V/M/R fields are deterministic xorshift64-derived 64-bit words;
- A is all false;
- clean source bits are the low two bits of a deterministic target word;
- independent ORDERED latent target B is the Boolean complement of source/base
  target A, so A != B in every override row;
- q=3 and route constants remain exactly N9/N11.

Each seed/panel uses an independent ledger/world; no state carries between panel
families.

## P4 identity controls

Per K and seed use one clean record plus:

- nonexistent record ID = written ID + 10_000;
- foreign stream = ContextStreamId(2);
- wrong subject = SubjectKey(written_subject + 1000, 0);
- incomplete-subject world: two active record versions share one SubjectKey,
  but only one version is present behind the selected write-key receipt;
- unrelated-source world: selected exact write key contains requested record plus
  one record from a distinct SubjectKey.

## P5 conflict

Per seed:

- one subject;
- record 1 bits [false,false];
- record 2 bits [true,true];
- neither supersedes the other;
- both records are written behind the same selected PRIMARY write key.

## P6 supersession

Per seed:

- one subject;
- record 1 bits [false,false];
- record 2 bits [true,true] and predecessor=record1;
- both records are written behind the same selected PRIMARY write key;
- intentionally wrong screened candidate = [false,false].

## P7 unverified controls

Use one declared R3 ORDERED override per seed.

Test independently:

1. same-length one-byte raw corruption;
2. source substitution with another admitted segment's bytes;
3. stale ledger missing the final admitted segment;
4. same-count fork with a changed descriptor/hash;
5. VerificationLimits max_sources=0.

No fallback path is allowed.

## P8 alias control

Reuse the current exact-key source-binding principle rather than searching for a
new probabilistic collision:

- two distinct subjects are deliberately admitted behind one identical stored
  write key;
- request each subject independently;
- request a third unwritten subject;
- then create an incomplete requested-subject version on a different write key.

This is an exact candidate-set/source-binding control, not a claim about random
q=3 collision probability.

## P9 perturbation

For every clean P1-style record, flip one deterministic V bit selected from
`tagged(seed, record_id, 1, 77) % 64`.

The independently supplied RecordRequest and SubjectKey remain unchanged.

## Budgets

Context memory per bank:

- max_writes: 32;
- max_sources_per_write: 8;
- max_source_links: 128;
- max_ledger_segments: 32;
- max_ledger_semantic_links: 128;
- max_element_bytes: 8 MiB;
- max_staging_bytes: 8 MiB.

Verification:

- max_sources: 32;
- max_raw_bytes: 64 KiB.

Record binding:

- max_write_records: 32;
- max_sources: 32.

History:

- max_events: 32;
- max_keys: 32;
- max_element_bytes: 64 KiB.

History verification:

- max_segments: 32;
- max_raw_bytes: 64 KiB;
- max_record_bytes: MAX_EVENT_BYTES.

Subject binding:

- max_candidate_sources: 32;
- max_record_bytes: MAX_EVENT_BYTES.

No value, route, threshold or fixture count may change after the first N12
execution.
