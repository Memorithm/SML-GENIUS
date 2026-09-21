# SML-1.1 task-loss learned conjunction gate

Status: bounded task-loss trainability experiment. This slice does not satisfy the SML-1 gate and does not establish natural-language quality, general reasoning, architectural novelty, or superiority over another model family.

## Question

SML-1.0 integrated learned event routing with fixed task micro-operations. The remaining confound was explicit: the conjunction query was still implemented as a hand-written Boolean AND.

SML-1.1 asks:

> Can the hard Boolean function used by the conjunction micro-operation be recovered from task examples and their expected answers without supplying the AND truth-table identity to the optimizer?

## Training signal

The development generator emits complete `TwoFactConjunction` task instances.

For each development instance, the trainer extracts:

- the Boolean value associated with the query's left key before the query;
- the Boolean value associated with the query's right key before the query;
- the scalar expected task answer.

The optimizer receives only these supervised triples. It does not receive `TruthTable2::AND` as a target.

The training set must contain all four Boolean input patterns. Training fails closed when the observed input-pattern mask is not `0b1111`.

## Latent gate

The reusable `LatentGate2::train_samples` path optimizes the same 16-way latent selector used by earlier SML-0 experiments, but now accepts arbitrary caller-owned samples rather than a complete truth-table oracle.

Hard deployment remains a four-bit Boolean truth table.

For this micro-operation:

- learned hard state: 4 Boolean bits;
- training-only latent state: 16 floating-point scalars;
- default development task samples: 64 conjunction tasks;
- task-loss optimization epochs: 600.

## Holdout

Development case IDs begin at 0. Qualification uses a disjoint holdout beginning at case ID 10,000.

The SML-1.1 test panel uses 64 holdout case IDs per task family, for 320 total tasks. The CLI smoke uses 16 per family, for 80 total tasks.

The complete candidate is evaluated, not only the gate in isolation: learned routing, bounded memory, learned conjunction table, task answer and event traces all remain active.

## Controls

Three conjunction semantics are evaluated on the same holdout panel:

1. the task-loss-learned gate;
2. a deliberately wrong fixed XOR gate;
3. the hand-coded fixed AND oracle.

The learned gate must outperform the wrong XOR control on conjunction tasks.

The learned gate is expected to tie the fixed AND oracle if it successfully recovers the target function. That tie is not evidence of superiority over hand-coded task semantics; it is evidence that the semantic function was recovered from task examples.

## What remains fixed

This experiment still does not learn:

- event-address routing from downstream task loss; routing uses the SML-0.6 supervised route oracle;
- associative-memory write/read semantics;
- delayed-recall state update semantics;
- copy/marker control semantics;
- distractor semantics;
- recurrent-depth policy.

These remaining fixed components prevent an SML-1 gate claim.

## Required next ablations

Before claiming that learned routing or memory contributes held-out quality, the same panel must compare at least:

- learned routing versus fixed routing;
- memory versus a no-memory control;
- multiple memory capacities and collision regimes;
- fixed recurrent depth versus adaptive recurrent depth;
- task-loss-trained state transitions beyond conjunction.

All results must retain the same disjoint case-ID boundary and complete execution evidence.
