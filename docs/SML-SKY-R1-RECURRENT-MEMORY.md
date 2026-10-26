# SKY-R1 — 75-bit local recurrent-memory falsification bench

26 October 2026. **Research, no inference integration.**

## Decision being tested

The earlier SKY-M0/M1/M2 references distinguish software lattice topology
from arbitrary binary faults and continuous geometric deformation.

SKY-R1 asks a separate, falsifiable question: with **75 logical mutable
state bits** and the **same trained one-bit probe family**, do simple
nearest-neighbor Boolean recurrences retain information more usefully
than an ordinary 75-bit shift register? A fixed topological-charge-sector
reference is included to show why an immutable Q bit cannot by itself
serve as writable stream memory.

**No model here is a skyrmion.** Rule 90 and rule 30 cellular automata are
purely Boolean local-recurrence ablations, not spintronics or a derived
skyrmion dynamical equation. The fixed sector arm is a software policy,
not physical error correction.

## Frozen protocol

- Five arms, all represented by 75 *logical* state bits:
  1. SHIFT75, nearest-neighbor delay-line register;
  2. LOCAL_RULE90, left XOR right on a 75-site periodic ring, with new
     input XOR-injected at site 0;
  3. LOCAL_RULE30, left XOR (center OR right) and the same injection;
  4. LAST_ONLY, last input at site 0, 74 reserved idle sites;
  5. FIXED_Q_SECTOR, original 25 octahedral spins serialized as 75
     logical bits, Q=-1 for 0 and Q=+1 for 1. An input requesting a
     different topological charge is *rejected*. Decoding always
     recomputes or validates the charge with SKY-M0/M1.
- States are actually Rust `[u8;75]` arrays (600 storage bits in the
  benchmark, plus model fields). Calling them 75 logical bits does not
  assert packed bytes, production RAM usage or equal arithmetic cost.
- No secret seeded tuning: two predefined training PRNG seeds, four
  disjoint testing seeds, deterministic nonlinear SplitMix64 with output bit 32,
  75-step zero-state warmup per independent sequence.
- Training length **1024** and test length **2048** per seed, of which
  75 steps are unscored. For each arm/task, the trained readout selects
  the best single state-bit index and optional inversion on training
  samples only. Ties break to the first index and non-inverted readout.
- Tasks: one-step COPY_1; delayed recall DELAY_7 / DELAY_31 /
  DELAY_63; and conditional readout CTX_1_OR_7, where a separate
  independent Boolean context selects which delayed input to recall.
  The context bit is explicitly **visible to the readout** and not
  stored by the dynamical reservoir. Two independent one-bit probes,
  one per context value, are fitted by the same training procedure.
  The contextual test is *not* proof of internal context memory.
- Report exact held-out accuracy (correct/total) and actual rejected
  input updates for the fixed-sector arm. No randomized train/test
  resampling, no online readout adaptation to test seeds.
- SHIFT75 must achieve perfect scores in all tasks. If it does not,
  the protocol has a leak, indexing bug or training failure.

## Reproducibility

```bash
cargo test --locked -p sml-core --example skyrmion_recurrent_panel
cargo run --locked -p sml-core --example skyrmion_recurrent_panel
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
```

## Precommitted decision / stop gates

**Do not promote** Rule90/Rule30 unless a future test with physically
motivated local interactions, bounded compute/energy and matched payload
exhibits held-out advantage over an appropriate conventional recurrent
or SBG/ANF memory. Failure against SHIFT75 under recall is expected
and must remain visible. A high-scoring readout on this isolated
synthetic benchmark does not establish language modeling ability.

The fixed-Q arm can retain an existing bit, but if its payload is
*encoded solely as Q*, a non-changing-sector write policy makes
opposite-bit writes impossible. This is a simple logical impossibility:
`Q_before == Q_after` and `bit=f(Q)` imply
`bit_before == bit_after`. Real memory within a topological sector
would need additional mutable degrees of freedom, with their own
protection, readout and energy/error analysis.

Next optional SKY-R2: design real sector-preserving **microstate**
encoding and test tasks with matched readout, memory access, latency
and entropy. This is not permission to integrate into SML-GENIUS.

No changes to the SBG/MOR/ANF runtime. SciRust and FLAT-ATTENTION
remain independent; RemoteOps is the control-plane reference.

## Superseded exploratory output (original GF2-linear PRNG)

Exact runner report: workflow `40414591151`, on implementation head
`8d2a2022681050ccbf4e7364f8ae209cfc50d050`. The targeted workflow
returned success, including repeated byte-for-byte report generation.
**This run is retained for audit only: it used a GF(2)-linear xorshift64
bitstream. Results are superseded by the post-review SplitMix64 experiment.
The linear generator was changed to prevent a future GF(2) readout from
exploiting generator predictability rather than actual temporal memory.**

Every held-out row contains 7,892 scored samples. The learned readout used
1,898 training samples per task/arm.

| Arm | Copy1 | Delay7 | Delay31 | Delay63 | Context1Or7 |
|---|---:|---:|---:|---:|---:|
| SHIFT75 | 7892 | 7892 | 7892 | 7892 | 7892 |
| RULE90 | 3912 | 3963 | 3931 | 3972 | 3929 |
| RULE30 | 3980 | 3966 | 4014 | 3901 | 3884 |
| LAST_ONLY | 4000 | 4003 | 3993 | 3983 | 3959 |
| FIXED_Q_SECTOR | 4000 | 4003 | 3993 | 3983 | 3970 |

The fixed-sector arm rejected 4,151 attempted writes across four held-out
episodes **per task**. No measured improvement relative to SHIFT75 occurs.
This particular single-probe experiment does not rule out recoverability
with more expressive multibit trained readouts; it rules out the asserted
benefit for the frozen one-bit probe family.

## PRNG methodological correction / new qualification gate

The current program now uses the published SplitMix64 integer-mixing
recurrence (increment constant 0x9e3779b97f4a7c15, finalizer multipliers
0xbf58476d1ce4e5b9 and 0x94d049bb133111eb), reading output bit 32.
This is deterministic and nonlinear over GF(2), not cryptographically
secure or a proof of statistical independence. The new head's exact CI
report, rather than the original xorshift64 table, is the primary source
of held-out accuracy. No claims of superiority are admissible without
requalifying the benchmark on the revised input stream.

## Qualified SplitMix64 output (current protocol)

Targeted workflow `40415736653`, successful, implementation SHA
`873a6b36f45d3b286747aa29f29fdbacb98b421b`. Frozen PRNG mixing
uses SplitMix64; see the methodological correction above.

Scored held-out samples: 7,892 per arm/task, 1,898 training samples.

| Arm | Copy1 | Delay7 | Delay31 | Delay63 | Context1Or7 |
|---|---:|---:|---:|---:|---:|
| SHIFT75 | 7892 | 7892 | 7892 | 7892 | 7892 |
| RULE90 | 4036 | 3952 | 3982 | 3999 | 3933 |
| RULE30 | 3864 | 3954 | 3886 | 3946 | 3932 |
| LAST_ONLY | 3984 | 3987 | 3991 | 3978 | 3952 |
| FIXED_Q_SECTOR | 3984 | 3987 | 3991 | 3978 | 3968 |

The fixed-sector arm rejects 4,138 requested sector changes in the four
test episodes **per task**. Both local CA families still score near chance
when the readout is restricted to one bit and optional inversion. The
restricted-readout result cannot exclude more complex Boolean/ANF readouts.
