# DKV-2a-N13A — prototype-native SBG/MOR census protocol

Status: frozen before implementation/execution.

Parent result:
`SML-DKV-2A-N12-VERIFIED-IDENTITY-OBSERVATION.md`.

## Purpose

N12 closes the synthetic memory-composition + exact identity/provenance gate for
the retained q=3 two-bank Development semantics.

The next unresolved question is distributional: the N1-N12 panels deliberately
used controlled states/routes. Before creating an unbalanced page/expert stress
panel, N13A measures what the **current SML prototype actually emits** on native
execution paths.

N13A is a census, not a claim that the current prototype is representative of a
future 25B model.

No memory policy, route policy or threshold is selected from this run.

## Three frozen strata

### A — learned SML-1 task-route traces

Use the current learned route candidate unchanged:

- `SmlTaskCandidate::train(TaskCandidateConfig::default())`;
- holdout = `suite_from_case_id(10_000, 256)`;
- all task instances and all event trace records are retained;
- no fixed-route control substitutes for the learned router.

Report:

- tasks and exact tasks by TaskKind;
- total trace records;
- page-count vector for PageId 0..9;
- nonzero pages;
- maximum page share ppm;
- minimum nonzero page share ppm;
- max/min nonzero imbalance ratio in ppm;
- route-address counts for all 16 four-bit addresses;
- selected-page transition matrix 10x10;
- self-transition fraction ppm;
- number of route switches per task distribution;
- longest same-page run.

This stratum measures current learned routing population only. It has no
SemanticState ambiguity field.

### B — retained q=3 recurrent SBG memory stream

Replay the current N11/N12 q=3 PRIMARY recurrent chain geometry without
provenance mutation:

- SemanticState<64>;
- K=1024;
- G in {4,16};
- seeds 0..15;
- fixed R2=[5,7];
- current ActiveOnly + PRIMARY q=3 key;
- six free-running recurrent rewrite cycles;
- same historical chain generator and write order as N11;
- no ORDERED overrides.

For the initial state and every produced recurrent state report:

- V/M/A/R popcount;
- A popcount histogram bins
  {0,1,2,3-4,5-8,9-16,17-32,33-64};
- share A=0;
- share A<=16;
- share A>16;
- per-cycle A mean/max;
- transition counts between A histogram bins;
- V Hamming delta from prior cycle;
- route field delta from prior cycle;
- stable-state fraction.

This stratum measures ambiguity produced by the current numeric/rewrite path on
the retained recurrent-memory proxy. It is not a learned 25B state
distribution.

### C — native dynamic-MOR control stream

Reuse the already-qualified DKV-4e dynamic MOR fixture identities unchanged:

- CONSENSUS;
- RESOLVING_MARGIN;
- PERSISTENT_MARGIN;
- STRUCTURAL_DISAGREEMENT;

crossed with:

- STABLE;
- CORRECT_ONCE.

Run the current native MOR + recurrent evidence path exactly as the retained
DKV-4e implementation does.

Report per cycle:

- selected expert count;
- selected expert tuple;
- primary page;
- route switch;
- structural_disagreement_ppm;
- noise_ppm;
- request_precision_escalation;
- MOR transition category;
- recurrent depth;
- semantic V/M/A/R delta.

Aggregate:

- route-length histogram;
- primary-page counts;
- route-switch fraction;
- disagreement histogram
  {0,1-49k,50-249k,250-499k,500k-1M};
- precision-escalation fraction.

This is a controlled native-MOR stress stratum, not an empirical deployment
distribution.

## Frozen output identity

The harness must print all stratum rows plus one aggregate summary in stable
lexicographic order.

CI executes the release harness twice and requires byte-identical output.

## Frozen invariants

H1 — learned-task integrity: every SML-1 holdout task remains exact and trace
length equals event count.

H2 — learned-route completeness: all learned-task trace records are accounted
for exactly once in the page vector and 10x10 transition matrix.

H3 — recurrent accounting: every N13A-B state appears in exactly one ambiguity
histogram bin and every adjacent state pair in exactly one transition cell.

H4 — no hidden ambiguity rewrite: M and R remain unchanged by the retained
Delta-KV rewrite kernel on the fixed-R2 chain proxy; any violation invalidates
the census.

H5 — dynamic-MOR fixture compatibility: all four retained MOR fixture contracts
and both state-fixture contracts reproduce before aggregation.

H6 — byte-identical determinism: two release executions are identical.

A failed invariant invalidates the census. No output-dependent fixture or bin
changes are allowed.

## Decision rule for N13B

N13A does not select a core policy.

After N13A is merged, N13B must be preregistered separately using the retained
census values.

N13B must include:

1. a uniform-page control;
2. an empirical page-population arm derived exactly from the N13A learned-task
   page vector;
3. an explicit heavy-skew control fixed independently of performance outcomes;
4. route-switch/cross-page sharing controls;
5. q=3 PRIMARY/ORDERED two-bank + N12 verified identity semantics unchanged.

No N13B threshold may be chosen to make the observed N13A distribution pass.

## Boundary

N13A establishes only current-prototype distribution evidence.

It does not establish:

- future 25B representativeness;
- natural-language model quality;
- core two-bank promotion;
- physical latency/RSS/energy;
- safe learned self-modification.

Model promotion remains blocked.
