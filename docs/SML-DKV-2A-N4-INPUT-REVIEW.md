# DKV-2a-N4 — owner input review: combined candidate on recurrent chains

Input: `sml-dkv-0007-dkv-4i-4j.diff`.

- bytes: 84,045;
- lines: 1,738;
- SHA-256: `58bab302cba162ada16468314aa8caf07c7a22c8c3cb553ed9b03e91b8014135`.

The supplied diff contains two Rust 1.97 Development studies labelled DKV-4i
and DKV-4j in the source material. Current SML-GENIUS does not reuse those
labels because the repository already has an independent DKV-4 adaptive-control
programme.

## Source result retained from DKV-4i

The source combines:

- sparse/active-only ambiguity projection;
- page-conditioned hash family;
- the historical DKV-4 multi-hop dynamic re-key panel.

Its reported pooled F6 accuracy improves materially over the old default:

- G=4: 60.9 -> 82.8 at K64; 75.0 -> 82.8 at K256/1024;
- G=16: 17.2 -> 31.2 at K64; 21.5 -> 62.1 at K256;
  23.0 -> 71.5 at K1024.

However the source's preregistered capability gate still fails: low-load
ORACLE accuracy is 82.8%, with L2/L3 recall remaining below the required 90%.
The supplied source therefore does not promote the combined projector or any
controller.

The source also reports that ambiguity is only a partial miss signal: 55-69%
of novel starts end ambiguous under STAB, below its 70% gate.

## Source result retained from DKV-4j

DKV-4j adds a value-field multiplier w in {1,2,4,8} after observing that chain
nodes share M/R while V changes between hops.

The reported result is negative:

- K256,G4 F6: 82.8, 84.4, 84.4, 84.4%;
- K1024,G4: 82.8% for all weights;
- K256,G16: 62.1, 62.1, 63.3, 63.3%;
- K1024,G16: 71.5, 71.9, 71.9, 71.9%.

The source's own H1/H2/H4 capability hypotheses are refuted. Therefore a
`with_value_weight` core API is not useful enough to import.

## Core changes rejected

The supplied diff adds a combined `new_candidate`, page-conditioning state,
a single-expert error, and `with_value_weight` directly to the projector.

Those changes are **not imported** now:

1. N3B already proved the page-seeded mechanism using the existing current
   projector plus an outer page-derived seed.
2. N3B explicitly left multi-expert MOR semantics unresolved; adding a core API
   that simply rejects multiple experts would prematurely fix a semantic choice.
3. N3B still requires unbalanced-page and DKV-3a record/subject/provenance
   qualification before core promotion.
4. value weighting has no demonstrated material benefit in the source input.

## Current-stack useful replay

The useful next action is a current Rust 1.89 replay of the historical chain
panel using only current APIs:

- default arm = current qualified Bipolar constructor;
- combined arm = current ActiveOnly constructor with the N3B outer
  page-derived seed for the panel's single fixed PageId(5).

Because every panel decision has exactly one fixed page, this harness-only
composition is semantically equivalent to the source's page-seeded candidate
for this panel without changing the core.

The replay is routed as **DKV-2a-N4**, not DKV-4.

No context/model promotion follows.
