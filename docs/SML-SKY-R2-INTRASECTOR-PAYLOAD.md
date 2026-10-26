# SKY-R2 — Sector-preserving microstate payload, failure of charge-only protection

2026-10-26. Standalone reference; NO SBG/MOR or inference integration.

## Research problem

SKY-R1 established that encoding the useful payload solely in a conserved
topological charge Q cannot perform opposite-bit writes if the admission
rule forbids changing Q. SKY-R2 tests an alternative: keep Q=-1 but write
information into an additional spin configuration **within that sector**.

This is **not skyrmion physics**, not magnetic dynamics, and not an
error-correcting code unless verified independently on a specified fault
distribution.

## Exact construction

- 5x5 discrete octahedral lattice, fixed +z boundary, Q=-1.
- Two payload configurations are identical except for spin (1,1):
  - payload 0: Spin::Nx (enum code 0b001);
  - payload 1: Spin::Ny (enum code 0b011).
- In the three-bits/spin software representation the payload bit is
  `(5*1+1)*3+1 = 19` (zero indexed). Both configurations pass the exact
  lattice-charge check and have Q=-1. Their Hamming distance is **1**.
- A transactional write checks sector admissibility before overwriting the
  codeword; the experiment attempts 512 alternating writes, with immediate
  readback. This is a **zero-delay** microstate write test, not a long-horizon
  temporal-memory result.
- An exhaustive panel flips exactly one of 75 logical serialized bits for
  each encoded payload, making 150 deterministic fault cases.
  Classification: useful bit recovered, decoder erasure, or silent wrong
  payload; an erasure is never labeled as recovery.
- Equal-serialized-bit-budget control is repetition-majority with 75 bits
  and one flip. This is a coding-theory control only: the 25 six-way spin
  codes are held in byte-backed Rust arrays, not in an equivalent magnetic
  area or energy budget.
- No hidden tuning, no thermal/activation model and no inferred device
  reliability, model throughput or power comparisons.

## Pre-registered critical negative control

Flipping logical bit **19** turns one valid microstate payload into the
other without changing Q. Both inputs remain topologically admissible,
so a charge-only guard cannot detect either of these two faults.

Consequently the binary codeword distance is 1: there is **no guaranteed
single-bit error detection or correction** for this within-sector payload,
regardless of whether Q remains invariant.

The comparison is mathematically constructive: an invariant can protect
a class of transitions while leaving other degrees of freedom and their
encoded information unprotected. A storage application would need a
separate code or physical barrier protecting those degrees of freedom.

## Running

```bash
cargo test --locked -p sml-core --example skyrmion_intrasector_panel
cargo run --locked -p sml-core --example skyrmion_intrasector_panel
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
```

## Promotion gate

Stop proposals claiming intrinsic fault tolerance from Q alone. In a
future *physical* study, quantify actual transition rates between distinct
same-sector microstates, readout ambiguity, energy barriers and write
overheads under matched conditions. In a *software* study, evaluate
conventional ECC or sector-aware microstate codes at equal useful capacity,
state size and operational cost.

The code is stand-alone and does not change SML-GENIUS training,
memory, routing, or inference paths. SciRust, FLAT-ATTENTION and
RemoteOps remain independent.

## Qualified targeted-CI outcome

Workflow `40415248006`, implementation head
`04114c0f144d44b5a6b63cc233b49754b4afafb1`:

| Code | Logical bits | Hamming distance | Trials | Recovered | Erased | Wrong |
|---|---:|---:|---:|---:|---:|---:|
| Same-Q microstates | 75 | 1 | 150 | 4 | 144 | 2 |
| Majority repetition | 75 | 75 | 150 | 150 | 0 | 0 |

All 512 immediate write/read trials succeeded. The deliberate data-bit
fault was not caught by Q but decoded as the opposite payload. These are
software-lattice results only, without device-equivalent cost matching.
