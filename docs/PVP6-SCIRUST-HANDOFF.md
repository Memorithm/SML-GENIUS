# PVP6 SML → SciRust handoff

Status: the SML-owned fixture contract is frozen and the SciRust structural
consumer is merged in SciRust PR #1569.

## Frozen artifact

- Contract: `docs/SML-PVP6-ANF-FIXTURE-CONTRACT.md`
- Fixture SHA-256:
  `feb1a144e23d9a29439b7cad6704f56637a7f7ead1a9b9abb39ad9c0ac5325fd`
- Geometry: K=2048, G=16, 4 densities, 3 schedules.
- Qualification: SML remains the independent semantic oracle.

Generate the fixture from the pinned SML source with:

```bash
cargo run -p sml-core --example pascal_boolean_pb8_gate_bank -- pvp6-fixture \
  > pvp6_fixture.csv
sha256sum pvp6_fixture.csv
```

The produced file must match the frozen SHA before it is transferred to
SciRust or used for timing.

## Destination check

SciRust provides the dependency-free structural checker:

```bash
cargo run -p scirust-simd --example pvp6_fixture_check -- \
  --fixture pvp6_fixture.csv
```

A successful result is only a structural admission record. It does not replace
SML's independent truth/task oracle and does not establish a SIMD/GPU winner.

## Timing gate

After both checks pass, the identical file may be supplied to the preregistered
native campaign. Native Thor timing, conversion costs and cross-repository
results remain pending until an admitted RemoteOps execution produces raw,
independently verified evidence.
