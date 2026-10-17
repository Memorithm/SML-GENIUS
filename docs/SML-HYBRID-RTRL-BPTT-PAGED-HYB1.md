# HYB-1: paged MOR/ANF mixed RTRL + truncated BPTT (pre-registration)

## Motivation and distinction

Previous **physically matched** [Dell run #39021666104](https://github.com/Memorithm/RemoteOps/actions/runs/39021666104) measured RTRL DEV 3.616843435 bits/byte versus BPTT DEV 7.885609702, with identical 50,000,896 allocated f32 weights, 64 KiB TRAIN, 32 KiB same-domain heldout, 128-byte windows and exact SHA-matched prefixes. This **does not isolate** a superiority of the RTRL gradient: online RTRL updates per bit while BPTT used normalized gradients and one update per window.

For a fixed parameter snapshot and the same finite recurrent horizon, exact RTRL and exact BPTT evaluate the same gradient by different chain-rule traversals. Averaging the identical gradient estimates provides no additional information. This v1 hybrid instead mixes their **update schedules**, preserving a single recurrent cell, MOR gate and physically paged trainable expert atlas.

## Algorithm: fixed 3R/1B

Within each document, partition predicted byte positions into disjoint contiguous windows of at most 128 positions (max supported by kernel: 512). Preserve 2-byte causal context, reset recurrent state at every window. For window indices 0,1,2 modulo 4, apply existing **online RTRL**; for index 3 modulo 4, apply existing **frozen-window truncated BPTT**. Only one optimizer processes each supervision target; all 8 output bits are counted exactly once.

No adaptive mixing from future labels or DEV, no extra train pass. Other policies (1R/1B or loss-adaptive) are *not* tried against the protected holdout here. `train_document_hybrid_windowed` exposes explicit RTRL/BPTT target-bit counts and per-window counts; `--pilot-paired-hybrid` emits `hybrid_schedule.tsv` and the same SHA-bound `paired_selection.tsv` as the RTRL/BPTT controls.

## Native comparison design

Three independent, equally initialized f32 masters: RTRL, BPTT and HYB-1. Each master has 48,829 pages × 1,024 = 50,000,896 weights, resident cache bounded at 32 pages. Exactly 64 KiB TRAIN (64 distinct SEC document prefixes) and 32 KiB DEV (32 disjoint document prefixes), per-report provenance SHA plus complete 10GiB corpus SHA. 128-byte window resets identical. Require *bitwise equality* of all three `paired_selection.tsv` manifests before interpreting any losses.

Report each arm's train and heldout bits/byte, exact physical distinct touched weights, optimizer counts, run time, capacity, source SHA and status. Validate hybrid sidecar with nonzero RTRL/BPTT windows and exactly 8×TRAIN predicted-byte targets. Expect 6 RTRL + 2 BPTT windows per 1024-byte report (last window shorter), but derive from actual counts, never invent benchmark values.

## Scientific limitations

HYB-1 is a **deterministic alternating schedule**, not an interpolated RTRL/BPTT gradient, learned mixture policy, two-stream model, or optimizer convergence proof. The BPTT arm still uses normalized 128-byte gradients and fewer update steps; the mix alone does not correct the previously measured optimizer-scale confound. The corpus is **NOT_ADMITTED** for production, and 2024 TEST stays **UNOPENED**. This does not establish multilingual generation, 25B trained weights, or independent out-of-time generalization.

Follow-up: isolate gradient reduction/step size using TRAIN-only tuning as [issue #593](https://github.com/Memorithm/SML-GENIUS/issues/593). Exact mathematical gradient blending/eligibility traces can be explored separately, not assumed from an alternating-window schedule.
