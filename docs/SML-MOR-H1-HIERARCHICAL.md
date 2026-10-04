# MOR-H1 — bounded hierarchical SBG/MOR candidate pool

Status: experimental reference implementation. No model-quality or latency claim.

## Purpose

The current SBG -> MOR bridge evaluates semantic similarity for every expert edge
reachable from the registry before MOR performs final sparse selection. That is
acceptable at small scale but does not provide the coarse-to-fine narrowing
needed by SML-D3 / SML-DS41-1.

MOR-H1 adds a bounded semantic cluster stage:

```text
SBG routing field
      |
      v
coarse cluster signatures
      |
      v
bounded candidate expert pool
      |
      v
expert similarity only inside that pool
      |
      v
existing native MixtureOfRouters
```

The existing `MixtureOfRouters` remains the only final routing engine.

## Contracts

- Every expert reachable from the source `MorRouterRegistry` must belong to
  exactly one declared coarse cluster.
- Cluster IDs, router signatures and expert signatures are unique.
- Candidate expert count and candidate router->expert edge count have separate
  hard budgets.
- The hierarchy fails closed on missing coverage, duplicate membership, zero
  cluster mass, arithmetic overflow or budget excess.
- The filtered registry never authorizes an expert edge absent from the source
  registry.
- Runtime hints, MOR consensus, hysteresis, balancing, denoising and precision
  escalation remain owned by the existing MOR implementation.
- Cluster score mass determines how many coarse clusters are retained between
  declared minimum and maximum bounds.
- No all-history scan, token-pair scoring, Q/K/V attention or softmax is added.

## Accounting

Each route reports:

- coarse cluster similarity evaluations;
- router similarity evaluations after narrowing;
- expert similarity evaluations after narrowing;
- exhaustive source-registry edge count as the matched logical baseline;
- unique candidate experts versus reachable experts;
- candidate edges versus exhaustive edges;
- candidate expert and edge fractions in ppm.

These are logical reference counts. They are not wall-clock, RAM, VRAM or energy
measurements.

## Qualification sequence

1. Conformance: hierarchical and exhaustive routes agree on fixtures where the
   exhaustive primary is retained.
2. Recall: measure whether the exhaustive correct/selected expert survives the
   coarse pool on disjoint development tasks.
3. Cost: compare cluster + candidate work against exhaustive expert scoring.
4. Robustness: test cluster ambiguity, ties, distribution shift and expert
   starvation.
5. Integration: only after recall/cost gates, evaluate the pool in SML-D3,
   Elastic Weight residency/prefetch and recurrent refinement.
6. Protected evaluation remains separate and requires a frozen protocol.

A reduction in expert comparisons is not a win if shortlist recall or final
decision quality regresses beyond the preregistered gate.

## Scientific boundary

MOR-H1 is an engineering primitive for SML-DS41-1 and SML-D3. It does not prove
that hierarchical routing improves language quality, decision accuracy, physical
latency or scaling. The first implementation intentionally keeps the existing
final MOR engine unchanged so any later improvement can be attributed to
candidate narrowing rather than a simultaneous rewrite of consensus semantics.
