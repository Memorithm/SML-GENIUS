# DKV-2a-N13B — page-skew and cross-primary sharing observation

Status: retained current-stack Development observation under
`SML-DKV-2A-N13B-PAGE-SKEW-PREREGISTRATION.md`.

## Execution identity

- protocol PR: #194;
- protocol merge: `5de6346a9f77c58716a7dbbfc7a62c38b4772fc2`;
- pre-execution isolation-metric correction PR: #195;
- correction merge: `46ebd64838ddc635e19e7b6616fda6ce1b289d7a`;
- fixture freeze: `e0c1b64cb6c8912f07395205c73c4804789ef840`;
- executed code head: `554a9180f8e13015a5bc0ca5819c8738d6afcae2`;
- GitHub Actions run: `37372778887`;
- job: `111973797573`;
- runner: `tarek-scirust-arm64-01`;
- Rust 1.89.0;
- two release executions were byte-identical;
- retained output: `research/dkv-2a-n13b/CURRENT-REPLAY.csv`;
- manifest: `research/dkv-2a-n13b/MANIFEST.json`.

The retained manifest records `selection = RETAIN` and `promotion = BLOCKED`.

## Frozen populations

Every population contains 144 records per seed, with 16 seeds:

- BALANCED = `[15,15,15,15,14,14,14,14,14,14]`;
- N13A_EMPIRICAL = `[2,4,53,2,4,6,2,9,62,0]`;
- HEAVY_SKEW = `[116,4,3,3,3,3,3,3,3,3]`.

The retained q=3 composition, N11 two-bank resolver, N12 source/record/subject
verification and one-selected-bank-read rule are unchanged.

## K=1024 capability under skew

| Population | P1 base micro | P1 macro | P1 worst page | P2 secondary continuity | P3 bound override | P3 unbound continuity |
|---|---:|---:|---:|---:|---:|---:|
| BALANCED | 99.65% | 99.65% | 99.11% | 99.57% | 100% | 99.42% |
| N13A_EMPIRICAL | 98.65% | 99.30% | 96.88% | 98.74% | 100% | 98.96% |
| HEAVY_SKEW | 96.96% | 99.42% | 96.28% | 96.88% | 100% | 97.16% |

All preregistered capability thresholds hold. The heavily skewed arm therefore
does not invalidate the retained composition on this Development panel.

## Primary-page isolation and explicit sharing

For unshared BASE -> SWITCH queries at K=1024:

| Population | exact key equalities | mean |cosine| | max |cosine| | two-bit target agreement |
|---|---:|---:|---:|---:|
| BALANCED | 0 | 0.025089 | 0.132954 | 21.88% |
| N13A_EMPIRICAL | 0 | 0.025350 | 0.114817 | 21.92% |
| HEAVY_SKEW | 0 | 0.024921 | 0.098167 | 22.22% |

The two-bit target agreement is descriptive only, as preregistered. Isolation
is judged by the zero-equality and key-geometry criteria.

Explicit replicated cross-primary sharing succeeds in every population:
shared SWITCH recall is 576/576 = 100% at K=1024. Each resolved query still
performs exactly one selected-bank matrix read.

## Learned-transition controls

The retained high-count same-page controls 8->8, 2->2 and 7->7 are 32/32 exact
at K=1024.

For the cross-page controls 2->8, 8->2, 5->8, 0->7 and 7->3:

- unshared projected keys have zero exact equalities;
- mean absolute cosine ranges from 0.018313 to 0.035545;
- every explicitly shared control is 32/32 exact.

This supports the frozen contract: primary-page isolation by default, explicit
numeric replication only when sharing is declared.

## Identity and fail-closed behavior

For each population, all six deterministic non-VerifiedPresent controls execute
zero matrix reads. The declared unverified ORDERED override does not fall back
to PRIMARY.

Aggregate retained accounting:

- VerifiedPresent classifications: 76,864;
- Missing: 3;
- Conflict: 0;
- Unverified: 15;
- matrix reads: 76,864;
- PRIMARY reads: 73,408;
- ORDERED reads: 3,456;
- verified source segments: 76,864;
- verified source bytes: 1,348,744.

A successful matrix read, page population, transition frequency, key cosine or
Q12 magnitude does not alter provenance or bank selection.

## Frozen hypotheses

All ten preregistered hypotheses hold:

- H1 identity invariants — HOLDS;
- H2 balanced capability — HOLDS;
- H3 empirical-skew capability — HOLDS;
- H4 heavy-skew capability — HOLDS;
- H5 secondary continuity — HOLDS;
- H6 explicit secondary override — HOLDS;
- H7 default primary-switch isolation — HOLDS;
- H8 explicit cross-primary sharing — HOLDS;
- H9 learned-transition semantics — HOLDS;
- H10 no hidden source authority — HOLDS.

## Decision

Retain the q=3 two-bank N12-verified composition as the strongest current
Development memory-composition candidate.

This does **not** promote a core API, a learned replication policy, context
integration or model quality. N13B is a current-prototype stress panel.

The next model-core gate before any PageSeeded/two-bank API promotion is
physical resource qualification: measured latency, RSS, traffic and energy on
the retained composition. Those measurements must remain separate from logical
operation counts and from model-quality claims.
