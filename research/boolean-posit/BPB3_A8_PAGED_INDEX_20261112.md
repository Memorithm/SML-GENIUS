# BPB-3-A8 paged index — bounded page-local seeking (2026-11-12)

Status: **experimental implementation**, no change to the A8 numerical format, weight fidelity, production inference, or standardized Posit arithmetic. This extension is separate from the BPB3-K AVX2 multiplication research.

## Why this is needed

The A8 codec uses independently decodable **variable-length** 256-coefficient superblocks with length `41 + 2 × sum(mode[g])` bytes (73–137 B). A raw concatenation does **not** provide constant-time random access: locating the nth block otherwise requires scanning all preceding variable-length headers. The A8 108B/256 source-study storage budget counts original compressed wires, but not the index required for random access. This extension measures and charges the metadata rather than treating block boundaries as free.

## Canonical page-local wrapper

A page contains **1..256** original A8 wires, unchanged. Wire layout:

| Byte range | Field | Notes |
|---|---|---|
| 0..3 | magic `BA8P` (ASCII) | format marker, not an authenticity tag |
| 4..5 | number of blocks, `u16 LE` | accepted 1..256 |
| 6..7 | A8 payload byte length, `u16 LE` | ≤ 256 × 137 = 35,072 |
| 8.. | checkpoint offsets, `u16 LE` | exactly `ceil(n/8)` offsets, one at each 8-block boundary, relative to payload start |
| remainder | concatenated canonical A8 wires | byte-for-byte identical to original packed A8 |

Page metadata = `8 + 2 × ceil(n/8)` bytes; **72 bytes for 256 blocks = 0.28125 B per 256 original weights**. An illustrative 108B mean A8 payload becomes **108.28125B/256** for a full 256-block page, before any inter-page directory, alignment, model-header or allocation overhead. For the original **24-row** corpus, one page costs 14 extra bytes, giving **108.583333… B/256**. These are storage-accounting arithmetic statements, **not measurements**.

For every random lookup, a validated page uses the nearest 8-block checkpoint and reads **at most seven preceding mode headers** plus the requested block. The complete page must first pass an O(n) one-time validation that checks every A8 block's canonical decode, every checkpoint offset, the exact payload length and the absence of trailing bytes. `ValidatedPage` holds an immutable borrow, preventing mutation after validation. `get` returns a borrowed wire slice; no copied or transient unpacked weight buffer is required to navigate.

**Scope and limits:** this is bounded **page-local** navigation, not fully random-access model tensor addressing. The external directory of variable-length page positions, tensor offsets, alignment and paging remain unimplemented and uncounted. As a result, the 0.28125B figure must **never** be advertised as a complete end-to-end indexing cost. No actual cache misses, DRAM bytes, memory bandwidth, whole-model perplexity, token agreement, model throughput or energy have been measured.

## Qualification

Standalone CPU-only Rust 1.89 tests in `bpb3_a8_paged.rs` verify that all 256 rows return their **original exact wire bytes**, including checkpoint boundary accesses and short pages; reject invalid magic/checkpoints, noncanonical A8 mode codes, truncated or overlong pages, zero/oversized page counts and out-of-range accesses; verify exact charged index bytes. All page inputs pass the original A8 canonical decoder.

Run on a GitHub-hosted CPU only:

```bash
rustfmt --edition 2021 --check research/boolean-posit/bpb3_a8_paged.rs
rustc --edition 2021 -D warnings --test research/boolean-posit/bpb3_a8_paged.rs -o /tmp/bpb3-a8-paged-tests
/tmp/bpb3-a8-paged-tests
```

This qualifies wire correctness and local seek bounds only. It does **not** qualify full-model A8 operation. No Dell/Thor/Mac jobs, vendor SDKs or production services are involved. Any physical benchmarks must be explicitly admitted through Memorithm/RemoteOps.
