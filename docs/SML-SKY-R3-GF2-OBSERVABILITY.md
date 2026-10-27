# SKY-R3 — Exact GF(2) controllability of 75-bit local recurrence

27 October 2026. Status: research-only, no model runtime integration.

## Question

SKY-R1's trained **single-site** readout was close to chance on delayed
recall for periodic Rule90 and Rule30, while Shift75 was perfect. Can an
algebraic limitation explain why the Rule90 local recurrence has less
usable state diversity? The result must not be misreported as inability
to decode with all possible nonlinear readouts or for all stream tasks.

## Frozen exact model

- Width = 75 sites with periodic boundaries. Per step, previous state
  `s` is updated by Rule90:
  `s'_i = s_(i-1 mod 75) XOR s_(i+1 mod 75) XOR (input AND [i=0])`.
  This is the same logic as `LOCAL_RULE90` in SKY-R1.
- Shift75 control: `s'=(s<<1 | input)` with oldest bit discarded.
- Represent a state as the low 75 bits of a `u128`; compute the first
  100 input-impulse response vectors by injecting 1 at t=0 and 0
  thereafter.
- Binary Gaussian elimination, using only GF(2), determines the rank of
  the controllability (impulse response) columns for prefix lengths
  1,7,31,38,39,63,75,100.
- Exact reference claim to test:
  - Shift75 impulse rank grows to **75**;
  - Rule90 grows to **38** by 38 inputs and then saturates at 38.

## Why the limit exists

Define reflection R about the injected site i=0 by mapping site i to
(-i mod 75). The Rule90 transition commutes with this reflection and
the injection vector is itself reflection-symmetric.

Starting from the zero state, every reachable configuration is thus
reflection-symmetric. A 75-site ring has exactly one reflection-fixed
site (0) and 37 symmetric pairs, giving a subspace dimension
`1 + 37 = 38` over GF(2). No input sequence can expand reachability
beyond that 38-dimensional subspace; the executable test confirms
the upper bound is attained.

In contrast Shift75's impulse vectors are successive one-hot basis
vectors, so its linear controllability rank is 75.

**Crucial interpretation:** controllability rank is the dimension of
the state subspace reachable from zero under arbitrary input histories,
not a Shannon mutual-information estimate, decoder training result,
fault correction guarantee or physical memory capacity measurement.
Rank 38 does not itself prove that 38 arbitrary old input bits can be
read back from a stream, especially after histories longer than the
system's state dimension.

Rule30 is nonlinear over GF(2) and this linear rank calculation does
**not** apply to it.

## Tests

The Rust reference freezes:
1. The first 38 Rule90 impulses are independent, all later 100-column
   prefixes remain rank 38;
2. The first 75 shift-register impulses are independent;
3. Every Rule90 impulse is reflection-symmetric;
4. Packed u128 Rule90 updates agree for 150 consecutive steps with
   an independent unpacked 75-byte local-neighbor implementation;
5. Gaussian-elimination sanity fixtures.

```bash
cargo test --locked -p sml-core --example skyrmion_observability
cargo run --locked -p sml-core --example skyrmion_observability
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
```

## Decision

This is an analytic obstruction to **general full-rank controllability**
of that exact 75-site symmetric single-injection Rule90 design; it is
not a reason to reject all cellular automata, Boolean recurrences,
topological memory or more expressive ANF readouts. A next-step
experiment should compare more than one injection site or direction
as a controlled symmetry-breaking ablation while reporting input
bandwidth and complexity costs.

No change to inference, SBG, MOR or other projects. SciRust,
FLAT-ATTENTION and RemoteOps remain independent.

## Follow-up symmetry-breaking ablation: input channels matter

The same Rule90 transition is retained, but its forcing matrix is changed:

- **SINGLE**: one independent input at site 0, controllability rank 38.
- **DUAL-INDEPENDENT**: two independently addressable Boolean input
  channels at sites 0 and 1, each having its own independent impulse
  column at each time. The expected rank reaches **75 at 38 time
  steps**, subject to executable verification.
- **DUAL-TIED**: one Boolean input copied to both sites 0 and 1 on
  every step; its impulse response is the XOR of the two site
  responses. The expected reachable rank is **37**, not 75.

All ranks are over GF(2) and reflect *reachable linear state
dimension*, not accuracy, saved bits, latency, or energy. DUAL-INDEPENDENT
uses **twice the input bandwidth** of SINGLE or DUAL-TIED, so it cannot
be described as a free architectural improvement. All three use
75 logical state bits. The extra input-channel cost must be reported.

The three exact test claims above are preregistered before
using CI results and are tested by the same rank elimination oracle.
This isolates injection symmetry from the Boolean recurrence rule.

## More efficient symmetry-breaking: one alternating data channel

A third forcing scheme uses a **single data bit each step**, routed
deterministically to site 0 on even time steps and site 1 on odd
time steps (or the reverse parity). The update is still Rule90, but
the forcing matrix alternates in time.

For an observation after T updates, the aged impulse column of the
input from time t is `A^(T-1-t) e_(t mod 2)`. For either parity of
T, the exact 75-column controllability matrix is predicted to have
**full rank 75** over GF(2). The test checks every prefix length
1 through 75 for both possible phase choices and confirms the
impulse reconstruction against 75 chronological input updates.

This uses **one independent input data bit per step**, as in the
single-site experiment, not the two independent channels of the
dual-input experiment. The improvement is not entirely free:
the hardware/software source must route the bit alternately between
two sites using an available clock phase or equivalent steering.
That routing and its switching cost are unmeasured.

Full time-varying controllability means that a prescribed input
sequence of length 75 from a known initial zero state can reach
any one of 2^75 states at a chosen horizon. It does NOT prove
error-resilient long-term streaming recall, stable recovery after
an arbitrary prior history, or lower overhead than a 75-bit delay line.
An independent held-out recall task is still required.
