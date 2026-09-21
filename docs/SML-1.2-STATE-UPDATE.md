# SML-1.2 task-loss state update

Status: bounded task-loss experiment. It establishes one learned Boolean state transition on deterministic synthetic tasks. It does not satisfy the SML-1 quality gate and is not evidence of language-model quality or scaling.

## Objective

SML-1.1 recovered the conjunction truth table from task answers. SML-1.2 moves the learned boundary into persistent task state.

The new `OverwriteBitRecall` family emits:

1. `StoreBit(first)`;
2. a deterministic distractor interval;
3. `StoreBit(second)`;
4. another distractor interval;
5. `QueryRecall`.

The complete answer is the second stored bit. Across the canonical training range, the pair `(first, second)` covers all four Boolean combinations.

## Learned transition

The candidate state-update micro-operation receives:

- left input: previous stored state;
- right input: incoming stored bit;
- target: final task answer.

The optimizer is given task-derived samples, not the identity of the expected truth table. Exact overwrite semantics correspond to the RIGHT projection, code `0b1100`, but that code is used only for qualification after training.

The first store begins from deterministic false state. Because the overwrite family supplies a second write and complete four-row coverage, qualification is not reducible to learning first-write behavior.

## Controls

Three candidates are evaluated on a disjoint case-ID holdout:

- task-loss learned state update;
- deliberately wrong XOR state-update control;
- fixed RIGHT-projection reference baseline.

The learned candidate must execute the full holdout exactly, outperform XOR specifically on overwrite tasks, and match the fixed reference baseline.

Conjunction remains fixed to AND in this slice. Event routing is still trained through the existing separate route oracle; associative-memory semantics and copy control remain fixed.

## Evidence boundary

This experiment establishes recovery of one state-transition micro-operation from bounded task answers. It does not yet establish:

- learned recall control;
- learned copy/marker control;
- learned associative-memory read/write semantics;
- advantage of learned routing or memory over fixed/no-memory ablations;
- natural-language generalization;
- throughput or scaling.
