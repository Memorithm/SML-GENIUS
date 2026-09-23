# SML-CONTEXT-0 immutable segment contract

Status: implementation candidate. No long-context capability claim.

## Contract

CONTEXT-0 introduces dependency-free identities and accounting in sml-core:

- ContextStreamId: non-zero stable stream identity;
- ContextSegmentId: stable non-zero segment identity scoped to one stream;
- ContextContentHash: caller-supplied SHA-256 digest of exact raw bytes;
- ContextSegmentRef: byte/token spans, order, content hash and forward semantic IDs;
- SemanticRecordProvenance: reverse semantic-record -> raw-segment provenance;
- RawContextLedger: append-only reference ledger with exact logical counters.

## Ordering and spans

The reference ledger requires:

- first segment begins at byte 0, token 0, order 0;
- later byte/token spans are contiguous with the previous segment;
- order_index increments by exactly one;
- segment_id is strictly increasing;
- byte length is non-zero;
- span arithmetic is checked;
- semantic record IDs are ascending/unique after canonicalization.

Tokenizer token counts are caller-supplied accounting under the caller's frozen
tokenizer contract. CONTEXT-0 does not define a tokenizer.

## Provenance

Forward segment -> semantic IDs and reverse semantic -> source segments must agree
exactly when validate_provenance() is called. Unknown segments, missing source
links and mismatched forward/reverse provenance fail closed.

Raw segment bytes remain authoritative. A semantic record is derived evidence and
cannot replace the raw source for exact-copy/source-attribution tasks.

## Content identity

CONTEXT-0 freezes the hash algorithm identifier to SHA-256 but sml-core does not
implement hashing. The admitted ingest/persistence layer must compute and verify
the digest over the exact raw bytes.

SHA-256 identity is distinct from the CRC32C corruption checks used by the SBG
semantic store.

## Evidence boundary

RawContextLedgerStats counts logical segments, raw bytes, tokenizer tokens and
semantic links. It is not RSS, allocator memory, mapped bytes, page-fault or disk
I/O evidence.

CONTEXT-1 will add bounded streaming ingest/persistence against this contract.
