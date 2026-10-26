# GROK-ICM-0 — Incremental syndrome page guard: preregistration

Date: 2026-10-27. Status: RESEARCH REFERENCE ONLY; no runtime integration.
Series: GROK-ICM (Invariant-Checked Memory). Independent of the SKY-M/SKY-R
line (#732–#739): no topological charge, no cellular automaton, no skyrmion
claim. The information lives in the page data; the invariant is only a guard.

## Question (frozen before code)

For a page of `n` 64-bit words, can a guard syndrome be maintained by an
incremental per-write update `apply_write(idx, old, new)` that is **exactly
equal** to a full recomputation `init(page)` after every write, for three
conventional codes, and what does each code actually do under exhaustive
1-bit and 2-bit faults of the stored codeword (page data + guard bits)?

## Arms (baselines, all conventional)

| Arm | Guard bits | Incremental update |
|---|---|---|
| ParityGuard | 1 per page | `p ^= popcount(old^new) & 1`, O(1) |
| Crc32Guard (CRC-32/MPEG-2: poly 0x04C11DB7, init 0xFFFFFFFF, no reflection, xorout 0; words big-endian) | 32 per page | `crc ^= mulmod(crc_lin(old^new), x^(64*(n-1-idx)) mod P)`, O(1) word ops using a per-position table of n u32 built once in O(n) |
| Secded72_64Guard (extended Hamming, 7 Hamming + 1 overall parity per word) | 8 per word | recompute the one check byte of word `idx`, O(1) |

CRC32 check value on ASCII "123456789" must equal 0x0376E6E7.

## Oracle

`G::init(&page).syndrome()` (full scan) is the oracle. Any divergence of the
incremental syndrome from the oracle is a failure.

## Frozen panels

1. Delta-vs-full equality: page sizes 64 bits (1 word) and 4096 bits (64
   words); 10,000 deterministic writes each (SplitMix64, seed 0x1C40), oracle
   compared after every write, per arm.
2. Exhaustive faults, 64-bit page (fixed content 0x0123_4567_89AB_CDEF):
   every 1-bit flip and every unordered distinct 2-bit flip of the codeword.
3. Exhaustive 1-bit faults, 4096-bit page (SplitMix64 seed 0x1C41 content).
   2-bit on 4096 bits (~8.5M pairs per arm) is OUT of scope for ICM-0.
4. Atomic reject: out-of-range index, and SECDED write onto a word whose
   stored check byte disagrees with its data; page and guard must be
   byte-identical after the rejected call.
5. Report generated twice must be byte-identical; FNV-1a-64 hash printed.

Outcome classes: RECOVERED (check clean, data intact), CORRECTED (decoder
restored original data), ERASURE (detected, refused), SILENT (accepted with
wrong data, including miscorrection). An erasure is never counted as recovery.

## Expected results (frozen; mathematical, to be confirmed by execution)

- Parity: 1-bit all ERASURE; 2-bit all SILENT if both flips hit data bits.
- CRC32: 1-bit and 2-bit all ERASURE (Hamming distance ≥ 4 at these lengths).
- SECDED (64-bit page = one 72-bit word): 1-bit all CORRECTED; 2-bit all ERASURE.

## Failure / stop criteria

- Any incremental-vs-full divergence → ICM-0 FAILS, no follow-up slice.
- Any SILENT outcome for CRC32 or SECDED in panels 2–3 → implementation bug or
  wrong claim; stop and record.
- Report non-replayable → fail.
- Non-atomic reject → fail.

## Non-claims

No wall-time, energy, or throughput claim. No claim that ICM beats SKY-M; it
answers a different question. No change to SBG/MOR/Delta-KV/PVP/BAF runtime,
`src/lib.rs`, or any dependency. Pure Rust, no GPU.

Promotion to ICM-1 (equal-redundancy local codes vs SECDED/CRC/REP3) requires
this panel green on the exact head in full workspace CI.

## Reproduction

```sh
cargo test --locked -p sml-core --example grok_icm_syndrome
cargo run --locked -p sml-core --example grok_icm_syndrome
```

## Results

(To be filled only from executed runs.)
