# SKY-M3 — Exact cached O(1)-neighborhood topological write admission

26 October 2026. Research-only; no SBG/MOR/ANF inference change.

## Prior bottleneck

The original SKY-M0 write guard clones an entire `width × height`
octahedral-spin lattice and recomputes its globally summed discrete
Berg–Lüscher charge for every proposed write. This is O(W×H) work
and O(W×H) temporary memory per write, which is incompatible with any
claim of cheap Boolean/topological memory access.

## Differential oracle

A cache is initialized only after a full, valid lattice charge scan.
It owns the field and its exact signed-octant count `S=8Q`.

For a proposed *interior* single-spin update at (x,y), the only affected
triangles lie in its **four adjacent grid squares**. The incremental
oracle evaluates at most 8 old and 8 proposed triangle octants. It adds
the integer difference to the cached `S`, rejects invalid/ambiguous
geometry, noninteger charges and sector changes, and commits the spin
and cache atomically only if `Q_after=Q_before`.

The candidate is checked before committing. Boundary changes are
rejected unless they are exact no-ops, preserving the original fixed
uniform boundary.

- Initial validation: **O(W×H) time**, once.
- Later interior single-spin admission: **O(1) time**, up to 16 exact
  `oriented_octant` evaluations independent of grid size.
- Candidate write: **O(1) additional memory** rather than full
  allocation/cloning. Cache adds one `i64` to owned field.
- No wall-time, CPU instruction count or energy result is asserted.
  Runtime/compile and physical costs require separate benchmarking.

## Frozen correctness comparisons

- For each of the two charges Q=-1 and Q=+1, for every grid cell in the
  5×5 reference, try each of the six axial spin replacements.
  Total **300 proposals**. Require the cached guard's exact
  `Result<i32,ChargeError>` to match the original full-scan guard.
  Require every committed/rejected state to match, and recomputed
  full-lattice signed octants to equal cached count.
- On a uniformly +z 129×129 field, perform an accepted central
  substitution that leaves Q=0, and reject a central antipodal
  substitution without cache corruption.
- All computations are exact integer octant operations; no numerical
  geometry tolerance is involved in this discrete variant.
- Known limitation: independently mutating a field behind the cache
  breaks its provenance; the field is owned privately and only
  modified by admitted operations.

## Reproduction

```bash
cargo test --locked -p sml-core --example skyrmion_octahedron
cargo run --locked -p sml-core --example skyrmion_octahedron
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
```

## Constraints and release gate

Only the *cost of checking charge conservation* is optimized. SKY-R2
shows that the same charge can hold opposite payload values while
a payload bitflip remains invisible. Therefore an O(1) topological
gate is **not** an error-correcting memory and does not authorize
model integration or physical resilience claims.

SciRust, FLAT-ATTENTION and RemoteOps remain separate. The research
example has no production runtime dependency.
