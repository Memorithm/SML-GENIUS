# SML-D: native decision programme

Decision capacity is a first-class SML objective, not a textual yes/no mode or a
small post-hoc classifier. A substantial learned expert family and dedicated
training objectives must be evaluated before SML-2, alongside language learning.
Do not freeze an arbitrary 25B allocation: separately report total decision
capacity, active parameters per decision, and fraction served without text.

## Architectural contract

Three paths share SBG/MOR/memory interfaces: direct decision, bounded recurrent
refinement, and optional language generation. Decision paths must run with no
text decoder. MOR chooses compute; decision experts answer the task. Neither
routing success nor a syntactically valid answer proves decision correctness.
A decision proposal is never authorization to execute an external action.

Long-term input: state + question + candidate definitions. Output: typed value,
uncertainty/abstention, evidence references, model/schema/policy versions and
resource evidence. Dynamic natural-language questions are NOT implemented by D0.

## Durable checkpoints and active slice

D0/D1a merged through PR #61 at 9749086c5ad9b77208d53539b179537dc844fc8c.
The exact pre-merge head 4d2f17a41b0a64a2c83e92a4b7838d9fbccdb18d passed
CI run 35785060702, including the four boundary tests previously held pending.

D1b merged through PR #62 at `6a640223015135e96c004bd836047a2ae7782f45`.

Active slice: **D1c bounded learned SBG feature-pair selection** on
`research/sml-d1c-learned-feature-pair`. Detailed contract:
[SML-D1c-FEATURE-SELECTION.md](SML-D1c-FEATURE-SELECTION.md).

D1b projects explicitly selected SBG coordinates into D0. Independent masks
state whether V, M, A and R were observed. Relevance M and absence of ambiguity A
are not substitutes for observation. A ContextValue is usable only when V/M/A
are observed, M is true and A false. Metadata features use their own availability.
The same versioned projection supplies inference and finite D1a training samples;
unknown training inputs reject the experiment instead of being filled or dropped.
Projection coordinates remain fixed metadata, not a learned semantic encoder.

D1b completion requires full CI, semantic-mask truth cases, schema/version/shape
rejection, budgets, large-width multi-head evidence, finite fitting, validation
rejection and source immutability. No wall-clock or language-quality claim.
Broad D1, D2-D5 and SBG-0.14 remain open after this narrow bridge.

## Reference implementation: SML-D0 and a narrow SML-D1a fixture

Code: crates/sml-decision. Dependencies: existing native sml-core only. The core
stays unchanged and dependency-free. No external Memorithm runtime dependency,
no new third-party package, no crate publication, and no unsafe-code exception.

D0 accepts at most 64 caller-supplied Boolean features with an explicit known
mask and schema ID. It compiles validated acyclic two-input SML truth tables
into a shared multi-head program. Heads return a Boolean, exactly one declared
candidate ID, or a bounded ordinal. The question IDs select declared heads;
this is NOT semantic comprehension of arbitrary questions or candidate text.

Bit planes pack independent records, not just coordinates within one record.
Each group contains at most 64 records. All heads reuse that group's gate values.
Missing information, inapplicable constraints, ambiguous/missing candidates and
out-of-range ordinals produce typed abstention. There is no forced argmax.
Immediate wire aliases are preserved. Unknown propagation through wider
reconvergent circuits is conservative, not a complete SAT/dependency analysis.

Every output is explicitly Uncalibrated. Truth-table determinacy and Boolean
similarity are not probabilities. Abstention here is logical, not a calibrated
statistical confidence threshold. Evidence/source authentication and runtime
policy authorization remain integration work; IDs alone are not authenticity.

Program admission limits gate count, head count and total head signals. Batch
planning checks records, result count, result element bytes, scratch element
bytes and logical gate evaluations before internal allocation/execution. It
validates the entire batch before returning outputs. Caller-owned input/program
vectors, allocator overhead, OS RSS and retained returned batches are not covered
by these payload bounds. No global process-memory ceiling is claimed.

D1a is an integer-only finite empirical-risk minimizer over 16 native two-input
truth tables. Training chooses the table; a disjoint-ID, fully covered validation
split may reject it but cannot choose another. A third test fixture is not fed
back into selection. Four-input-pattern identification is not generalization to
unseen concepts. No protected final SML-1 benchmark is used for model selection.
The fitter returns a proposal/report; it does not mutate a live model or persist
weights. Hand-authored circuit tables are not automatically learned parameters.

## Qualification

```sh
cargo test --locked -p sml-decision -- --nocapture
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
bash scripts/check-lockfile.sh
```

D0 tests exhaust all 16 tables and 9 partial-input combinations; compare with an
independent scalar completion oracle; test 0/1/63/64/65/127/128/129/4096 records;
exercise composition, aliasing, multiple heads, constraints, schemas, budgets,
overflow, deterministic fitting, split overlap and validation rejection.
D1b adds explicit observations and real SBG states at 257 and 16,384 bits/field.

Count logical gate-lanes, word-gate kernel calls, semantic/mask reads, feature
extraction, head signal reads, output cardinality and text tokens separately.
A 64:1 ratio of logical gate-lanes to grouped calls is NOT a 64x speedup: packing,
unknown-value logic, head resolution, allocation and I/O still cost work.
No throughput or Jev superiority claim is supported by these reference fixtures.

## Full evidence-gated roadmap

| Gate | Work and admission evidence |
|---|---|
| D0 | Typed decision contract, abstention, bounded batches, scalar equivalence. Merged PR #61. |
| D1a | Finite two-input fitting and validation rejection. Merged PR #61; reference fixture only. |
| D1b | Explicit observed-SBG coordinate projection, shared inference/training path. Merged PR #62. |
| D1c | Bounded training-only SBG feature-pair selection with fixed-coordinate control and validation veto. Active slice. |
| D1 | Substantial trained experts; learned SBG features; conditional questions/candidates; multi-task loss and active/total parameter accounting. Open. |
| D2 | Calibration on reserved data; risk/coverage and false-acceptance metrics; shifted-distribution tests; integer probabilities only after validation. Open. |
| D3 | Streaming/columnar batches, expert grouping, backpressure, sparse activation and storage; full encoding-to-result latency/throughput. Open. |
| D4 | Explicit escalation to bounded memory/recurrent refinement; fixed/adaptive depth and abstention at matched costs. Open. |
| D5 | Independent repeated evaluation against exact rules, small classifiers, constrained generative systems and external decision baselines. Open. |

Report decisions/sec at matched error/coverage, p50/p95/p99, admission failures,
RAM/RSS, page faults, disk traffic, compute and retained output size. The language
40-token/sec target does not substitute for decision throughput. Separate cold
and warm measurements and never infer whole-system speed from one kernel.

## Integration and ecosystem

SBG-0.14 must feed native decision experts as well as optional generation. D1b
reuses actual sml-core SemanticState and the v1 store decoder in a test without
adding a store runtime dependency. It does not yet schedule mapped pages, MOR,
Elastic Weights or recurrent experts. Observation masks are external metadata;
the existing store v1 does not supply or authenticate them.

SML-D does not replace SBG-0.13c resource qualification, native Elastic actuation,
learned marker/control, learned wiring, or the V888/CSP matched-control programme.
Reuse candidates: SciRust Boolean conformance, Forge topology search, TDI matched
panels, ElasticXxx admission and CCOS memory selection. Promote tested primitives
only after inspecting destination contracts; no mandatory external Memorithm
inference dependencies. Raw V888 grants no quality/performance presumption.
