# SML-CONTEXT-1a immutable raw segment store

Status: implementation candidate. This is persistence/source-integrity
infrastructure, not an effective-context result.

## Artifact

Each raw segment is published as one immutable artifact containing:

- CONTEXT-0 stream and segment identity;
- exact byte/token spans and order index;
- canonical semantic-record IDs;
- SHA-256 of the exact raw source bytes;
- CRC32C of the semantic-ID section;
- CRC32C of the fixed header;
- exact raw bytes.

SHA-256 is recomputed on build and read. CRC32C is corruption detection only and
is not used as source identity or authentication.

## Publication

publish_raw_segment builds and fsyncs a complete temporary file, then publishes
it with a no-overwrite hard link. Existing target paths fail closed.

This gives atomic visibility of a complete inode in the reference filesystem
path without mutating a published segment. Cross-filesystem/network-filesystem
durability semantics are not claimed.

## Read boundary

load_raw_segment requires explicit independent limits for:

- total artifact bytes;
- raw payload bytes;
- semantic provenance links.

The parser validates header structure/checksum, canonical layout, semantic-ID
CRC/order, exact file length and SHA-256 before returning source bytes.

## SHA-256 implementation

sml-store contains a small dependency-free SHA-256 implementation so sml-core
remains dependency-free. Qualification includes the standard empty-string and
abc SHA-256 vectors.

This implementation provides content identity for the research artifact. It is
not a digital signature, authorization primitive or proof that the producer was
trusted.

## Next slice

CONTEXT-1b must add bounded streaming ingest that:

1. emits CONTEXT-0 ledger entries in order;
2. publishes raw segment artifacts through this store;
3. emits semantic records/index artifacts separately;
4. keeps the complete raw stream outside the bounded ingest buffer;
5. proves deterministic boundaries and exact provenance.
