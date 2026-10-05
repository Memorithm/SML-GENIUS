# DKV-2a-N13B — unbalanced page population and cross-page sharing protocol

Status: frozen before implementation/execution.

Parent evidence:
- `SML-DKV-2A-N13A-RUNTIME-CENSUS-OBSERVATION.md`;
- `SML-DKV-2A-N12-VERIFIED-IDENTITY-OBSERVATION.md`;
- `SML-DKV-2A-N11-TWO-BANK-OBSERVATION.md`.

## Pre-execution metric correction

This correction was frozen **before the first N13B execution**.

The original H7/H9 wording used <=10% exact+clear recovery of a two-bit target
as an isolation gate. That is statistically invalid: an independent two-bit
sign output has a 25% exact-match chance baseline.

No population, route, q value, write count, sharing subset or capability
threshold changed. Only the unshared cross-primary isolation criterion is
corrected to the preregisterable geometric property that the mechanism is meant
to provide: zero exact key collisions plus mean |cosine| <=0.10. Two-bit target
agreement is still retained and reported descriptively.

## Purpose

N11/N12 retained a q=3 two-bank Development composition with exact
record/subject/provenance preflight.

N13A then showed that current learned SML-1 routing is strongly unbalanced:
pages 8 and 2 carry 43.0555% and 36.8055% of learned trace records while page 9
is unused.

N13B asks two separate questions:

1. does the retained memory composition remain usable when page populations are
   unbalanced?
2. what is the explicit semantic contract for a record that is or is not
   intended to be shared across a **primary-page** switch?

N13B does not change q, projector weights, the two-bank resolver or N12
provenance semantics.

## Frozen population size

Every population arm contains exactly **144 unique semantic records per seed**.

This count is fixed because the N13A empirical vector reduces exactly by 128 to
144 integer units.

Seeds: 0..15.

Widths: K in {256,1024}.

Every record has:

- unique RecordRequest identity;
- unique SubjectKey;
- one verified raw ObservationEvent;
- deterministic SemanticState<64>;
- deterministic two-bit latent target;
- A initially false.

Write order is shuffled deterministically after page assignment.

## P0 — three frozen page-population arms

### BALANCED

Largest-remainder representation of uniform 1/10 population at total 144,
with page-index tie break:

`[15,15,15,15,14,14,14,14,14,14]`.

Maximum minus minimum population is one record.

### N13A_EMPIRICAL

Exact reduced N13A learned-task page vector:

`[2,4,53,2,4,6,2,9,62,0]`.

No smoothing is allowed. Page 9 remains empty.

### HEAVY_SKEW

Independent stress control, fixed without using N13A dominant pages or memory
outcomes:

`[116,4,3,3,3,3,3,3,3,3]`.

Page 0 therefore carries 80.5555% of records. The choice of page 0 is
intentional: it is not either N13A hot page 2 or 8.

No population count may change after first output.

## Frozen route construction

For primary page p:

- BASE(p) = [p, (p+1) mod 10];
- SECONDARY(p) = [p, (p+2) mod 10];
- SWITCH(p) = [(p+1) mod 10, p].

All tuples contain distinct PageIds.

BASE -> SECONDARY changes only a secondary expert.

BASE -> SWITCH changes the primary expert.

## Frozen memory/identity semantics

Exactly retain N11/N12:

- q=3: 3/4 PAGE_VALUE + 1/4 PAGE;
- ActiveOnly;
- PRIMARY_ANCHORED outer domain for PRIMARY;
- ORDERED_ROUTE outer domain for ORDERED;
- exact record/ordered-route override directory;
- stored write locator for source verification;
- independent RecordRequest + SubjectKey;
- complete VerifiedHistory and subject binding;
- Missing/Conflict/Unverified zero-read fail-closed behavior;
- one selected-bank matrix read only after VerifiedPresent preflight.

No retrieval strength, cosine or successful matrix read may mint source
presence or sharing.

## P1 — page-local base recall

For each population arm:

1. write all 144 records to PRIMARY under BASE(page);
2. query each record under the identical BASE(page);
3. verify source/record/subject before the matrix read.

Report:

- micro exact+clear recall;
- per-page recall;
- macro recall over nonempty pages;
- worst nonempty-page recall;
- hot-page recall;
- cold-page recall;
- provenance counts;
- matrix reads.

For N13A_EMPIRICAL, page 9 is reported as empty and excluded from macro/worst
nonempty-page calculations.

## P2 — secondary-route continuity under skew

Using the same page-local facts with no ORDERED override:

- query every record under SECONDARY(page).

PRIMARY must remain selected because the primary page is unchanged.

Report the same per-page/macro/micro metrics as P1.

## P3 — explicit secondary override under skew

In an independent world for each population arm:

- every fourth record by canonical record order receives an independent
  complement target in ORDERED under SECONDARY(page);
- the exact (record_id, SECONDARY(page)) binding is installed;
- all other records remain PRIMARY-only.

Queries:

- BASE(page) -> PRIMARY base target;
- bound SECONDARY(page) -> ORDERED override target;
- unbound SECONDARY(page) -> PRIMARY base target.

Report bound and unbound recall separately by population/page.

## P4 — primary-switch isolation is default

In an independent world:

- write each record only to PRIMARY under BASE(page);
- query the same record/state under SWITCH(page);
- retain the same independent RecordRequest/SubjectKey.

Source identity may remain VerifiedPresent because the requested record exists.

However there is **no declared cross-primary sharing write**.

Report:

- provenance;
- exact+clear recovery of the original two-bit target as a descriptive control;
- exact projected-key equality count;
- mean and maximum absolute cosine between BASE and SWITCH keys.

The two-bit exact-target rate is **not** an isolation gate: an independent
two-bit sign candidate has a 25% exact-match chance baseline. Primary-page
isolation is therefore judged geometrically before the matrix value is
interpreted.

This panel defines primary-page switch as isolated by default.

## P5 — explicit cross-primary sharing

For every fourth canonical record in an independent world:

1. write the same verified record/target to PRIMARY under BASE(page);
2. add a second PRIMARY numeric write under SWITCH(page) using the same verified
   record/source;
3. keep the original stored write locator as provenance authority;
4. query under both BASE(page) and SWITCH(page).

The second numeric write does not create a new source identity and does not
replace the original locator.

Report:

- base recall;
- shared-switch recall;
- additional writes per shared fact;
- matrix reads per resolved query.

This panel tests explicit replication as the only cross-primary sharing
mechanism in N13B.

It does not promote replication as a final learned policy.

## P6 — learned-transition controls

Freeze these N13A transition pairs:

High-count cells:
- 8 -> 8 (5040);
- 2 -> 2 (4096);
- 2 -> 8 (1920);
- 8 -> 2 (1920);
- 7 -> 7 (896);
- 5 -> 8 (720).

Low-count cross-page controls:
- 0 -> 7 (256);
- 7 -> 3 (256).

For same-page pairs, report ordinary page-local recall.

For cross-page pairs, run both:

- unshared: one BASE(from) write, query with primary=to;
- explicitly shared: add the second PRIMARY write for the to-domain before
  query.

Use 32 deterministic records per pair per K, independent of the population
arms.

## P7 — N12 fail-closed identity controls under skew

For each population arm at K=1024, execute deterministic controls for:

- nonexistent record;
- wrong subject;
- foreign stream;
- incomplete subject candidate set;
- source corruption;
- declared ORDERED override with failed verification.

Requirements remain unchanged:

- no non-VerifiedPresent path performs a matrix read;
- declared unverified ORDERED override never falls back to PRIMARY.

## P8 — accounting

Report per arm and page:

- PRIMARY writes;
- ORDERED writes;
- explicit sharing replication writes;
- directory probes;
- PRIMARY reads;
- ORDERED reads;
- verified source segments/bytes;
- final K.

Counterfactual diagnostics, if any, are separately labelled.

Logical work is not physical latency/RSS evidence.

## Frozen hypotheses

H1 — identity invariants: N12 source/record/subject classification and zero-read
fail-closed behavior hold in every population arm.

H2 — BALANCED base capability at K=1024: P1 micro recall >=90%, macro nonempty
page recall >=90%, and worst page >=80%.

H3 — empirical-skew capability at K=1024: N13A_EMPIRICAL P1 micro recall >=85%
and every nonempty page >=75%.

H4 — heavy-skew capability at K=1024: HEAVY_SKEW P1 micro recall >=75% and
dominant page-0 recall >=70%.

H5 — secondary continuity: at K=1024 P2 micro exact+clear recall is >=85% in all
three population arms and PRIMARY is selected for every query.

H6 — explicit secondary override: at K=1024 P3 bound ORDERED and unbound PRIMARY
queries are each >=85% exact+clear in every population arm.

H7 — default primary-switch isolation: at K=1024 P4 has zero exact BASE/SWITCH
key equalities and mean absolute BASE/SWITCH key cosine <=0.10 in every arm.
The two-bit target-agreement rate is descriptive only. VerifiedPresent
provenance is allowed and must be reported independently of latent recall.

H8 — explicit cross-primary sharing: at K=1024 P5 shared-switch recall is >=85%
in every population arm and each resolved query still performs exactly one
matrix read.

H9 — learned-transition semantics: at K=1024 every frozen cross-page pair has
zero exact unshared key equalities, mean absolute unshared key cosine <=0.10 and
>=85% explicitly-shared recall; same-page controls are >=90%.

H10 — no hidden source authority: no page population, transition frequency,
key cosine, Q12 magnitude or successful numeric read changes bank choice,
record/subject binding or provenance classification.

A failed hypothesis remains failed. No page counts, thresholds, sharing subset,
route construction or q value may change after first output.

## Selection rule

If H1-H10 all hold, retain:

- q=3 two-bank N12 verified composition;
- primary-page isolation by default;
- explicit replicated PRIMARY writes as the Development cross-primary sharing
  control.

This still does not promote a core API or automatic replication policy.

If a capacity gate fails, report the failed population regime. Do not increase K
or reduce writes inside N13B.

## Boundary

N13B is still a current-prototype stress panel.

It does not establish:

- future 25B page distribution;
- learned sharing policy;
- physical latency/RSS/traffic/energy;
- model-quality superiority.

After N13B, the remaining DKV model-core gate before any API promotion is
physical resource qualification of the retained composition. Learned model
quality remains governed by the staged SML scale gates.
