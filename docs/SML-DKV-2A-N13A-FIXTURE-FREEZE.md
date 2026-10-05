# DKV-2a-N13A — implementation fixture freeze

Status: frozen before first N13A execution.

Parent protocol:
`SML-DKV-2A-N13A-RUNTIME-CENSUS-PREREGISTRATION.md`.

## Stratum A details

Holdout generation is exactly:

`suite_from_case_id(10_000, 256)`.

Switch count for one task is the number of adjacent trace records whose
`selected_page` differs.

The longest same-page run is measured over each complete task trace and the
reported maximum is the maximum over the whole holdout.

The 10x10 transition matrix includes every adjacent trace pair. One-event
traces contribute no transition.

## Stratum B details

The chain generator, write ordering and q=3 PRIMARY key are copied
mechanically from the retained N11 example.

A free-running cycle is:

1. construct the current q=3 PRIMARY/R2 key from the current state;
2. perform one DeltaKvMemory read;
3. apply the retained q=3 threshold rewrite used by N11:
   - |value| <= 128 marks A=true and preserves V;
   - otherwise V becomes the read sign and A=false;
   - M and R are not modified.

A state transition is stable exactly when complete SemanticState equality holds
between adjacent states.

A-bin indexes are fixed:
0=0, 1=1, 2=2, 3=3-4, 4=5-8, 5=9-16, 6=17-32, 7=33-64.

The initial state is cycle 0. Six produced states are cycles 1..6.

## Stratum C details

The MOR fixtures, scores, registry shape, previous-route update and DKV-4
controller logic are copied mechanically from the retained
`dkv_4_dynamic_mor.rs` implementation.

The census transition category is defined solely from `PreviousRoute`:

- INITIALIZED: previous primary is absent;
- STABLE: previous primary equals the next primary;
- SWITCHED: both exist and differ.

This category is descriptive and is not the TemporalMorController transition
enum.

`route_switch` is true exactly for SWITCHED.

`noise_ppm`, disagreement, precision escalation, selected tuple and primary
page are read from the current `MorDecision` produced in that cycle.

The state-fixture compatibility invariant requires every dynamic run to end
with the same correct Boolean value as the retained DKV-4e panel.

## Output order

Rows are emitted in this order:

1. A task-kind summaries in Debug-name lexicographic order;
2. A page/address/transition/switch-distribution rows by numeric index;
3. B state rows by G, seed, chain, cycle;
4. B aggregate histogram/transition rows by G and numeric bin;
5. C cycle rows by fixture order from DKV-4e, state-fixture order, depth;
6. C aggregate rows in fixed numeric order;
7. H1-H6 invariant rows.

No implementation detail above may change after first N13A output.
