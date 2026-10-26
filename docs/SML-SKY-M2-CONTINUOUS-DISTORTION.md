# SKY-M2 — Continuous-spin distortion versus six-axis quantization

26 October 2026. Status: *research-only*, no inference adoption.

## Question and frozen competing interpretations

SKY-M1 tests arbitrary changes in a hypothetical 75-bit serialization.
SKY-M2 instead applies deterministic bounded perturbations to a
**continuous-spin** (three `f64` coordinates/cell) grid and compares:

1. A spherical-solid-angle lattice topological charge computed before
   *any* six-axis quantization (Berg–Lüscher formula).
2. A lossy nearest-axis discretization into the SKY-M1 75-logical-bit
   serialization, decoded by the original SKY-M1 fail-closed oracle.

These are not matched-storage encodings: three `f64` coordinates per spin
require nominally 4,800 payload bits for 25 cells, versus 75 **logical**
serialized bits in the quantized candidate (excluding Vec allocation,
metadata and Rust layout). Comparison is diagnostic only, never evidence
of 75-bit error resilience, lower energy or higher density.

## Exact preregistered methods

- Two fixed 5×5 orientation textures, with charges -1/+1 and uniform +z
  boundary. The reference has 25 cells and 32 triangles.
- Perturb only the 3×3 interior, with
  `n'(x,y;a) = normalize(n(x,y) + a * p(x,y))`,
  where `p=(sin((x+1)*1.71 + y*.31), cos((y+1)*2.17 + x*.23),
  sin((x+y+2)*.73))` using integer cell coordinates x,y.
- Amplitudes frozen to [0, .10, .25, .50, .60, .75, 1.00].
- Spherical area of each oriented triangle (a,b,c):
  `Omega = 2*atan2(dot(a,cross(b,c)), 1+dot(a,b)+dot(b,c)+dot(c,a))`.
  Sum Omega/(4*pi), check distance from nearest integer <=1e-9.
- Fail closed on nonfinite/unnormalized spins, changes to prescribed boundary,
  antipodal shortest-geodesic ambiguity, numerically ambiguous area or
  noninteger charge.
- Six-axis control: quantize each normalized spin to nearest ±x/±y/±z with
  documented fixed axis tie-break, serialize 3 bits/spin in SKY-M1 order,
  decode with SKY-M1; report either useful payload or ERASURE.
- The test checks both charges at all amplitudes <=.75 and includes the
  targeted .60 quantization erasure. At 1.00, the -1 reference is expected
  to reach Q=0 on this *finite-lattice interpolation*. A continuous path in
  a well-defined non-singular topological sector would **not** change
  degree; this result must not be interpreted as physical spontaneous
  topological destruction without locating a geometric breakdown along
  the path.

## Provisional calculation, **not** physical results

An independent Python arithmetic check of these frozen fixtures yielded
Q=-1/+1 for 0 through .75. At amplitude .60 the continuous Q=-1 texture
remains Q=-1 but six-axis decoding reports ERASURE. At amplitude 1.00
the same finite triangulation yields Q=0, so a blanket claim of invariance
over arbitrary perturbations would be false. All numbers require the Rust
CI oracle to pass before being retained as implementation evidence.

The panel is a deterministic geometric software perturbation: **not**
Boltzmann noise, LLG spin dynamics, a thermal stability experiment,
a learned state machine, a token-memory task, or a hardware speed test.
No proof of general noise robustness or model advantage follows.

## Running and stopping

```bash
cargo test -p sml-core --example skyrmion_smooth_panel
cargo run -p sml-core --example skyrmion_smooth_panel
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

Stop if the fixture fails to reproduce across normal supported hardware
or if a hidden assumption equates topological charge with correction of
the actual underlying spin/payload state.

The next research candidate, not implemented here, is a continuously
sampled amplitude sweep detecting *the first geometric singularity*,
then a causal local-memory task against equal-useful-capacity SBG/ANF/
MOR controls. No changes to live SBG memory, routing or inference are
approved by this research reference.

## Sources

- B. Berg and M. Lüscher (1981), "Definition and statistical distributions
  of a topological number in the lattice O(3) sigma-model",
  *Nuclear Physics B* 190(2), 412–424,
  https://doi.org/10.1016/0550-3213(81)90568-X.
- See preceding SKY-M0 and SKY-M1 preregistrations for the representation
  and adversarial single-bit controls. This experimental code does not
  purport to reproduce physical device literature.

No change to SciRust or FLAT-ATTENTION; these remain independently
maintained projects. RemoteOps remains the sole remote control plane.
