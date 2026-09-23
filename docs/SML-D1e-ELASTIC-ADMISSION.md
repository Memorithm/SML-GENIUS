# SML-D1e: MOR-selected Elastic decision admission

Implementation: `crates/sml-decision/src/elastic_bank.rs`.
Tests: `crates/sml-decision/tests/elastic_bank.rs`.

## Contract

D1e inserts the native Elastic Weight planner between MOR routing and decision
expert execution.

The execution order is:

`SBG -> MOR route -> union selected pages -> Elastic PLAN/VALIDATE -> decision experts`.

MOR remains the sole decision-expert selector in this slice. Elastic does not
perform another semantic ranking and does not replace the MOR-selected set. It
receives exactly the unique page IDs already selected by MOR and determines
whether that working set satisfies native active-parameter, precision, RAM and
VRAM policy.

Every route and every decision-batch admission is established first. The Elastic
plan is then generated and independently validated. Decision experts execute only
after that complete plan succeeds.

## Page identity

Decision expert page IDs and Elastic Weight page IDs share the same page
namespace for this adapter. Construction requires exact inventory equality.
Missing, extra or duplicate page identity is rejected before evaluation.

The active page set in the generated Elastic plan is independently compared with
the MOR-selected union. Any mismatch fails closed.

## Non-mutating boundary

D1e calls `ElasticWeightPlanner::plan_cycle` and
`ElasticWeightPlan::validate_contract_v1`. It does **not** call commit.

Therefore:

- planner generation remains unchanged;
- current page residency/activation remains unchanged;
- reported bytes moved are proposed transition accounting, not observed physical
  I/O;
- no disk/RAM/VRAM payload is actually materialized by this slice.

Physical MATERIALIZE/VERIFY/COMMIT/ROLLBACK remains a separate runtime project.
D1e deliberately prevents a planning result from being mislabeled as physical
actuation.

## Empty batches

An empty decision batch performs no MOR route and creates no Elastic plan. The
result contains neither admission evidence nor a plan.

## Evidence

Tests distinguish high-importance Residual4/VRAM planning from Boolean/RAM
planning, exact one-page and two-page MOR unions, active-parameter and residency
budget rejection, inventory mismatch, empty batches and the 16,384-bit SBG
reference width.

The next step is a physical page-materialization layer or a measured plan-only
end-to-end control. No throughput or memory-residency claim follows from D1e.
