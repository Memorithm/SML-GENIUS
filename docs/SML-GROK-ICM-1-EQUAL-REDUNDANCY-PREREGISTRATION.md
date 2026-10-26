# GROK-ICM-1 — Local codes vs SECDED/CRC/REP3 at equal redundancy: preregistration

Date: 2026-10-27. Stacked on GROK-ICM-0 (#740). Research example only.

## Question (frozen before code)
For one 64-bit data word, do cheap *local* parity codes (bit-lane interleaved
parity, 8×8 row/column product parity) beat conventional SECDED(72,64) and
CRC at the **same number of check bits**, on exhaustive 1-, 2-, 3-bit faults
and all contiguous bursts of length 2..=8, counting SILENT outcomes first?

## Arms
Tier A (8 check bits, 12.5 %): SECDED72_64 (ICM-0), CRC8 (poly 0x07, init 0,
MSB-first, detect only), INTERLEAVE8 (check j = parity of data bits i≡j mod 8,
detect only).
Tier B (16 check bits, 25 %): PRODUCT8X8 (8 row + 8 column parities, single
error correction when exactly one row and one column disagree), CRC16
(CCITT poly 0x1021, init 0, detect only).
Reference (128 check bits, 200 %): REP3 bitwise majority. Not equal-budget;
reported only as an upper reference.

## Panel
Data word: 0x0123_4567_89AB_CDEF and 0 (two fixed words). Codeword serialized
as data bits 0..63 then check bits. Exhaustive: every 1-, 2-, 3-bit flip set;
every contiguous burst (all bits in a window flipped) of length 2..=8 at every
start. Outcomes as ICM-0: RECOVERED / CORRECTED / ERASURE / SILENT.

## Decision rule (frozen)
A local arm is PROMOTED for ICM-2 only if, within its tier, it has SILENT=0 on
1-bit, 2-bit and burst panels AND no more SILENT on 3-bit than the best
conventional arm of that tier. Otherwise it is recorded as NEGATIVE.
Mathematical expectations frozen now: INTERLEAVE8 has SILENT>0 on 2-bit
(two flips in the same lane) → expected NEGATIVE; SECDED has SILENT>0 on 3-bit
(miscorrection); PRODUCT8X8 has SILENT=0 on 1- and 2-bit.
The arm carried into ICM-2 is the best tier-A arm by this rule (fallback
SECDED72_64 if no local arm is promoted).

## Stop / failure
Any arm whose encode/decode fails its own clean-word self-test → stop.
Report must be byte-identical on replay.

## Non-claims
No timing/energy; no physical noise model; burst model is serialized-order only.

## Reproduction
`cargo test --locked -p sml-core --example grok_icm_codes`

## Results
Local execution (cargo 1.89.0), byte-identical replay:

```
GROK-ICM-1 report v1
SECDED72_64 check_bits 8
  1bit: trials 144 recovered 0 corrected 144 erasure 0 silent 0
  2bit: trials 5112 recovered 0 corrected 0 erasure 5112 silent 0
  3bit: trials 119280 recovered 0 corrected 0 erasure 28672 silent 90608
  burst2-8: trials 952 recovered 0 corrected 0 erasure 482 silent 470
CRC8 check_bits 8
  1bit: trials 144 recovered 0 corrected 0 erasure 144 silent 0
  2bit: trials 5112 recovered 0 corrected 0 erasure 5112 silent 0
  3bit: trials 119280 recovered 0 corrected 0 erasure 119280 silent 0
  burst2-8: trials 952 recovered 0 corrected 0 erasure 952 silent 0
INTERLEAVE8 check_bits 8
  1bit: trials 144 recovered 0 corrected 0 erasure 144 silent 0
  2bit: trials 5112 recovered 0 corrected 0 erasure 4536 silent 576
  3bit: trials 119280 recovered 0 corrected 0 erasure 119280 silent 0
  burst2-8: trials 952 recovered 0 corrected 0 erasure 952 silent 0
PRODUCT8X8 check_bits 16
  1bit: trials 160 recovered 0 corrected 160 erasure 0 silent 0
  2bit: trials 6320 recovered 0 corrected 0 erasure 5936 silent 384
  3bit: trials 164320 recovered 0 corrected 0 erasure 154336 silent 9984
  burst2-8: trials 1064 recovered 0 corrected 0 erasure 1062 silent 2
CRC16 check_bits 16
  1bit: trials 160 recovered 0 corrected 0 erasure 160 silent 0
  2bit: trials 6320 recovered 0 corrected 0 erasure 6320 silent 0
  3bit: trials 164320 recovered 0 corrected 0 erasure 164320 silent 0
  burst2-8: trials 1064 recovered 0 corrected 0 erasure 1064 silent 0
REP3 check_bits 128
  1bit: trials 384 recovered 0 corrected 384 erasure 0 silent 0
  2bit: trials 36672 recovered 0 corrected 36288 erasure 0 silent 384
  3bit: trials 2322560 recovered 0 corrected 2249856 erasure 0 silent 72704
  burst2-8: trials 2632 recovered 0 corrected 2632 erasure 0 silent 0
verdict INTERLEAVE8 NEGATIVE
verdict PRODUCT8X8 NEGATIVE
fnv1a64 2a16dc134771dbd3
```

### Verdicts (by the frozen rule)
- INTERLEAVE8: NEGATIVE (576 silent 2-bit, same-lane pairs) — as expected.
- PRODUCT8X8: NEGATIVE. **Frozen expectation "SILENT=0 on 2-bit" is FALSIFIED**:
  384 silent 2-bit cases = one row-check bit + one column-check bit flipped,
  which the decoder miscorrects as a data flip. Without a corner/overall check
  the 8×8 product code has distance 3 on the full codeword. Kept as a negative.
- No local arm is promoted. By the frozen fallback, **SECDED72_64** is carried
  into ICM-2.
- Additional observations (not promotions): SECDED is SILENT on 470/952 bursts
  and 90,608/119,280 3-bit sets (miscorrection); CRC8 and CRC16 have SILENT=0
  on every panel here but correct nothing. ICM-2 must therefore keep CRC as a
  detection baseline next to SECDED. REP3 (200 % overhead) still has silent
  2-bit cases (same bit in two copies).
