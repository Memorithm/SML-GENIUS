# SML-D1c: bounded learned SBG feature-pair selection

Implementation: `crates/sml-decision/src/feature_search.rs`.
Tests: `crates/sml-decision/tests/feature_search.rs`.

## Scope

D1c is the first slice in which training selects **which declared SBG features**
feed a decision expert. It remains deliberately finite and bounded.

The caller declares a candidate pool through `SemanticProjection`. Training then
selects one unordered pair of feature slots and one of the sixteen native
`TruthTable2` functions. The search is exhaustive only inside the admitted
candidate pool.

Validation is evaluated only after the training winner is frozen. Validation may
reject the proposal, but cannot choose another pair or truth table.

This is feature selection over declared SBG coordinates, not a learned semantic
encoder, natural-language question understanding or a general sparse MoE router.

## Admission and work bounds

The policy separately limits:

- total training + validation samples;
- candidate feature count;
- candidate-pair count;
- worst-case training table evaluations;
- validation errors;
- validation missingness.

The default candidate pool is at most sixteen features, yielding at most 120
unordered pairs. This is intentionally much smaller than the full 16,384-bit SBG
field. Full-state search is not permitted by this slice.

Before search, the implementation checks the worst-case training work:

`training_rows * candidate_pairs * 16 truth tables`.

Projection uses the existing D1b projection budget and explicitly accounts logical
semantic/observation reads. Internal projected-label payload is also bounded.

## Selection discipline

A candidate pair is eligible for training-only selection only if:

1. both selected features are known for every training row;
2. the pair covers all four Boolean input combinations on the training split.

Among eligible pairs, training minimizes exact classification errors over all
sixteen truth tables. Deterministic iteration order provides the tie break:
lowest left slot, then right slot, then truth-table code.

A declared fixed-coordinate baseline is scored separately.

Acceptance requires:

- the selected training errors improve on the fixed baseline by the configured
  minimum;
- selected validation errors do not exceed the fixed baseline;
- validation error and missingness limits are satisfied;
- validation covers all four selected Boolean input combinations.

Missing selected validation features count as failures. They are never filled,
dropped or coerced to false.

## Scientific boundary

D1c demonstrates bounded feature selection in controlled Boolean fixtures. It
does not establish that a learned selector generalizes to unseen concepts,
languages or domains. The candidate pool is hand-declared, and the selected
feature coordinates remain explicit metadata.

The fixture intentionally includes distractor coordinates and an independently
declared fixed baseline. A perfect fixed baseline prevents an improvement claim,
even if training rediscovers the same optimal pair.

The next substantial D1 work must expand from this finite pair search toward
learned conditional expert families and sparse SBG/MOR activation while keeping
protected evaluation data out of selection.
