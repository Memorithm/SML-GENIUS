# DKV-2a-N9 — canonical multi-expert domain observation

Status: current-stack Development experiment under
`SML-DKV-2A-N9-MULTIEXPERT-PREREGISTRATION.md`.

## Execution identity

- protocol merged before execution in PR #181;
- executed scientific head: `9c1a7292a65a7f0f14c2c521d53ac4472173cb4c`;
- GitHub Actions run: `37286037221`;
- job: `111685051480`;
- runner: `tarek-scirust-arm64-01`;
- Rust 1.89.0;
- two release-mode executions were byte-identical;
- retained output: `research/dkv-2a-n9/CURRENT-REPLAY.csv`;
- manifest: `research/dkv-2a-n9/MANIFEST.json`.

No core projector or Delta-KV memory implementation changed. Both policies
construct the selected N8 q=3 fixed-width hybrid and pass the complete
`MorDecision` into both current-library source projectors.

## Singleton compatibility

All singleton assertions pass for K=256 and K=1024.

For R1=[5]:

- PRIMARY_ANCHORED reproduces the N8 q=3 singleton key;
- ORDERED_ROUTE is explicitly defined to collapse to the same singleton
  construction and also reproduces N8;
- repeated projections are deterministic;
- complete selected-expert evidence is retained;
- duplicate selected experts remain rejected by the current projector.

H1 holds.

## PRIMARY_ANCHORED preserves secondary-route continuity

At K=1024, changing only secondary experts leaves the hybrid key almost
unchanged:

| route change | mean |cosine| | exact-key equalities / 3200 |
|---|---:|---:|
| R2=[5,7] -> R3=[5,8] | 0.992789 | 212 |
| R2 -> R4=[5,7,9] | 0.997123 | 881 |
| R2 -> R5=[5,9,7] | 0.993188 | 180 |

A memory written under R2 is recovered under secondary-route drift with:

- R2 query: 256/256 exact+clear;
- R3 query: 256/256;
- R4 query: 256/256.

The same qualitative result is already present at K=256: R3 continuity is
254/256 and R4 continuity 255/256.

H2 holds strongly.

The consequence is equally clear: secondary-route changes do **not** isolate
independent memories under PRIMARY_ANCHORED. When the same semantic state is
assigned independent values under R2 and R3, K=1024 conflict recall is exactly
256/512 = 50%.

## Primary switch is isolated

Changing the primary expert does change the outer domain.

For PRIMARY_ANCHORED R2=[5,7] -> R6=[7,5]:

- K=1024 mean |cosine| = 0.024578;
- exact-key equalities = 0/3200;
- same-state R2/R6 conflict recall = 512/512 = 100%.

At K=256 the corresponding conflict recall is 478/512 = 93.36%.

H3 holds.

## ORDERED_ROUTE provides full route isolation but destroys secondary continuity

ORDERED_ROUTE conditions the outer domain on the complete ranked route tuple.

At K=1024:

- R2->R3 mean |cosine| = 0.025138;
- R2->R4 = 0.025030;
- R2->R5 = 0.025089;
- R2->R6 = 0.025090.

R2/R3 same-state conflict recall is 512/512 = 100%, so H4 holds.

But a fact written under R2 is recovered under R3 or R4 in **0/256** cases.
H5 therefore also holds, and by preregistration this is negative evidence
against using the full route tuple as a transparent continuation domain.

The two extreme policies reproduce the expected semantic tradeoff:

- PRIMARY_ANCHORED: secondary continuity, weak secondary isolation;
- ORDERED_ROUTE: secondary isolation, zero secondary continuity.

## q=3 recurrent capability survives multi-expert R2

PRIMARY_ANCHORED with fixed R2=[5,7]:

| K | G | F6 | ORACLE |
|---:|---:|---:|---:|
| 256 | 4 | 98.44% | 98.44% |
| 1024 | 4 | 100% | 100% |
| 256 | 16 | 55.47% | 60.55% |
| 1024 | 16 | 92.97% | 92.97% |

At K=1024/G=4 teacher-forced advancing reads are all exact+clear:

- h0: 48/48;
- h1: 32/32;
- h2: 16/16.

At K=1024/G=16:

- h0: 177/192 = 92.19%;
- h1: 119/128 = 92.97%;
- h2: 64/64 = 100%.

H6 and H7 hold.

ORDERED_ROUTE gives nearly identical chain capability for a fixed R2 route,
which is expected because the route does not change during the chain panel.

## Local V robustness is retained

Under fixed R2 at K=1024, one-bit V perturbations give:

- PRIMARY_ANCHORED mean |cosine| = 0.252650;
- exact+clear next-value recovery = 26/96 = 27.08%.

ORDERED_ROUTE is similar for a fixed route:

- mean |cosine| = 0.249339;
- exact+clear = 26/96.

H8 holds for PRIMARY_ANCHORED and remains consistent with the N8 q=3
continuity tradeoff.

## H9 fails: exact collisions remain in secondary-route conflicts

The decisive N9 failure is exact written-key collision under
PRIMARY_ANCHORED when the **same semantic state** is written under R2 and R3.

Retained collision counts in the secondary-route conflict panel:

- K=256: 30 exact collisions;
- K=1024: 11 exact collisions.

No collision occurs in:

- PRIMARY_ANCHORED primary-switch R2/R6 conflict cells;
- PRIMARY_ANCHORED continuity or recurrent-chain cells;
- any ORDERED_ROUTE cell.

These collisions are not accidental cross-state aliases. They arise because
PRIMARY_ANCHORED deliberately keeps the same outer domain for secondary-route
changes, while the current MOR bucket contribution is sometimes too small to
change the final quantized q=3 key at all.

H9 is therefore **REFUTED**.

## Frozen hypotheses

- H1 singleton compatibility — HOLDS;
- H2 PRIMARY secondary continuity — HOLDS;
- H3 primary-switch separation — HOLDS;
- H4 ORDERED secondary isolation — HOLDS;
- H5 ORDERED secondary brittleness — HOLDS;
- H6 PRIMARY multi-expert chain capability — HOLDS;
- H7 PRIMARY teacher-forced capability — HOLDS;
- H8 PRIMARY Hamming-1 robustness — HOLDS;
- H9 no exact written-key collisions — **REFUTED**.

Per the preregistered selection rule:

**canonical multi-expert candidate = NONE.**

## Mechanistic conclusion

N9 isolates a second interpolation problem.

The q=3 value-domain hybrid solved the within-page V-crosstalk/continuity
tradeoff. Multi-expert routing exposes an analogous outer-domain tradeoff:

- anchoring only on primary preserves secondary-route continuity but leaves
  route conflicts partly aliased, including exact key collisions;
- conditioning on the full ordered route removes conflicts but makes every
  secondary route change a categorical domain change.

The next bounded Development candidate should therefore interpolate between
PRIMARY_ANCHORED and ORDERED_ROUTE at **fixed final K**, rather than retuning
MOR weights or silently choosing one endpoint.

A natural N10 panel is a deterministic coordinate partition between:

- the PRIMARY_ANCHORED q=3 key; and
- the ORDERED_ROUTE q=3 key.

A small frozen fraction sweep can test whether enough ordered-route coordinates
eliminate exact secondary-route collisions and raise conflict isolation while
retaining non-zero secondary continuity, q=3 chain capability and Hamming-1 V
robustness.

No core domain API, controller, context or model promotion follows.
