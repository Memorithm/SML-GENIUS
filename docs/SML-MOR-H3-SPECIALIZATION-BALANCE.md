# MOR-H3 — specialization-preserving balance

Status: experimental reference implementation. No model-quality, throughput, or
deployment-promotion claim.

## Problem addressed

The original stateful MOR controller balances expert load over the experts visible
in the current candidate set. A newly visible expert begins with load zero. If
another expert already has substantial historical load, the new expert can receive
a large balancing boost even when it belongs to a different semantic
specialization.

That behavior is useful as a generic anti-starvation mechanism, but it can also
push routing away from semantic specialization for reasons unrelated to the
current task.

MOR-H3 separates two questions:

1. should currently competing **clusters** receive a small balancing correction?
2. inside one cluster, should currently competing **experts** share work more
   evenly?

## Architecture

MOR-H3 introduces a generic `MorSpecializationRegistry` and
`MorSpecializationBalancer`.

The hierarchy remains:

```text
SBG state
   |
   v
MOR-H1 / MOR-H2 cluster shortlist
   |
   v
same cluster partition
   |
   v
MOR-H3 specialization-preserving balance hints
   |
   v
unchanged MixtureOfRouters final selection
```

`SemanticHierarchicalMorBridge::specialization_registry()` derives the H3
registry directly from the H1/H2 cluster declarations. The shortlist topology
and the balance topology therefore cannot silently use different cluster
memberships.

## Exposure-conditioned load

Expert and cluster load EMAs are updated only when that expert/cluster was
actually exposed in the current candidate set.

If a domain is absent for many cycles, its historical load does not decay merely
because unrelated domains were active.

This is intentionally different from treating every registered expert as a
candidate on every cycle.

## Neutral first exposure

A newly exposed cluster is initialized to the mean historical load of the
already-known clusters that it is currently competing with.

A newly exposed expert is initialized to the historical mean of already-known
experts in its own cluster.

Therefore a new domain or expert does not receive an artificial underuse bonus
solely because it had no previous exposure.

This is neutral initialization, not evidence that the new expert is good.

## Separate balance budgets

The policy exposes independent integer-ppm controls for:

- inter-cluster balance gain;
- intra-cluster balance gain;
- maximum inter-cluster adjustment;
- maximum intra-cluster adjustment;
- maximum total adjustment.

The default inter-cluster correction is deliberately smaller than the
intra-cluster correction. These defaults are engineering hypotheses, not
calibrated values.

All adjustments remain bounded runtime hints. The existing
`MixtureOfRouters` still owns consensus, denoising, hysteresis, residency,
precision escalation and final expert ranking.

## Evidence

Each cycle reports:

- historical cluster load;
- historical conditional expert load;
- inter-cluster adjustment;
- intra-cluster adjustment;
- combined bounded adjustment;
- effective runtime-hint bias;
- which clusters/experts were neutrally initialized;
- candidate and selected cluster/expert counts;
- cluster-load spread;
- maximum intra-cluster load spread;
- route-switch status.

## Reference controls

The implementation includes a regression fixture where the legacy global
controller is deliberately driven into selecting a newly visible expert from a
different cluster because that expert starts at load zero.

Under the same routing evidence, MOR-H3 neutrally initializes that new cluster
and preserves the semantically stronger existing expert.

This fixture demonstrates a mechanism difference only. It is not representative
task-quality evidence.

Additional tests verify:

- an absent cluster/expert does not decay without candidate exposure;
- intra-cluster balancing can redistribute work inside one specialization;
- inter-cluster adjustment obeys its own hard cap;
- malformed or uncovered specialization mappings fail closed.

## Qualification sequence

Before any runtime promotion:

1. freeze matched tasks and seeds;
2. compare legacy global balancing, no balance, H3 inter-only, H3 intra-only and
   H3 combined arms;
3. report task quality, shortlist recall, per-cluster exposure, per-expert
   exposure/starvation, route switches and cumulative active work separately;
4. test domain-frequency shift and introduction of previously unseen clusters;
5. retain negative results and cross-cluster regressions;
6. measure wall-clock and residency costs only after logical behavior is
   qualified.

MOR-H3 does not authorize answers, relax SML-D calibration, or establish that
specialization-preserving balance improves general language quality.
