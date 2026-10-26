# SKY-R4 — Full GF(2) controllability is not persistent streaming memory

27 October 2026. Standalone exact-memory controls; **no physical skyrmion
dynamics, no inference or SBG/MOR integration.**

## Preregistered question and distinction

SKY-R3 shows a 75-site periodic Boolean Rule90 lattice reaches rank
75 with **one** input data bit per step if the injection site alternates
between sites 0 and 1. It is essential to distinguish:

1. **Finite-horizon reachability from zero:** after exactly 75 inputs
   starting from the known all-zero state, the linear 75×75 map has
   full rank and can be inverted to recover all input bits.
2. **Persistent streaming recall:** beyond the first 75 updates and
   without resets, the present state also includes contributions
   from older input history. A decoder that assumes a zero state
   75 steps earlier can be wrong even though the 75×75 map is full rank.

The old-state term is `A^75 s_(t-75)`. An exact independent packed
GF(2) calculation predicts `rank(A^75)=74` for this odd-length
Rule90 ring. Thus most old-state dimensions persist, preventing
general zero-history inversion of the most recent 75 input bits.

## Frozen exact design

- Two arms with 75 logical binary state bits:
  - **SHIFT75**: ordinary delay-line update `s'= ((s<<1)|input) & MASK`.
  - **ALT_RULE90**: periodic nearest-neighbor `s'_i = s_(i-1)
    XOR s_(i+1) XOR [i=t mod 2]*input`.
- One independent input bit per step in both arms. Rule90 needs
  a clock-phase site switch, whose cost is not accounted for here.
- SplitMix64 nonlinear deterministic data bit 32; four fixed seeds.
  Separate from SKY-R1 seeds. It is not a cryptographic PRNG.
- **Reset regime:** 512 separate 75-step windows per seed;
  zero-initialize both states at each window. At the terminal step,
  decode ages 1,7,31,63,74. Totals: 2,048 trials per age and arm.
- **Continuous regime:** no resets for 4,096 steps per seed.
  Evaluate ages 1,7,31,63,74 from time index 75 onward.
  Totals: 4 × (4096-75) = 16,084 trials per age and arm.
- Rule90 reader: compute its **full state-readout** by exact
  75×75 Gauss-Jordan GF(2) inverse of the 75 most recent impulse
  columns for either observation clock phase. No fitted weights or
  leakage from the evaluation streams. Readout uses parity of an
  entire 75-bit state, versus the Shift75 one-bit direct readout.
- Both readers are structurally known / hard-coded oracle decoders.
  Comparison is **equal logical state bits and input bandwidth**,
  NOT equal readout arithmetic, energy, area, switching or hardware
  cost.
- Pass/fail gates: both finite-window arms must decode 100% for all
  frozen ages. SHIFT75 must decode 100% on all continuous trials.
  ALT_RULE90 is hypothesized to have at least one continuous-stream
  error at each tested age; CI must confirm rather than assuming it.
  Exact rank of `A^75` must be 74, and both phase decoders must
  invert all 75 basis columns.

## Interpretation and stopping rules

Full controllability can make a system usable as a **resettable
block memory**, but this does not prove it is suitable as an
indefinitely streaming language-model memory without an additional
mechanism to track/erase the pre-existing state.

If streaming recall is degraded, do not claim "75 bits of
persistent useful memory" from the 75/75 controllability rank.
If no degradation occurs, inspect this interpretation and the test
before promoting. Compare ordinary shift registers, ANF/SBG
state, readout cost and finite block boundaries on identical tasks
before integrating any new primitive.

Nothing is a physically simulated skyrmion and no physical
fault-tolerance, latency or energy claim follows.

## Reproduction

```bash
cargo test --locked -p sml-core --example skyrmion_stream_recall
cargo run --locked -p sml-core --example skyrmion_stream_recall
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
```

SKY-R4 is stacked on the current SKY-R3 reference for review only.
SciRust, FLAT-ATTENTION and RemoteOps remain separate projects.

## Qualified deterministic results

Targeted workflow `40419992505` succeeded for implementation SHA
`5619606b7a90b0639868acb78c47bf32c12b084a`.
Each reset-window row scored exactly 2,048 samples. Each never-reset
row scored exactly 16,084 samples. Reported counts are actual
correct/total, not confidence intervals:

| Age | Shift75 reset | Shift75 stream | Alt-Rule90 reset | Alt-Rule90 stream |
|---|---:|---:|---:|---:|
| 1 | 2048/2048 | 16084/16084 | 2048/2048 | 8072/16084 |
| 7 | 2048/2048 | 16084/16084 | 2048/2048 | 8055/16084 |
| 31 | 2048/2048 | 16084/16084 | 2048/2048 | 8041/16084 |
| 63 | 2048/2048 | 16084/16084 | 2048/2048 | 8021/16084 |
| 74 | 2048/2048 | 16084/16084 | 2048/2048 | 8085/16084 |

The independent GF(2) test confirms `rank(A^75)=74`, meaning
the old-state term does not vanish after 75 updates. The reset decoder
cannot in general isolate the most recent 75 bits after prolonged
streaming. This is specifically the *finite-block inverse* readout
without additional state tracking; no impossibility for all recurrent
architectures or all decoders is asserted.

The equal logical state and input-bit rate control is deliberately
not an equal processing-cost control: Rule90 computes 75 local XORs
and the oracle readout evaluates up to 75 bit parities per output,
whereas Shift75 stores and directly indexes a recent bit.
