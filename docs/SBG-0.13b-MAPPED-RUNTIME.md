# SBG-0.13b: Linux read-only mapped semantic store

Implementation: `crates/sml-store`. Core format/descriptor: `sml-core`.

## Contract

`MappedSemanticStore::<BITS>::open(path, limits)` opens a regular immutable file.
On Linux it uses `/proc/self/fd/<fd>` to map the already-open inode through
`mmap-guard = 0.2.2`. The mapping owns a read-only handle and cooperative shared
lock. No loaded-buffer fallback is allowed. Unsupported platforms fail closed.
The upstream OS/unsafe implementation remains a third-party dependency; the
workspace's `forbid(unsafe_code)` does not certify that dependency as sound.

The file must not be modified or truncated while any mapping exists. Every
writer must honor the lock protocol. Non-cooperative mutation can invalidate
Rust references or cause SIGBUS; this API does not prevent that. Publish a new
file via atomic rename rather than modifying the old inode. Deployment must
restrict access to immutable artifacts. CRC is not an authenticity mechanism.

## Work and accounting

- Open: bounded header/index validation, O(records) time, no decoded dictionary.
- Lookup: binary search with O(log(records)) index entries, fixed-header recheck,
  lazy CRC32C and V/M/A/R decoding for a hit only.
- Limits: file virtual bytes, index entries, decoded payload bytes per record.
- Zero full-file heap fallback and no retained decoded-state cache.
- Returned owned states may outlive the mapping; caller-retained states are not
  included in the reader's admission limits.
- Counters report virtual mapping size and cumulative decoded payload, NOT RSS,
  page-cache residency, actual allocated bytes, page faults or physical I/O.
- At BITS=16,384 the decoded packed payload is 8,192 bytes; vector/container
  metadata and allocator overhead are additional.

The Linux adapter adds canonical-layout checks (including nonzero/sufficient
stride) before any record decoding. Existing v1 format bytes remain compatible.
Header CRC and payload CRC are verified; the index is structurally validated,
not cryptographically authenticated. A same-header descriptor is not a content
hash and must not be reused with a different or mutated image.

## Qualification

`cargo test -p sml-store -- --nocapture`

Tests cover actual Linux file mappings, lazy decoding, valid empty stores,
truncation before mapping, header/payload corruption, wrong dimensions, limits,
regular-file admission, simultaneous readers and shared-lock lifetime, an
exclusive writer, atomic file replacement, and forged valid-CRC bad strides.
Never truncate a live mapping in an in-process test.

Full qualification is `cargo fmt --all -- --check`, `cargo check --workspace
--all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, and
`cargo test --workspace`, plus the repository's existing CLI smoke probes.

## Next evidence gate

Measure cold/warm lookup latency, RSS/page faults and throughput over growing
stores before claiming the target memory/throughput envelope. A streaming
writer, authenticated index/manifest, portable backends, and immutable-file
hardening beyond cooperative locks remain separate work. SBG-0.14 must integrate
this reader with SBG/MOR/Elastic execution; the reader alone is not a model.

## Ecosystem reuse

The native SML descriptor and lazy record contract are reusable design inputs
for SciRust storage/representation tests and ElasticXxx page-budget admission.
No other Memorithm repository was changed by this slice. Shared benchmark data
should precede promotion; SML has no runtime dependency on those repositories.

## Upstream references

- mmap-guard 0.2.2, `map_file`/`FileData` and cooperative-lock limitations:
  https://docs.rs/mmap-guard
- Rust mmap dependency source: https://github.com/EvilBit-Labs/mmap-guard

Dependency versions must be frozen in the workspace lockfile before merge.
