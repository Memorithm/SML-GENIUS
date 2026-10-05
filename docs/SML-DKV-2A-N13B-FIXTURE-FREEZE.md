# DKV-2a-N13B — implementation fixture freeze

Status: frozen before first N13B implementation execution.

Parent protocol:
`SML-DKV-2A-N13B-PAGE-SKEW-PREREGISTRATION.md`.

## Population assignment

Each arm has exactly 144 canonical record positions.

Page arrays are expanded from the frozen vectors and then permuted with the
deterministic xorshift stream tagged by:

`(seed, population_index, 13, 1300)`.

This changes which record receives which page but never changes per-page
population counts.

A second independent deterministic permutation tagged by:

`(seed, population_index, panel_id, 1399)`

defines matrix write order for each panel.

## Record/state/source identity

Per seed/population:

- stream id = `1 + population_index`;
- record id = `seed*1000 + index + 1`;
- subject = `SubjectKey(record_id, 0)`;
- one ObservationEvent and one raw segment per record;
- A starts all false;
- V/M/R and the two-bit target are deterministic xorshift outputs;
- the raw ObservationEvent bits equal the base latent target bits.

All three population arms use the same state/target generator for a given
seed/index before page assignment.

## Memory/storage bounds

Each bank uses:

- max_writes = 256;
- max_sources_per_write = 4;
- max_source_links = 512;
- max_ledger_segments = 160;
- max_ledger_semantic_links = 512;
- max_element_bytes = 32 MiB;
- max_staging_bytes = 32 MiB.

Verification/history/subject bounds admit at most 160 segments/events, 256
write records and 2 MiB raw bytes.

These are logical admission bounds, not physical RAM claims.

## P3 override subset

Canonical records with `index % 4 == 0` receive the frozen ORDERED
SECONDARY override.

The override target is the Boolean complement of the two-bit base target.

## P5 explicit sharing subset

Canonical records with `index % 4 == 1` receive the second PRIMARY numeric
write under SWITCH(page).

The original BASE write locator remains the record's provenance locator.
The replication write does not replace it.

## P6 learned-transition routes

For a frozen transition pair `from -> to`:

- source route = BASE(from);
- destination route = BASE(to).

There are exactly 32 deterministic records per pair and K.

For same-page pairs, only ordinary same-page recall is measured.

For cross-page pairs:

- UNSHARED writes only source BASE(from);
- SHARED writes source BASE(from) plus a second PRIMARY numeric write under
  BASE(to) while retaining the source locator.

The eight pairs and their N13A counts remain exactly those listed in the parent
protocol.

## P7 negative controls

For each population label at K=1024, the identity controls use the first
canonical record/page in that population world.

The incomplete-subject control uses an independent two-version subject fixture
with the second version stored behind a different PRIMARY key.

The declared-override verification-failure control installs one R3-like
SECONDARY override for the selected record and then corrupts its source bytes.
No fallback is implemented.

No fixture detail may change after the first N13B output.
