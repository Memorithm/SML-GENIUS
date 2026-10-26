# SKY-M1 — Frozen exhaustive software bit-fault controls

Date: 2026-10-26. Status: independent research arm; no inference integration.

## Falsifiable question
Does an octahedral-spin lattice encoding with an integer charge check provide
better protection for **one useful Boolean bit** than conventional encodings
under a fixed single-arbitrary-serialized-bit flip fault model?

No physical-device reliability or continuous-magnetization claim follows from
software-codeword flip tests.

## Freeze
- Payload: both Boolean values, equally represented.
- Encoders/decoders: RAW (1 bit), PARITY (2 bits), CRC8/ATM (9 bits),
  REPETITION3 (3 bits), REPETITION75 (75 bits), and SKY-OCTAHEDRAL (75 bits).
- For SKY, Q=-1 encodes 0 and Q=+1 encodes 1; five-by-five octahedral
  textures with uniform fixed boundary. Six orientations are serialized as
  three logical bits per cell. **75 logical codeword bits are NOT 75 physical
  skyrmion cells, nor are they packed Rust memory usage or device area.**
  The reference allocates byte vectors and performs a global charge scan.
- Exactly one bit is flipped at each codeword position for each payload.
  This is exhaustive, not a sampled hardware/thermal noise experiment.
- Outcome is **recovered payload** (value still correct), **detected erasure**
  (decoder refuses), or **wrong payload** (silent error). Successful decoding
  does NOT imply reconstruction of original spin texture.
- CRC8 is a detection-only decoder, repetition uses majority correction.
  The 75-bit repetition arm is the equal-serialized-bit-budget comparator.
- No timing, energy or quality improvement claim follows from this panel.

## Independent Python diagnostic (to be requalified by Rust CI)
| Arm | Coded bits | Trials | Recovered | Erasure | Wrong |
| --- | ---: | ---: | ---: | ---: | ---: |
| RAW | 1 | 2 | 0 | 0 | 2 |
| PARITY | 2 | 4 | 0 | 4 | 0 |
| CRC8 | 9 | 18 | 0 | 18 | 0 |
| REPETITION3 | 3 | 6 | 6 | 0 | 0 |
| REPETITION75 | 75 | 150 | 150 | 0 | 0 |
| SKY-OCTAHEDRAL | 75 | 150 | 8 | 142 | 0 |

The two SKY codewords differ in exactly two serialized bits. Their Hamming
distance is therefore 2, and no guarantee of correcting an *arbitrary*
single-bit flip is mathematically possible. The presence of detected erasures
does not change this bound. A topological invariant of a continuous field is
not an error-correcting code for arbitrary bit flips.

## Executable evidence
```bash
cargo test -p sml-core --example skyrmion_fault_panel
cargo run -p sml-core --example skyrmion_fault_panel
```

The frozen outcome assertions are not a quality benchmark, a real physical
noise model, or proof of speed. A success qualifies only the discrete oracle.

## Follow-up SKY-M2 (separate prerequisite and PR)
- Pre-register physically motivated smooth local spin disturbances and
  thermal/activation/reversal noise assumptions; do not recycle bit flips
  as a thermodynamic model.
- Use independent triangulation, edge/finiteness validity, repeated seeds,
  false-rejection and undetected-error metrics.
- Match useful payload, memory overhead, active reads/writes, energy assumptions,
  and latency to majority, parity, CRC and SBG/DKV reference controls.
- STOP generic bit-fault resilience claims where lower-cost majority repetition
  dominates. Do not change SBG/MOR/ANF inference semantics without held-out
  benefit and a separate integration gate.

## Sources
- B. Berg and M. Lüscher (1981), *Nuclear Physics B* 190:412–424,
  https://doi.org/10.1016/0550-3213(81)90568-X.
- J. Dhakshinamoorthy, H. Chhabra and A. K. Nayak (2025), *Nanotechnology*,
  https://doi.org/10.1088/1361-6528/adf970. This reports micromagnetic
  logic-gate simulations, not the benchmark represented here.

SciRust, FLAT-ATTENTION and RemoteOps remain separate projects. No external
control-plane code or runtime dependency is introduced.
