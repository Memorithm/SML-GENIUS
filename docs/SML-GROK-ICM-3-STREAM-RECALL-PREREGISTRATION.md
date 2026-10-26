# GROK-ICM-3 — Guarded shift memory vs naked Shift75 vs alternating Rule90 under faults

Date: 2026-10-27. Stacked on GROK-ICM-2 (#742). Research example only.
Answers the question left open by GPT's SKY-R4 (#739, unmerged; NOT a
dependency — the alternating-Rule90 control is reimplemented here from its
published description).

## Question (frozen before code)
In a never-reset bit stream with injected state faults, does a 75-bit shift
register protected by the ICM composite guard (SECDED72_64 per word + CRC32
over the state, from ICM-1/2 conclusions) recall the input at every age
1..=74 with zero silent errors, where naked Shift75 and alternating-site
Rule90 cannot?

## Arms (all 75 logical state bits, one input bit per step)
- SHIFT75: naked shift register; bit j holds x_{t-j}.
- SHIFT75+ICM: same, state stored in 2 u64 words, guard recomputed after
  each shift (a shift changes every word: no incremental O(1) delta applies —
  cost stated, not hidden), scrubbed before the next step. Correction repairs;
  uncorrectable detection → state marked UNKNOWN, reads are ERASURE until
  75 clean steps have refilled it.
- ALT_RULE90: periodic 75-ring, s'_i = s_{i-1} XOR s_{i+1}, input XOR-injected
  at site 0 on even t and site 1 on odd t. Readout: exact GF(2) inverse of the
  75×75 impulse matrix for the current phase (zero-older-history block decoder,
  as in SKY-R4). Rank 75 for both phases is asserted (independent check of
  #738's claim).

## Protocol
Inputs: SplitMix64 bit 32. Sealed TEST seeds 2000..2004, 4096 steps each,
first 150 steps unscored. Faults: at each step with probability 1/32, flip k
distinct random state bits after the guard was computed; panels k=1 and k=2,
plus a no-fault panel. Scored ages: all 1..=74 every step. Outcome per read:
CORRECT / ERASURE / WRONG(silent).

## Gates (frozen)
SHIFT75+ICM PROMOTED iff on TEST, for k∈{1,2}: WRONG = 0 AND ERASURE ≤ 5 %
of reads AND strictly fewer WRONG than both other arms. No-fault panel:
SHIFT75 and SHIFT75+ICM must be 100 % correct (else protocol bug).
Expected: ALT_RULE90 ≈ 50 % WRONG in stream even without faults (SKY-R4
reproduction); naked SHIFT75 WRONG > 0 under faults.

## Non-claims
No skyrmion physics, no energy claim; the guard costs a full recompute per
shift, reported as such.

## Reproduction
`cargo test --locked -p sml-core --example grok_icm_stream_recall`

## Results
(Filled only from executed runs.)
