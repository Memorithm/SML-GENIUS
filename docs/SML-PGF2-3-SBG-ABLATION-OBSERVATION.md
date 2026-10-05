# PGF2-3 — K=2048 SBG representation ablation observation

Status: retained negative component result under
`SML-PGF2-3-SBG-ABLATION-PREREGISTRATION.md`.

## Execution identity

- preregistration merged before execution in PR #198;
- protocol merge: `9e6b66cdb6c8d80d67c0a6fcc321b6b81cd8dd25`;
- execution PR: #200;
- executed scientific head: `dbf02474c9124df9f30f5625727eff727027c349`;
- CI base: `29b1d1fd09cf4588e49265517cdac3af7edd12a1`;
- CI synthetic merge: `877b00223cbdaa35a0a2949f0bc1ca1b4b846761`;
- GitHub Actions run: `37379128362`;
- job: `111996153340`;
- runner: `tarek-scirust-arm64-01`;
- Rust: 1.89.0, aarch64;
- two release-mode executions were byte-identical;
- retained output: `research/pgf2-3/CURRENT-REPLAY.csv`;
- manifest: `research/pgf2-3/MANIFEST.json`.

No MOR, Delta-KV, context or model API changed.

## Result

The global K=2048 Pascal-GF(2) transform is algebraically correct and reversible,
but it does **not** pass the preregistered SBG representation gate.

Selection:

**PGF2-4 = NONE.**

PGF2 remains a qualified reversible Boolean primitive. It is not selected as the
default K=2048 SBG representation and must not be handed to MOR or Delta-KV
under this protocol.

## Exactness and collisions

All four arms were inverted across both fixture families:

- inverse checks: 320;
- inverse failures: 0;
- full class collisions: 0 for every arm/family;
- full route collisions: 0 for every arm/family.

The structured low-12 direct-address diagnostic did not improve:

| Arm | excess low-12 class collisions |
|---|---:|
| IDENTITY | 30 |
| PERMUTE | 31 |
| XOR_PERMUTE | 31 |
| PGF2 | 30 |

BALANCED_RANDOM had zero low-12 excess collisions for every arm.

H1 holds.

## Structured-local Hamming geometry

The three matched controls are exact Hamming isometries and agree identically.

For class prototypes:

| Arm | minimum nearest-wrong | median nearest-wrong | mean |
|---|---:|---:|---:|
| controls | 32 | 32 | 32.00 |
| PGF2 | 4 | 12 | 15.25 |

The PGF2 median is therefore 62.5% smaller than the matched controls, rather
than at least 10% larger as required by H2.

For route prototypes:

| Arm | minimum nearest-wrong | median nearest-wrong | mean |
|---|---:|---:|---:|
| controls | 64 | 64 | 64.00 |
| PGF2 | 4 | 6 | 7.00 |

The route median contracts by 90.625%.

H2 is refuted.

## Structured-local uniform-noise retrieval

All three matched controls remain 100% correct at every frozen noise level.

PGF2:

| Input flips | class retrieval | route accuracy |
|---:|---:|---:|
| 0 | 512/512 = 100% | 128/128 = 100% |
| 1 | 497/512 = 97.07% | 125/128 = 97.66% |
| 4 | 481/512 = 93.95% | 124/128 = 96.88% |
| 16 | 392/512 = 76.56% | 102/128 = 79.69% |
| 64 | 217/512 = 42.38% | 73/128 = 57.03% |

Thus the reversible Pascal transform progressively amplifies sparse input
perturbations into the Hamming geometry used for retrieval/routing.

## Structured-local pair-boundary stress

Matched controls:

- 1/4 boundary: 512/512 correct;
- 1/2 boundary: 0/512 correct, 512/512 exact ties;
- 3/4 boundary: 0/512 correct.

PGF2:

- 1/4 boundary: 270/512 = 52.73%, with 120 ties;
- 1/2 boundary: 30/512 = 5.86%, with 240 ties;
- 3/4 boundary: 16/512 = 3.125%, with 120 ties.

PGF2 can break some exact midpoint ties that the isometric controls preserve,
but the isolated midpoint gain is accompanied by much larger regressions at
1/4 and across the uniform-noise panel. The preregistered no-regression clause
therefore prevents treating the tie-breaking effect as a positive retrieval
signal.

H3 is refuted. H4 is also refuted because no structured routing cell improves
over the 100% control baseline and several degrade materially.

## Balanced-random control

The clean random panel behaves as required by the sanity gate.

Class nearest-wrong median:

- matched controls: 978/2048;
- PGF2: 974/2048.

Route nearest-wrong median:

- matched controls: 998/2048;
- PGF2: 1002/2048.

Clean h=0 retrieval and routing remain 100% for every arm. The small clean
distance movement is consistent with a finite random fixture and does not
constitute semantic gain.

Under noise PGF2 again loses robustness; at h=64:

- class retrieval: 416/512 = 81.25% versus 100% controls;
- route accuracy: 110/128 = 85.94% versus 100% controls.

H5 holds.

## Complete single-bit stability

For IDENTITY, PERMUTE and XOR_PERMUTE every one-bit input perturbation remains
exactly one output bit: min=median=p95=max=1.

For PGF2 across all 2048 input coordinates:

- minimum amplification: 1 output bit;
- median: 48 output bits;
- exact mean: 177147 / 2048 = approximately 86.50 output bits;
- p95: 256 output bits;
- maximum: 2048 output bits.

The preregistered mean <=128 and p95 <=512 bounds hold, so H6 holds. However,
the comparison explains the functional degradation: PGF2 is reversible but is
not a Hamming isometry and a sparse input error can become a broad output
difference.

## Structural cost

At K=2048:

| Arm | coordinate maps | logical XOR updates | packed-word updates | scratch |
|---|---:|---:|---:|---:|
| IDENTITY | 0 | 0 | 0 | 0 B |
| PERMUTE | 2048 | 0 | 0 | 0 B |
| XOR_PERMUTE | 2048 | 2048 | 32 | 0 B |
| PGF2 | 0 | 11264 | 272 | 0 B |

All arms represent a 256-byte state/output. PGF2 matches its already-qualified
cost contract exactly, so H7 holds.

This is structural operation accounting only. PGF2-3 did not measure wall-clock
latency.

## Frozen hypotheses

- H1 exactness — **HOLDS**;
- H2 structured separation — **REFUTED**;
- H3 retrieval signal — **REFUTED**;
- H4 routing signal — **REFUTED**;
- H5 random-control sanity — **HOLDS**;
- H6 local-noise bound — **HOLDS**;
- H7 cost bound — **HOLDS**.

The preregistered selection requires H1, H5, H6, H7, H2 and at least one of
H3/H4. It therefore fails.

## Mechanistic conclusion

Invertibility is not the relevant property for SBG Hamming geometry.

The global lower-triangular Pascal/subset-zeta transform changes the weight of
difference vectors according to their binary-index structure. On the frozen
local structured fixtures, correlated class/route differences undergo
substantial XOR cancellation: the class median nearest-wrong distance falls
from 32 to 12 and the route median falls from 64 to 6.

At the same time, elementary input noise is expanded heterogeneously, with a
mean one-bit amplification of about 86.5 output coordinates and a p95 of 256.
That combination explains why exact clean retrieval is preserved while noisy
retrieval and routing deteriorate.

The retained conclusion is therefore narrow:

**global K=2048 PGF2 is not selected as an SBG representation mixer.**

Any later block-local or differently oriented Pascal construction is a distinct
candidate and requires a new preregistration. The failed PGF2-3 result must not
be overwritten or reinterpreted as a PGF2-4 authorization.
