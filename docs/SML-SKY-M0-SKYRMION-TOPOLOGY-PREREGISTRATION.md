# SKY-M0 — Skyrmion-inspired topological memory: preregistration

Date: 2026-10-26. Status: RESEARCH REFERENCE ONLY; no model-runtime integration.
Owner: SML-GENIUS model programme. RemoteOps remains the separate remote control plane.

## Scientific scope, with a strict hardware/software boundary

A magnetic skyrmion is a smooth spin texture whose continuum charge is the degree of a map S² -> S²:

Q = (1 / 4pi) integral n · (d_x n × d_y n) dx dy.

The equivalent fully summed Levi-Civita expression has factor 1/(8pi), not 1/(4pi).
The expression is an invariant only under the necessary continuity, boundary and non-singularity assumptions. Real devices can lose skyrmions by thermally activated transitions or edges; no bit is indestructible.

Presence/absence is a possible hardware binary encoding. It is **not** the same as
the scalar topological charge of an arbitrary software configuration.
A logic-in-memory device is not free of motion, nucleation, detection, clocking
or peripheral-energy costs. Gate designs based on skyrmion interactions have
been demonstrated in micromagnetic simulation, not universal transistor-free
commercial digital logic or a trained 25B model.

The initial programme must not claim zero bus cycles, Landauer-limit efficiency,
automatic resistance to adversarial attacks or a runtime acceleration on CPU/GPU.

## Mathematical reference: six-spin octahedron / Berg-Luescher restriction

For a triangular plaquette with three unit spins a,b,c the signed spherical
solid angle is

  Omega(a,b,c) = 2 atan2(a · (b × c), 1 + a·b + b·c + c·a).

Its lattice charge is Q = sum(Omega)/(4pi). The full Berg-Luescher treatment
requires a consistent triangulation and non-ambiguous geodesic interpolation.

For the **limited reference alphabet** {+x,-x,+y,-y,+z,-z}:
- A triangle with any opposite-axis pair is rejected as ambiguous.
- Three *different* axes produce Omega=sign(det(a,b,c))*pi/2.
- Triangles with repeated (equal) spins produce Omega=0.
- With a fixed uniform boundary, eight signed triangular octants constitute
  one charge unit: Q=sum(signed_octants)/8.
- A charge outside the integer sector, a nonuniform boundary, or an antipodal
  plaquette is refused rather than rounded to a convenient answer.
- This is an exactly integer-valued **restricted discrete charge oracle**,
  not a magnetic dynamical solver or a one-bit skyrmion representation.

Reference source:
B. Berg and M. Luescher (1981), Definition and statistical distributions of a
topological number in the lattice O(3) sigma-model,
Nuclear Physics B 190:412–424. DOI 10.1016/0550-3213(81)90568-X.

## SKY-M0 executable experiment

Location: `crates/sml-core/examples/skyrmion_octahedron.rs`.
Run:
```sh
cargo test -p sml-core --example skyrmion_octahedron
cargo run -p sml-core --example skyrmion_octahedron
```

The 5x5 reference texture has uniform +z boundary, -z center, and an equatorial
ring. It yields Q=-1; reflecting its winding yields Q=+1. A uniform field
yields Q=0. The test panel also includes one sector-preserving local rewrite,
one sector-changing rewrite refused **atomically**, and fail-closed malformed
configurations. The reference stores each spin as an enum byte; it is not
packed, SIMD-optimized or integrated with SBG state.

The write guard rejects a changed charge by **software policy**. It is not
physical topological protection: an attacker may alter the checker, a bitflip
may cause an ambiguous lattice, and no automatic correction occurs.

## Falsifiable programme and mandatory controls

SKY-M1, **only after SKY-M0 passes**:
- Define a deterministic mapping from an SBG micro-task into lattice states,
  including the exact index, decoding rule and storage overhead.
- Choose a frozen positive and negative panel: sector-conserving rewrites,
  intended sector changes, single and multi-bit faults, boundary disturbances,
  nonuniform reads, adversarially selected corruptions.
- Compare equal *useful* payloads against unprotected packed bits, parity,
  CRC/checksum-based detection and repetition/majority correction.
- Record undetected error rate, false rejection rate, **actual corrected
  errors**, and information recovered. Reject any claim that detecting a
  changed Q corrects the original payload.
- Report raw logical bits, physical bytes, grid metadata, cloned allocations,
  active operations, wall time, memory traffic and latency for every arm.
- Treat arbitrary-bit corruption as potentially destructive; do not assume
  topology conserves state under discrete arbitrary errors.

SKY-R1 (not implemented):
- Study causal local recurrent dynamics at bounded neighbour degree.
- Match state bytes, active operations and latency to existing SBG recurrence
  and ANF/MOR controls on frozen temporal tasks.
- Assess long-context retrieval and copy/ambiguity tasks, not only a
  self-created geometric classification target.

SKY-G1 (not implemented):
- Map fixed Boolean micro-operations (AND/OR/XOR) and selected ANF monomials
  onto a device-independent interaction oracle.
- Keep a physical-device model separate: no energy/throughput estimate without
  measured switching, readout, interconnect and clocking assumptions.
- Compare against existing packed Boolean kernels and direct-routing controls.

Stop conditions:
1. No held-out task improvement under identical useful information and budget.
2. Overhead exceeds a matched parity/CRC/majority reference.
3. An alleged 'invariant' changes on admitted transitions, or ambiguous
   configurations are silently coerced into an integer.
4. The algorithm requires dense global rescanning or all-history comparisons
   in the deployed path.
5. An apparent gain is solely due to changing task/data or different controls.

Promotion into SBG/MOR/DKV/Elastic Weights requires a **separate** preregistered
integration PR and physical validation. SKY-M0 itself changes no learned model
parameters, inference route, backend or memory semantics.

## Primary evidence (hardware relevance, not software speed claims)

- Y. Sun et al., Nature Communications 14, 3434 (2023),
  experimental skyrmion-enhanced physical reservoir:
  https://doi.org/10.1038/s41467-023-39207-9
- G. Beneke et al., Nature Communications 15, 8103 (2024),
  Brownian skyrmion reservoir, gesture classification:
  https://doi.org/10.1038/s41467-024-52345-y
- T. da Camara Santa Clara Gomes et al., Nature Electronics 8,
  204-214 (2025), neuromorphic weighted sums with skyrmions:
  https://doi.org/10.1038/s41928-024-01303-z
- J. Dhakshinamoorthy, H. Chhabra, A. K. Nayak, Nanotechnology
  (2025), simulated programmable logic gates:
  https://doi.org/10.1088/1361-6528/adf970
- T. N. G. Meier and C. H. Back, Journal of Physics: Condensed
  Matter (2026), thermal stability and finite lifetime review:
  https://doi.org/10.1088/1361-648X/aeb085

## Relationship to Memorithm projects

SML-GENIUS owns the research oracle and potential model integration.
SciRust remains an independent general-purpose AI/scientific framework:
a reusable octahedral/topology primitive can be *proposed* there only after
independent validation, rather than creating an automatic runtime dependency.
FLAT-ATTENTION remains an autonomous research project and is not merged with
SML-GENIUS. RemoteOps remains the reference self-hosted control plane; this
research PR does not modify it or require local services.
