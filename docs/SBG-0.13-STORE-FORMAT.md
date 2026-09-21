# SBG-0.13 semantic store format v1

This document specifies the reference on-disk format introduced by SBG-0.13a.
It is designed so a later read-only mmap runtime can binary-search metadata and
decode only requested semantic states.

## Byte order and alignment

All integers are little-endian.

- file/page alignment: 4096 bytes;
- header size: 64 bytes;
- index begins at byte 4096;
- every record payload begins at a 4096-byte boundary;
- record stride is the payload storage size rounded up to 4096 bytes.

The default 16,384-bit SBG state contains four fields (V/M/A/R), each stored as
256 u64 words. Its record payload is therefore exactly 8192 bytes and its record
stride is exactly 8192 bytes.

## Header

| Offset | Bytes | Field |
|---:|---:|---|
| 0 | 8 | magic: SMLSBG13 |
| 8 | 2 | version = 1 |
| 10 | 2 | header bytes = 64 |
| 12 | 4 | logical bits per field |
| 16 | 2 | field count = 4 |
| 18 | 2 | word bytes = 8 |
| 20 | 4 | index entry bytes = 24 |
| 24 | 8 | record count |
| 32 | 8 | index offset |
| 40 | 8 | records offset |
| 48 | 8 | record stride |
| 56 | 4 | CRC32C over bytes 0..56 |
| 60 | 4 | flags/reserved = 0 |

Unknown versions, flags, dimensions, field counts, word widths or misaligned
layouts fail closed.

## Index entry

Index entries are sorted strictly by semantic_id and have fixed width.

| Offset | Bytes | Field |
|---:|---:|---|
| 0 | 8 | semantic_id |
| 8 | 8 | record offset |
| 16 | 4 | payload CRC32C |
| 20 | 4 | payload bytes |

This permits allocation-free binary search directly against a borrowed file
image or future mapped byte slice.

## Record payload

A record is the concatenation of packed little-endian u64 words for:

1. V — semantic value;
2. M — context meaning mask;
3. A — ambiguity;
4. R — routing.

Padding bits outside the declared logical dimension must remain zero because
record decoding passes through SemanticBitField::from_words.

Payload CRC32C is checked only when a record is requested. Parsing a store does
not touch every payload, which is necessary for large mapped stores.

## Integrity model

SBG-0.13a provides corruption detection, not cryptographic authenticity.

- header: CRC32C;
- each record payload: CRC32C stored in its index entry;
- index structure: strict sort order, fixed offsets, exact file length and
  alignment checks.

A later authenticated or signed artefact layer can bind the complete file hash
without changing record-level lazy corruption checks.

## Scope boundary

SBG-0.13a defines and qualifies the byte format and borrowed parser. It does not
claim OS-backed mmap yet. SBG-0.13b will select and qualify the read-only mapped
runtime while preserving the workspace safe-Rust policy.
