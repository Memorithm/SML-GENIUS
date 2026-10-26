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
(Filled only from executed runs.)
