# SML-GROK-ICM-4 — Feature-gated integration and end-to-end PVP6 bench (preregistration)

Status: PREREGISTERED (written and committed before any ICM-4 code).
Series: GROK-ICM-0..3 (#740–#743). Owner authorization: "Pousse et Continue … ICM-4".

## Question
Can the SECDED(72,64)+CRC-32 composite from ICM-2 be shipped as an opt-in
library module that protects real PVP6 gate coefficient pages end-to-end, with
zero silent output corruption for k ≤ 3 at-rest bit flips per page, and what
latency does it add?

## Integration contract (frozen)
- Cargo feature `icm` in `sml-core`, **default off**. Default build and
  `cargo test --workspace` are unchanged.
- `src/lib.rs` gains exactly one line: `#[cfg(feature = "icm")] pub mod icm_guard;`.
- `icm_guard::GuardedPage { new(words), scrub() -> ScrubOutcome, words() }`,
  `ScrubOutcome::{Clean, Corrected, Uncorrectable}`. Scrub repairs single-bit
  errors per word via SECDED, then verifies CRC-32 over the repaired page; any
  CRC mismatch or SECDED double detection is `Uncorrectable` and the page is
  left untouched.
- Dedicated workflow `.github/workflows/grok-icm.yml` runs fmt-independent
  `cargo clippy/test -p sml-core --features icm` and the bench.

## End-to-end bench `grok_icm_bench` (required-features = icm)
- Bank: 32 `Pvp6Gate<4096>` gates, 8 random distinct monomials each
  (SplitMix64 seed 4000 + trial), evaluated on all 4096 queries via
  `Pvp6Bank::evaluate_bounded_on(Selected, …)`.
- Per trial: flip k ∈ {1,2,3} distinct random bits in each gate's 64-word page.
  Trials: 64 per k (sealed seeds 4000..4064). Plus a k=0 panel.
- Arms: NONE (rebuild gates from faulted pages), ICM (scrub each page; an
  Uncorrectable page is refused = ERASURE for that gate's 4096 outputs).
- Outcome per gate: CORRECT (all 4096 outputs equal clean reference),
  ERASURE, SILENT (outputs differ, accepted). Bit-loss = wrong output bits
  accepted.
- Latency: median of 31 timed repetitions of (scrub all pages + rebuild +
  evaluate) vs (rebuild + evaluate) on the clean bank; reported, not hashed,
  no pass gate (host-dependent).

## Gates (PASS iff all)
1. k=0: both arms 100 % CORRECT, ICM 0 false rejects.
2. ICM: SILENT = 0 and bit-loss = 0 for k = 1, 2, 3.
3. ICM: k=1 100 % CORRECT (every single flip repaired).
4. NONE: SILENT > 0 for every k ≥ 1 (the guard is doing work).
5. Default build without the feature still compiles and tests (CI `rust` job).
Negatives are kept; no retuning after the TEST run.

## Results
(Filled only from executed runs.)
