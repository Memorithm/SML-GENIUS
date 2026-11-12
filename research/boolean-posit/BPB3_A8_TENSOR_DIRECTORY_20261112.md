# BPB-3-A8 tensor directory and exact bipolar matvec (2026-11-12)

**Research-only.** This is a real functional whole-tensor indexing and computation increment, **not** trained SML model inference, a native ggml replacement, or proof of quality/speed. BPB-3-A8 is not a standardized Posit.

## Canonical tensor wire

The original A8 superblock's 256-weight bitplanes, group exponents, decoding, and signed Boolean POPCOUNT arithmetic remain **byte-for-byte unchanged**.

Serialized tensor format:

- Bytes 0..3: ASCII magic `BA8T`.
- Bytes 4..7: u32 LE original row count, minimum one.
- Bytes 8..11: u32 LE total page count = ceil(row count / 256), capped at 4,096.
- Bytes 12..: **page_count + 1** little-endian u32 offsets relative to the following page-data payload, with the first offset zero, last offset equal to the entire payload length and all adjacent offsets strictly increasing.
- Following: every original canonical `BA8P` page, each containing exactly 256 independently decodable original A8 wires, except the final page of 1..256 wires.

At construction time, the reader validates all page offsets, page cardinalities, all per-page checkpoints and all original A8 blocks. It retains page borrows and a bounded in-memory reference directory. Each subsequent row lookup uses O(1) page addressing plus at most seven earlier block-mode-prefix scans inside that page; there is no full preceding-tensor scan or copied weight buffer for random access.

The `signed_bipolar_matvec(mask)` operation uses **one common 256-element bipolar input** (four u64 words; 1-bit is negative) for each A8 matrix row. It returns one **exact checked i128 integer in units of 2^-24** per row using the original signed POPCOUNT A8 kernel. For recurring token vectors, `signed_bipolar_matvec_into(mask, outputs)` uses a preallocated caller-owned `i128` output slice and rejects wrong-length buffers before mutation, avoiding per-call output-vector allocation. The path does not unpack all 256 coefficients into a dense float buffer and explicitly rejects corrupt or unvalidated tensor data. Independent tests reconstruct the original signed coefficients and sum a scalar reference from the actual quantized codes and dyadic exponents.

## Fully charged serialized storage

For n original A8 rows across p = ceil(n/256) pages, the added storage is:

```
tensor_directory_bytes(n) = 12 + 4 × (p + 1)
page_local_bytes(n)      = Σ_pages [8 + 2 × ceil(page_rows / 8)]
total_serialized_bytes   = Σ_original_A8_wire_bytes + tensor_directory_bytes + page_local_bytes
```

For exactly **256 blocks** (each 256 original weights) stored together, **92 additional bytes** (20 directory + 72 page-local) = **0.359375 extra bytes per original 256-weight superblock** beyond A8 payload. If those superblocks averaged 108B, all-in is 108.359375 B per 256 weights, *excluding* model tensor headers, physical alignment, allocation and runtime memory. For the original **24-row** A8 study, overhead is **34B** (20 + 14), or about **1.416667B per 256-weight row** beyond the 108B average payload. Reporting A8 as 108B/256 end-to-end without these charges would be incorrect.

This is an **exact on-wire size accounting**, not a measured hardware-byte-read value. There is still no general real-valued activation path, native SIMD kernel, transcode of a full trained model, tokenization, model perplexity, throughput, memory traffic, cache/NUMA or energy measurement.

## Tests

The hosted Rust 1.89 standalone workflow compiles and executes the tensor's own tests plus the original A8/page tests. It checks every byte across 1,24,255,256,257,511,512,513 rows, 300-row exact integer bipolar matvec and reusable-buffer output against independent scalar codes at four input masks (including wrong-length rejection without mutation), two-level directory corruption, page corruption, length and bound failures, and exact serialized overhead. The parent page-local workflow remains part of the same gate.

No Dell, Thor, Mac, vendor SDK or production process is involved. Physical qualification, if needed, must be admitted by Memorithm/RemoteOps and cannot be inferred from these hosted CPU checks.
