# SML-RSI: human-gated recursive self-improvement

SML-GENIUS may participate in its own improvement, but the repository owner
remains the promotion authority. The current model may propose and implement
changes, open GitHub pull requests, gather CI/evaluation evidence and revise a PR.
It may not merge its own RSI PR or silently replace the current champion.

## Governance invariant

For RSI work:

```text
Champion_n = approved main
      |
      +--> SML proposes one bounded improvement
      |
      +--> branch + code/data/config/tests
      |
      +--> GitHub Pull Request
      |
      +--> CI / exact verifiers / benchmarks / protected evaluation
      |
      +--> model may repair the same PR from evidence
      |
      v
   HUMAN GATE
      |
      +--> user rejects/closes  -> Champion_n remains
      |
      +--> user merges          -> Champion_{n+1}
```

A green PR is evidence that the candidate satisfied declared automated gates.
It is not authorization to promote it. **The user's GitHub merge decision is the
final promotion event.**

This rule applies even if future SML versions become better than the current
system at coding, evaluation, architecture search or research planning.

## Repository mapping

The RSI state machine maps directly onto GitHub:

- `main`: currently approved champion lineage;
- research branch: one candidate or bounded candidate family;
- pull request: proposed promotion package;
- commits: candidate revisions;
- checks/CI: machine-verifiable evidence;
- PR discussion/review: human/model critique and requested changes;
- merge commit: explicit human-approved promotion;
- closed/unmerged PR: rejected or archived candidate;
- revert PR: explicit rollback path.

The model may continue to produce ordinary development PRs, but any PR that
changes the self-training, promotion, evaluator, mutation, autonomous-data or
meta-improvement machinery must be classified as an **RSI PR** and must never be
auto-merged.

## Candidate package

An RSI PR should eventually include a machine-readable evidence manifest with:

- parent `main` SHA;
- candidate head SHA;
- improvement class;
- affected model/runtime/training surfaces;
- generated data/task identifiers;
- verifier versions;
- training/search configuration;
- compute/time/storage budget;
- development metrics;
- protected-evaluation references;
- resource regressions/improvements;
- known failures;
- rollback/revert target.

Protected evaluation material must not be exposed to the candidate-generation
path merely because GitHub stores its metadata.

## Direct design inheritance from Memorithm/RSI

`Memorithm/RSI` is an internal **source of inspiration and prior design evidence** for loop engineering. SML-RSI must remain an independent implementation inside SML-GENIUS. It may study mature contracts from RSI, but it must re-derive and implement the required mechanisms natively rather than depend on, call into, import, vendor or orchestrate the RSI repository.

The following RSI mechanisms are useful design inspirations to re-implement independently:

- **AUTOPILOT outer loop**: one human objective -> frozen spec -> task DAG ->
  bounded implementation -> draft PR -> human review. Human review is the
  terminal fitness signal.
- **No direct main mutation**: RSI's PR core exposes branch creation and pull
  request planning, deliberately no default-branch write or merge action.
- **DGM inner loop**: proposer -> isolated candidate snapshot -> build/test/bench
  evaluator -> elite acceptance/archive; the model's self-assessment is never
  authoritative.
- **CandidateState**: complete materialized candidate trees with deterministic
  SHA-256 identity, explicit parent identity and storage budgets.
- **CumulativeArchive**: replayable branching lineages rather than one greedy
  chain, with complete-state promotion evidence.
- **EvaluationPipeline**: declarative host-defined evidence steps with explicit
  evidence kinds (build, tests, numerical parity, provenance, determinism,
  resource budget, policy, benchmark), bounded timeout and output.
- **Loop Engineering L1-L9**: motivated stop reasons, plateau/divergence
  detection, multi-scale schedules, circuit breakers, checkpoint/replay,
  HITL veto, interactive control, swarm/portfolio execution and loop
  benchmarking.
- **Flywheel**: capture real candidate/verdict trajectories and use them to
  improve prescreen/world-model/proposer quality. Training data comes from
  measured outcomes, not model narration.
- **Frozen-tests feature regime**: when the agent would otherwise write both the
  feature and its tests, tests/specification are human-approved/frozen first to
  break self-grading circularity.
- **Chaos-tested safeguards**: a safeguard is not treated as real until an
  adversarial test demonstrates that it blocks the relevant failure mode.

SML-RSI should preserve these semantics while replacing RSI-specific
`SI_global`/continuous agent-state scoring with SML-specific evidence vectors:
model/task quality, decision calibration, context retrieval, active compute,
RAM/VRAM/disk traffic, latency, reproducibility and protected-regression gates.

For SML-GENIUS the intended relationship is:

```text
Memorithm/RSI
   (ideas / lessons only)
          |
          v
independent SML-RSI design inside SML-GENIUS
          |
          +--> SBG / MOR / memory / recurrent-depth candidates
          +--> SML-D decision candidates
          +--> SML-CONTEXT retrieval/retention candidates
          +--> Elastic/runtime candidates
          |
          v
GitHub PR + SML CI/evidence
          |
          v
human merge/reject
```

Hard boundary: no Cargo dependency, git dependency, submodule, vendored crate, MCP dependency, service call, shared runtime state, CI dependency or mandatory orchestration dependency on `Memorithm/RSI`. SML-RSI must be fully buildable, testable, reproducible and operable from SML-GENIUS alone.

## What can self-improve

### RSI-A — data and curriculum

The champion may propose:
- new exact-verifier algorithmic tasks;
- adversarial distractors;
- decision/calibration datasets;
- long-context curricula;
- counterexamples from prior failures.

Synthetic data is never trusted because it is synthetic. It is admitted through
independent validation, deduplication, provenance and held-out evaluation.

### RSI-B — model parameters and experts

The system may train candidate:
- Boolean/low-bit decision experts;
- SBG feature selectors;
- routing experts;
- recurrent update/halt policies;
- calibration components.

The deployed champion remains frozen until the user merges the resulting PR.

### RSI-C — MOR, memory and context

The system may propose:
- router signatures and specialization;
- memory write/read/retention policies;
- SML-CONTEXT retrieval/retention changes;
- candidate shortlisting policies.

Exact-source provenance and fail-closed retrieval semantics remain hard gates.

### RSI-D — topology and wiring

SML may search bounded topology/wiring DSLs. Candidate topologies require
independent destination-side requalification. Search-system scores alone are not
promotion evidence.

### RSI-E — runtime

SML may tune:
- hot/cold page policy;
- prefetch;
- batching;
- scheduling;
- precision/residency thresholds.

Runtime candidates cannot weaken correctness, integrity, provenance or resource
measurement merely to improve throughput.

### RSI-F — evaluator and training-loop changes

The system may propose modifications to:
- reward/scoring rules;
- candidate allocation;
- verifier selection;
- curriculum policy;
- mutation operators;
- promotion heuristics.

These are the most sensitive RSI changes because they change how future changes
are judged. They always remain human-gated GitHub PRs.

## Verification hierarchy

Prefer the strongest available evidence:

1. formal/exact verifier;
2. executable environment/compiler/simulator;
3. frozen independent learned verifier;
4. independently trained judge ensemble;
5. current champion judging candidates;
6. candidate self-judgment.

Self-judgment may generate hypotheses or triage examples; it is not sufficient
promotion evidence by itself.

## Protected champion/candidate evaluation

A candidate must be compared against its frozen parent champion on the same
declared evaluation protocol.

At minimum report:
- quality delta;
- per-capability regressions;
- resource delta;
- active-compute delta;
- latency/throughput delta where applicable;
- calibration/risk delta for decisions;
- context retrieval delta for long-context changes;
- candidate variance/repeated runs when stochastic training is involved.

Promotion criteria should be Pareto-aware rather than one scalar reward. A large
gain on one benchmark must not silently erase important capabilities.

## Recursive levels

The programme separates increasing levels of loop closure:

- **RSI-0** — GitHub lineage/promotion contract;
- **RSI-1** — self-generated tasks with exact verifiers;
- **RSI-2** — candidate self-training from generated/curated tasks;
- **RSI-3** — proposer/solver/verifier co-evolution;
- **RSI-4** — self-improvement of SML-D, MOR, memory and SML-CONTEXT;
- **RSI-5** — topology/runtime policy search;
- **RSI-6** — candidate evaluator improvement against frozen anchors;
- **RSI-7** — candidate changes to the improvement procedure itself;
- **RSI-8** — multi-generation qualification under the persistent human GitHub
  merge gate.

The loop can become increasingly autonomous in proposing, coding, testing and
repairing PRs without removing the final promotion authority.

## First implementation sequence

### RSI-0a — PR evidence manifest

RSI-0a merged through PR #69.

RSI-0b merged through PR #70.

RSI-1 exact-oracle curriculum merged through PR #71.

RSI-2 control-plane cycle merged through PR #72.

Active implementation slice: `research/sml-rsi-1b-matched-training`.

The dependency-free `sml-rsi` crate defines `sml.rsi-candidate@1.0.0` with:
parent main SHA, candidate head SHA, non-default branch, change class, objective,
bounded candidate budget, typed evidence refs with SHA-256 digests, fixed-point
metric comparisons, known regressions and rollback to the exact parent champion.
The manifest contains no promotion flag. Promotion eligibility is RSI-0b.

### RSI-0b — promotion-state verifier

The native verifier checks:
- manifest parent SHA equals the trusted current-main SHA;
- manifest candidate SHA equals the trusted exact PR head;
- policy-required typed evidence is present;
- selection evidence and protected evaluation evidence are declared separately and disjoint;
- every referenced digest exists in the candidate manifest;
- no hard-gate metric regresses;
- critical known regressions are rejected under the baseline policy.

Its only positive state is `EligibleForHumanReview`. The API deliberately reports
`is_merge_authorization() == false`; the future model still cannot promote itself.

### RSI-1 — exact-verifier curriculum generator

The model may propose only bounded task descriptors. Native `sml-core::tasks`
constructors and their independent `solve_task` oracle recompute every answer;
a proposal cannot supply its own truth label.

RSI task case IDs start above a reserved floor to avoid the canonical low-ID
development fixtures. Host code assigns only Training/Development partitions;
there is deliberately no Protected variant in the proposal/curriculum API.
Duplicate proposal IDs, duplicate family/case identities, invalid difficulties
and event/case/proposal budget overruns fail closed.

The next matched experiment must compare this model-proposed curriculum against a
fixed human-authored curriculum at equal training compute; merely generating
oracle-valid tasks is not evidence that the curriculum improves learning.

#### RSI-1b — matched curriculum training

The native matched experiment trains the same conjunction micro-operation and the
same `SmlTaskCandidate` path from two curricula:

1. the model-proposed, oracle-verified RSI training partition;
2. a deterministic fixed curriculum with exactly the same number of training tasks.

Both candidates are then evaluated on the same separately constructed holdout
range after training. Proposed, fixed and holdout case IDs must be disjoint.
The complete incoming curriculum report is revalidated before training, so
post-generation tampering with labels or counters fails closed.

The reference conjunction fixture is expected to tie because both curricula expose
the complete AND truth table. A tie is retained as evidence; RSI gain is not
inferred from task generation alone.

### RSI-2 — first closed candidate-cycle state machine

The first native loop state machine freezes an exact champion SHA, requires an
oracle-verified curriculum, freezes one distinct candidate head, binds the RSI-0a
manifest to that exact parent/head pair, and finally runs RSI-0b eligibility
against trusted current-main / PR-head observations.

Legal phases are:

`ChampionFrozen -> CurriculumVerified -> CandidateFrozen -> EvidenceBound -> ReviewedEligible|ReviewedIneligible`.

Main moving after champion freeze or PR head moving after candidate freeze makes
review fail closed. The cycle can render deterministic PR evidence, but
`self_promotion_authorized() == false` and `human_merge_gate_required() == true`.

This is a control-plane closed cycle. It does not yet train real model weights;
the later training experiment must plug an actual candidate trainer between
CurriculumVerified and CandidateFrozen while preserving these bindings.

## Archive and branching

Do not keep only a single winner lineage. Rejected and non-dominated candidate
evidence can remain available as bounded research stepping stones. However only
human-approved `main` is the canonical champion for the next recursive cycle.

## Failure modes

Track explicitly:
- self-confirmation;
- model/data collapse;
- reward hacking;
- benchmark contamination;
- catastrophic forgetting;
- evaluator drift;
- compute runaway;
- diversity collapse;
- PR/evidence manipulation;
- improvement-loop complexity growth.

A candidate that weakens the audit/promotion boundary is not considered an
improvement merely because downstream benchmark scores increase.

## Research context

Relevant external work motivates individual mechanisms rather than serving as
SML capability evidence:

- STaR (arXiv:2203.14465): iterative training on model-generated reasoning that
  leads to correct answers;
- Self-Rewarding Language Models (arXiv:2401.10020): model-generated judging and
  iterative preference training;
- Absolute Zero (arXiv:2505.03335): proposer/solver self-play grounded by a code
  executor that validates tasks and answers;
- Darwin Gödel Machine (arXiv:2505.22954): branching archive of empirically
  evaluated self-modified coding agents;
- Recursive Self-Improvement in AI survey (arXiv:2607.07663): taxonomy from
  bounded self-refinement to closed research loops and emphasis on verifier
  strength and human direction-setting.

SML-RSI deliberately remains human-gated at the GitHub promotion boundary even
while automation inside each candidate cycle increases.

## Scientific boundary

SML-GENIUS does not currently demonstrate open-ended recursive self-improvement.
The first objective is a reproducible, bounded improvement loop in which the
model can generate useful candidate PRs and training proposals that outperform a
frozen champion under independent evidence.

Multiple successful human-approved generations would demonstrate an engineered
recursive improvement process. They would still not prove unlimited or
self-sustaining intelligence growth.
