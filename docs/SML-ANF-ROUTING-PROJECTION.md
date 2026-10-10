# Explicit ANF output to SBG routing integration

Date: 2026-10-10. Status: functional candidate requiring exact-head CI.

## The missing connection

The original `unified_anf_sbg_loop` from #461 invokes every stage in one
transactional recurrent execution. However, `LargeBankRecurrent` originally
rewrites only `SemanticState.value`, while `SemanticMorBridge::build_inputs`
derives router affinity and expert scores from `SemanticState.routing`.
Sequential composition alone does not make the large-ANF output control MOR.
The native diagnostic from #462 intentionally retains this unprojected control.

## Opt-in execution contract

`LargeBankRecurrent::with_routing_projection` accepts a bounded vector of
`(post_anf_value_bit, routing_destination_bit)` assignments. It rejects an
empty plan, out-of-range coordinates, more assignments than semantic bits,
duplicate destinations, and reconfiguration after a committed history prefix.
A source may be broadcast to multiple distinct destinations. Unassigned routing
bits, meaning and ambiguity fields are preserved by this projection.

The projection executes after the current ANF output is computed, before MOR
reads the routing field. The cycle is committed only after the existing coupled
kernel has completed expert execution and the current-route Delta-KV step.
The public `routing_projection()` accessor retains the exact configured plan.
Rewinding ANF history preserves this plan and still requires the caller to
supply the matching complete semantic state; this is not a disk checkpoint.

`unified_anf_sbg_loop_with_routing_projection` exposes this option for the full
pipeline. The original constructor remains the unchanged unprojected control.
There is no inferred, learned or silently installed default projection.

## Reproduce focused validation

From an authorized checkout containing this change:

```bash
cargo test --locked -p sml-core anf_sbg_unified::projection_tests -- --nocapture
```

Five tests cover projection from the post-ANF rather than incoming value,
preservation of unassigned fields, invalid plans and late reconfiguration,
real multi-cycle changes of selected/executed expert pages and memory keys,
rollback after downstream memory rejection, and late budget rejection after a
committed projected cycle.

The causal fixture toggles ANF value bit 7 and explicitly broadcasts it to the
eight routing coordinates. With the same initial state and ANF bank, the
unprojected control selects page 0 throughout. The projected candidate is
required to execute pages 1, 0, 1, 0 across four cycles. Its actual Delta-KV key
is checked against the projector using the current state and current decision;
a frozen-route counterfactual is evaluated on exactly the same post-expert state.
These are test requirements, not results until the exact-head suite passes.

## Remaining boundaries

This is a declared synthetic wiring mechanism, not learned routing or a
language-model quality result. Semantic width remains limited to 1..31 by the
existing large-bank recurrent adapter. Selected expert bodies still use PVP6,
not large-ANF banks. Declaring a 20-bit ANF address domain does not demonstrate
exhaustive K=1,048,576 execution or million-expert/25B-scale operation.

The plan performs one routing write per configured assignment per cycle.
This source-level bound is not physical RAM/VRAM enforcement or a speed claim.
Full-loop physical admission and measurement remain RemoteOps responsibilities.
No external runtime dependency, GPU SDK, persistent service or CI policy change
is introduced. The future SML-HARNESS remains separate.

The reusable pattern is an explicit state-field projection plus a same-state
frozen-route counterfactual. Applying it in SciRust, FLAT-ATTENTION or NNIS would
require destination-specific qualification; this PR does not change those repos.
