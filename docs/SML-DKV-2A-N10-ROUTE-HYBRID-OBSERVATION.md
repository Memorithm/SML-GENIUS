# DKV-2a-N10 — route-domain hybrid observation

Status: current-stack Development experiment under
`SML-DKV-2A-N10-ROUTE-HYBRID-PREREGISTRATION.md`.

## Execution identity

- protocol merged before execution in PR #183;
- protocol merge: `ade47eef1e198df5da29708a031fea56efca8f62`;
- executed scientific head: `82473950cb7c045d544f2d0ddf4b0ad428625c60`;
- GitHub Actions run: `37288126527`;
- job: `111691835985`;
- runner: `tarek-scirust-arm64-01`;
- Rust 1.89.0;
- two release-mode executions were byte-identical;
- retained output: `research/dkv-2a-n10/CURRENT-REPLAY.csv`;
- manifest: `research/dkv-2a-n10/MANIFEST.json`.

No core projector or Delta-KV memory implementation changed. q=3 remains fixed.
N10 changes only the harness-level coordinate partition between the two N9
route-domain endpoint keys.

## Endpoint controls

At K=1024 the endpoints retain the N9 semantics:

- r=0 / PRIMARY_ANCHORED:
  - R2->R3 continuity 256/256;
  - R2->R4 continuity 256/256;
  - R2/R3 conflict recall 256/512 = 50%;
  - R2/R3 exact conflict-key collisions: 16 in this deterministic panel;
- r=4 / ORDERED_ROUTE:
  - R2->R3 continuity 0/256;
  - R2->R4 continuity 0/256;
  - R2/R3 conflict recall 512/512 = 100%;
  - exact conflict-key collisions: 0.

Primary-switch R2->R6 remains isolated at both endpoints.

H1 holds.

## Geometry interpolates smoothly

At K=1024, R2->R3 mean absolute cosine:

| r | ORDERED fraction | mean |cosine| |
|---:|---:|---:|
| 0 | 0% | 0.992904 |
| 1 | 25% | 0.744138 |
| 2 | 50% | 0.495100 |
| 3 | 75% | 0.245802 |
| 4 | 100% | 0.025154 |

R2->R4 follows the same monotone pattern:
0.997019, 0.747012, 0.497474, 0.248215, 0.024809.

H2 holds.

## Continuity and conflict isolation do not share an interior operating point

K=1024 secondary-route behavior:

| r | R2->R3 continuity | R2->R4 continuity | R2/R3 conflict recall | exact conflict collisions |
|---:|---:|---:|---:|---:|
| 0 | 100% | 100% | 50.00% | 16 |
| 1 | 99.61% | 100% | 50.00% | 0 |
| 2 | 92.58% | 89.45% | 55.47% | 0 |
| 3 | 7.42% | 3.13% | 98.83% | 0 |
| 4 | 0% | 0% | 100% | 0 |

Continuity decreases monotonically and conflict recall increases monotonically,
so H3 and H4 hold.

But the frozen route gate requires simultaneously:

- conflict recall >=85%;
- R3 continuity >=20%;
- R4 continuity >=20%;
- zero exact conflict collisions.

No interior r satisfies all four.

The key transition occurs between r=2 and r=3:

- r=2 preserves high continuity but conflict isolation remains only 55.47%;
- r=3 essentially solves conflict isolation but destroys secondary continuity.

H5 is therefore **REFUTED**.

## Removing exact collisions is not sufficient

The r=1 arm is an important negative control.

At K=1024 it removes all exact R2/R3 conflict-key collisions while retaining
99.61% R3 continuity, yet conflict recall remains exactly 50%.

Therefore the N9 exact collisions were a visible failure mode, but they were
not the complete cause of route-conflict interference. Strong key correlation
alone is sufficient to keep the two independent values coupled.

## Capability and local V robustness are not the limiting gates

Every interior K=1024 arm retains the selected q=3 mechanism's low-load
capability:

- r=1/r=2/r=3: G4 F6 = 64/64 = 100%;
- all G4 teacher-forced h0/h1/h2 reads are 100% exact+clear.

High-load K=1024 F6:

- r=0: 92.97%;
- r=1: 91.80%;
- r=2: 92.58%;
- r=3: 90.23%;
- r=4: 92.97%.

One-bit V perturbation exact+clear recovery:

- r=0: 26/96 = 27.08%;
- r=1: 27/96 = 28.13%;
- r=2: 29/96 = 30.21%;
- r=3: 28/96 = 29.17%;
- r=4: 26/96 = 27.08%.

Thus r=1, r=2 and r=3 all satisfy the frozen capability/Hamming-1 thresholds.
H6 is refuted only because **no arm first satisfies H5**, not because the
recurrent or V-robustness gates fail.

## Primary-switch isolation remains stable

For every K=1024 r arm:

- R2/R6 conflict recall is 512/512 = 100%;
- R2->R6 mean |cosine| stays near 0.025.

The unresolved tradeoff is specifically secondary-route semantics, not primary
page separation.

## Logical work

For K=1024 every r executes the same matched Development work for 16 hybrid-key
constructions:

- semantic-coordinate reads: 16,384;
- bucket accumulations: 12,416;
- quantized source coordinates: 65,536;
- final key width: 1,024.

H8 holds.

The harness intentionally computes both q=3 endpoint keys for every r. This is
matched logical accounting, not a claim about an optimized runtime.

## Frozen hypotheses

- H1 endpoint compatibility — HOLDS;
- H2 secondary cosine monotonicity — HOLDS;
- H3 secondary continuity monotonicity — HOLDS;
- H4 secondary conflict monotonicity — HOLDS;
- H5 interior route tradeoff — **REFUTED**;
- H6 interior capability — **REFUTED by H5 precondition**;
- H7 selected-candidate collision gate — **REFUTED because no candidate is selected**;
- H8 equal logical work — HOLDS.

Selection:

**route fraction = NONE.**

## Mechanistic conclusion

N10 shows that a single fixed-width key cannot recover the requested secondary
semantics merely by coordinate interpolation between PRIMARY_ANCHORED and
ORDERED_ROUTE on this panel.

The geometry interpolates smoothly, but associative behavior exhibits a sharp
regime change: the route-specific component is too weak to isolate independent
secondary-route values at r<=2, while r>=3 is strong enough to make secondary
route drift effectively a different address.

The next useful design should not be another finer fraction sweep. It should
separate the two semantic roles:

1. a shared PRIMARY_ANCHORED retrieval path for transparent secondary-route
   continuation;
2. an ORDERED_ROUTE-specific path for route-specific override/conflict
   resolution.

A bounded two-bank Development experiment can write the same fact to both
domains and preregister a deterministic read-resolution rule, while measuring
false overrides on continuity cells and successful overrides on conflict cells.

Any such resolver is retrieval-composition evidence only. Read magnitude or
agreement must not become source-presence or correctness authority.

No core domain API, controller, context or model promotion follows.
