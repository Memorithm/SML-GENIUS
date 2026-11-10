# SBG-REA-3 — TRAIN-only learned edge-to-page associations

Status: **experimental native SML algorithm**, stacked after SBG-REA-2.
No production model routing, parameter-training scale, quality, or speed claims.

## Hypothesis

Can a learned sparse mapping predict which already-qualified MOR expert
pages are associated with new SBG `R` fronts, with less downstream page
work than always preparing all expert pages?

The `semantic::rae_learned_route` module fits deterministic
hard associations from each rising R-coordinate using genuine
`SbgRaeNativeFrame` labels. Training counts how often a feature rises
in a TRAIN frame and how often the native MOR selects each original
PageId simultaneously. Candidates are ordered by the largest co-occurrence
count, ties resolved by smaller original PageId.

The original `train` method retains the one-page-per-front control.
The separately configured `train_with_fanout` can retain up to
`RAE_MAX_LEARNED_FANOUT = 8` TRAIN-ranked expert pages per rising feature.
It preserves original MOR PageIds, sorts by highest observed co-occurrence
and then ascending ID, and applies the same frozen exposure/precision gate
to each candidate. Increasing fan-out raises emitted page work and
can improve multi-expert recall, but cannot rescue a required page when
no front arrives. Exact per-frame missed/extra page counts report the
trade-off; no model quality gain is assumed.

The model only freezes a feature -> page association if:
- the feature has at least the configured number of TRAIN rising exposures;
- its observed conditional co-occurrence ratio reaches a predeclared
  integer parts-per-million threshold.

These TRAIN ratios are **not calibrated prediction confidence**,
downstream quality or scientific proof. A feature with no evidence is
unmapped, never assigned a fabricated page. New input fronts trigger
sparse assignments with bit-packed output masks and original page IDs.

## Exact controls

- Training and evaluation require distinct, nonempty caller-supplied
  dataset identities (not cryptographic proof of no leakage).
- TRAIN exposures, page-hit counts, high padding and source width are
  checked exactly. Per-frame original MOR PageId lists and packed page
  masks must agree; duplicate, missing or out-of-range IDs fail closed.
- Front masks must be compatible with active states and accurate popcounts.
  From the second frame onward, rising/falling masks must agree with
  consecutive sampled states, and cycles must be strictly increasing.
  The initial frame's baseline remains a trace-producer obligation.
- A bounded sparse association map prevents implicit quadratic
  feature×expert allocations. The caller selects a frozen maximum number
  of observed associations.
- During holdout, never update the learned map. Report original
  PageIds selected, missed native MOR pages and extra pages per frame
  as well as trace totals.
- A missing front while a native expert remains needed must produce
  a visible **false negative**, not a silently suppressed operation.
- Native MOR remains the full model-owned baseline. No inference-time
  skipped model computation is authorized by this research module.

## Paired frozen policy replay

The same TRAIN-frozen association map supports three counterfactual policies:

- `RisingOnly`: select pages for current 0 -> 1 R transitions;
- `PersistentActive`: select pages for every currently high R bit,
  including held-high features with no fresh impulse;
- `RisingWithContextRecheck`: select rising pages and additionally
  re-evaluate currently active features on an explicit trusted context
  invalidation.

`evaluate_with_policy` compares these policies against precisely the
same independent native MOR masks, with selected, required, missed and
extra original-page counts. The persistent policy can recover a held-high
page missed by rising-only selection, but incurs work on held state.
A context recheck does not guarantee correctness: the newly required
page could have **no corresponding currently active feature**. No
policy is allowed to suppress MOR until such residual dependencies
are independently qualified with a fail-safe native fallback.

## Explicit limitations

A single event feature can correlate with multiple experts; even the
bounded one-to-many variant may lose necessary pages and is not an exact
substitute for distributed SBG/MOR scoring. It is a deliberately falsifiable
candidate. No speedup follows from event count alone: feature-state
generation/scanning, TRAIN, map storage, page scheduling, cache residency,
and fallback must all be accounted for.

The SML model must remain free of mandatory FLAT/NNIS/CUDA dependencies.
After qualification, neutral native trace export can be consumed by
FLAT-ATTENTION RAE-3's paired page-policy audit. Comparisons to
Transformer QK/softmax models require **separate** numerical oracles.

## Required validation

```bash
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked -p sml-core semantic::rae_learned_route
cargo test --locked --workspace
```

This milestone is not complete before its final-head CI is green.
Do not promote to default SML routing before learned event map is
validated on held-out real model traces and downstream task quality.
