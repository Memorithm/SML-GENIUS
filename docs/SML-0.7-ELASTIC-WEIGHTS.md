# SML-0.7 elastic weights

Status: executable reference control plane. It defines deterministic precision, residency, and activation planning for logical SML weight pages. It is not evidence of 25B throughput, quality, or a working GPU/disk materializer.

## Objective

SML has a large cold parameter atlas but intends to touch only a small routed subset per recurrent cycle. SML-0.7 makes three properties of a logical page elastic:

- representation class: Boolean (1 bit), ternary (2 bits), or residual low-bit (4 bits);
- residency tier: Disk, RAM, or VRAM;
- activation: selected for the next frozen recurrent cycle or inactive.

The learned parameter count does not change when a representation changes. Only physical representation cost and residency change.

## Versioned interop contract

SML-0.7.1 publishes the self-contained Rust contract `sml.elastic-weight-plan@1.0.0`.
Every transition carries its logical parameter count plus source/target representation versions.
The plan carries the RAM, VRAM and active-parameter limits under which it was produced.

`ElasticWeightPlan::validate_contract_v1` independently recomputes payload bytes, RAM/VRAM totals,
active parameters, active storage bits, movement totals and precision-change count from the page
transitions. It also rejects duplicate pages, active-on-disk targets, invalid representation-version
changes and budget overflow.

This makes the plan suitable for an external trusted runtime such as ElasticXxx to independently
revalidate rather than trusting SML's aggregate counters.

## Transaction boundary

The reference API is intentionally transactional:

1. record caller-owned access/importance evidence;
2. PLAN a cycle from explicit selected PageIds;
3. validate active-parameter, RAM, and VRAM limits;
4. COMMIT the immutable plan;
5. execute the cycle against the committed representation;
6. plan again only at the next boundary.

Every plan contains the planner generation and the source state of every page. Reusing an old plan fails closed. This prevents a precision/residency mutation from silently occurring halfway through a forward/recurrent cycle.

## Deterministic policy

Importance is expressed in integer parts per million. Thresholds choose Boolean, ternary, or residual4 representation. High-importance selected pages prefer VRAM; other selected pages prefer RAM and may use spare VRAM only as a deterministic capacity fallback.

Within one plan, selected pages are ordered by:

1. descending importance;
2. descending observed access count;
3. ascending PageId.

Unselected pages are inactive and disk-resident in this first reference policy. A future ElasticXxx runtime may retain warm cache copies, but any such policy must report duplicate residency/cache bytes separately.

## Exact accounting

The planner reports:

- total and active learned parameters;
- Boolean/ternary/residual4 parameter counts;
- disk/RAM/VRAM payload bytes;
- active semantic storage bits;
- bytes planned into RAM and VRAM;
- number of representation-class changes.

Payload bytes use exact bit counts rounded upward to whole bytes.

## Important scope boundary

A promotion from Boolean to ternary or residual4 does not magically create information that was absent from the Boolean representation. SML-0.7 changes and versions representation descriptors; actual rematerialization must be supplied by a training/master-weight or quantization layer.

This distinction is required before implementing adaptive precision training. A later slice must define the master representation, quantizer, calibration/evaluation loss, and reversible promotion/demotion rules.

## ElasticXxx boundary

The API is designed to map onto ElasticXxx's OBSERVE -> FORECAST -> PLAN -> VALIDATE -> ACT -> VERIFY -> COMMIT/ROLLBACK loop. SML remains authoritative for page identity, learned-parameter semantics, and cycle selection. ElasticXxx may become the runtime policy/execution layer for real page movement once the adapter is qualified.

## Evidence in this slice

Tests establish:

- exact 1/2/4-bit payload and ParameterBudget accounting;
- deterministic hot/warm/cold planning;
- explicit RAM/VRAM and active-parameter fail-closed limits;
- immutable planning before commit;
- stale-plan rejection;
- self-contained versioned plan validation and tamper rejection;
- representation version increments only on precision changes;
- access observations do not mutate representation outside a commit boundary.
