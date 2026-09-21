# SML-1.0 integrated task candidate pilot

Status: integration experiment. This slice does **not** satisfy the SML-1 gate and does not establish natural-language quality, general reasoning, architectural novelty, or superiority over attention/SSM/recurrent baselines.

## Question

SML-0.6 established that direct page-routing bits can be trained and frozen into the existing O(1) hard router. SML-1.0 asks a narrower systems question:

> Can a learned hard router be combined with bounded direct memory and deterministic task micro-operations while retaining complete execution evidence on case IDs excluded from route training?

The answer is tested by executable code and CI. The learned component is intentionally limited so that the evidence boundary remains explicit.

## What is learned

The event router has a 4-bit address space: 16 possible addresses.

There are 10 micro-operation pages. Nine correspond to the currently declared task events and one is a reserved no-op page. The route mapping is deliberately non-identity.

For 10 pages, each route requires 4 semantic Boolean bits. Therefore the router contains:

- 16 route entries;
- 4 semantic learned bits per entry;
- 64 learned Boolean route bits;
- 64 training-only latent scalars.

The route oracle is complete and supervised. It trains event-address to micro-operation-page selection only.

## What is fixed

The following semantics are reference code in this pilot and are **not learned**:

- storing one delayed Boolean bit;
- direct keyed Boolean fact writes and reads;
- marker-controlled payload collection;
- keyed conjunction;
- query emission;
- distractor no-op behavior;
- memory collision policy.

Consequently, exact held-out task execution in this slice does not prove that SML learned task semantics from data.

## Holdout boundary

Task generation now supports an explicit starting case ID.

The executable qualification uses development case IDs separately from a holdout range beginning at case ID 10,000. Route training does not consume task instances from either range; it consumes only the fixed 16-entry routing oracle.

The holdout therefore checks deterministic integration on unseen generated task instances. It is not a learned-semantic generalization claim.

## Memory contract

Each task execution receives a fresh fixed-capacity direct-address memory with one Boolean value bit per slot.

The pilot uses `ReplacementPolicy::RejectCollision`. A key collision is surfaced as `TaskCandidateError::MemoryCollision`; data is never silently evicted to make an evaluation pass.

A dedicated test forces every key into one slot and requires the second distinct write to fail closed.

## Evidence retained

For every event the candidate records:

- event index;
- exact typed event;
- 4-bit route address;
- frozen selected page;
- any emitted answer;
- cumulative execution counters.

The final evidence additionally records:

- task family and case ID;
- expected oracle answer;
- predicted answer;
- exact-match result;
- semantic route bits touched;
- complete event trace;
- final execution counters.

The candidate rejects non-zero token-pair comparison or attention-score counters.

## Qualification panel

The test suite executes 64 holdout case IDs for each of the five deterministic task families, for 320 task instances total.

The CLI smoke uses 8 holdout case IDs per family, for 40 task instances, and reports exact tasks, trace records, route lookups, memory reads/writes, and semantic route-bit touches.

All generated answers remain independently checkable through the existing symbolic task oracle.

## Scientific boundary and next experiment

SML-1.0 is an integration pilot, not evidence that learned routing or memory improves task quality.

The next experiment must remove this confound by learning task-state transitions or micro-operation semantics from task loss, then comparing at minimum:

1. learned routing + bounded memory;
2. fixed routing + bounded memory;
3. learned routing without memory;
4. memory-capacity ablations;
5. fixed-depth versus adaptive recurrent-depth controls.

Only a reproducible held-out advantage attributable to learned mechanisms can support progression toward the SML-1 gate.
