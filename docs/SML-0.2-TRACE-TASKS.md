# SML-0.2 recurrent trace and deterministic task contract

Status: executable development infrastructure. This document does not report trained-model quality.

## Recurrent trace

SML-0.2 records every internal reasoning cycle at execution time. A trace record contains:

- emitted-token position;
- 1-based recurrent depth;
- circuit-atlas pages selected in execution order;
- learned parameters touched;
- semantic storage bits touched;
- Hamming distance between the previous and next Boolean workspace;
- contradiction flag;
- token-pair comparison count;
- attention-score operation count.

The normal `run_recurrent` API now delegates to the traced runner and discards the trace only for callers that do not need evidence. This prevents the traced and untraced execution semantics from drifting apart.

A candidate trace is rejected if any cycle reports a non-zero token-pair comparison count or attention-score operation count.

## SML-1 development tasks

The first task panel uses complete symbolic oracles and no model-based judge.

Families:

1. delayed bit recall;
2. keyed Boolean fact recall;
3. copy after marker;
4. two-fact conjunction;
5. distractor rejection.

Every task instance contains a typed event sequence and an expected answer. The expected answer is independently recomputed from the event stream by `solve_task`.

Generation is deterministic from `case_id` and declared task size. Re-running the same generator arguments must produce byte-equivalent Rust values.

## Scope

These tasks are intentionally bounded. Passing them would establish only that an implementation can learn or execute the declared operations under the declared resource budget.

They do not establish:

- natural-language quality;
- general reasoning;
- long-context capability;
- architectural novelty;
- superiority to attention, SSMs, recurrent language models or other baselines.

## Evidence rule

Before a trainability result is retained, the evaluator must preserve:

- exact task instance;
- expected oracle answer;
- predicted answer;
- recurrent trace;
- final execution counters;
- model/config identity;
- exact source revision.

SML-0.3 should add the first trainability mechanism and evidence bundle around this contract.
