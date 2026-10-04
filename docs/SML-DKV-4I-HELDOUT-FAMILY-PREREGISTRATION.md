# DKV-4i — held-out task-family lightweight-expert gate preregistration

Status: frozen before implementation/execution.
Base: `85fefd4515ccd219df319f1cd771c968db320604`.

## Motivation

DKV-4h demonstrated train/development/test separation for a learned
applicability gate, but its hidden relation was still a completely enumerated
two-input Boolean function.

DKV-4i moves the applicability problem onto the existing trained SML task
candidate and asks whether a gate trained on several task families can identify
when a lightweight expert that has **no keyed associative memory** is safe to
use on two task families withheld entirely from applicability fitting.

This is a held-out-family applicability experiment. It is not a claim that the
underlying SML candidate itself has never seen those task semantics: its
micro-operation training remains the existing multi-task SML training path.
The family holdout applies specifically to the downstream applicability gate.

## Base candidate

Build one verified six-family curriculum and train the existing
`train_multitask_candidate` path under `MultiTaskTrainingPolicy::default()`.

The curriculum contains:

- DelayedBitRecall;
- OverwriteBitRecall;
- KeyedFactRecall;
- CopyAfterMarker;
- TwoFactConjunction;
- DistractorRejection.

Use these exact base-curriculum proposals, 64 cases each:

- DelayedBitRecall: start 1,000,000, difficulty 7;
- OverwriteBitRecall: start 1,100,000, difficulty 7;
- CopyAfterMarker: start 1,200,000, difficulty 8;
- TwoFactConjunction: start 1,300,000, difficulty 7;
- KeyedFactRecall: start 1,400,000, difficulty 7;
- DistractorRejection: start 1,500,000, difficulty 7.

Use `CurriculumPolicy::default()` and `MultiTaskTrainingPolicy::default()`.

The resulting learned hard micro-operations and event router are frozen before
applicability training.

Two execution experts are then constructed with exactly those learned hard
micro-operations:

1. FULL:
   - ordinary direct associative memory;
2. LIGHT:
   - identical router and learned hard micro-operations;
   - `AssociativeMemoryMode::DisabledZeroRead`.

No expert may receive applicability labels, route alignment, test oracle or
DKV/MOR confidence as an input.

## Applicability target

The LIGHT expert is applicable only when the FULL reference execution requires
no keyed associative-memory operation:

```
light_applicable =
    full.memory_reads == 0
    AND full.memory_writes == 0
```

This target is computed only for applicability training/development labels and
for final evaluation. It is not available to the inference gate.

FULL is the conservative fallback expert and is always considered applicable.

This target is operational resource semantics, not answer correctness.

## Applicability features

The LIGHT gate receives only two pre-execution Boolean features:

1. `has_fact_event`:
   true iff the task event stream contains at least one `TaskEvent::Fact`;
2. `case_parity`:
   low bit of case_id, included as a frozen nuisance feature.

It receives no:

- TaskKind;
- expected answer;
- solved oracle;
- candidate predicted answer;
- memory counters;
- MOR score/margin;
- Delta-KV correction;
- route-alignment label.

The gate is one hard `TruthTable2`, selected by the existing
`fit_boolean_expert` path.

## Applicability train/development families

The gate sees only four families:

- DelayedBitRecall;
- OverwriteBitRecall;
- KeyedFactRecall;
- TwoFactConjunction.

Training and development each include all four families with exactly 64
examples per family.

Training ranges, all at difficulty 5:

- DelayedBitRecall: 2,000,000-2,000,063;
- OverwriteBitRecall: 2,100,000-2,100,063;
- KeyedFactRecall: 2,200,000-2,200,063;
- TwoFactConjunction: 2,300,000-2,300,063.

Development ranges, all at difficulty 11:

- DelayedBitRecall: 3,000,000-3,000,063;
- OverwriteBitRecall: 3,100,000-3,100,063;
- KeyedFactRecall: 3,200,000-3,200,063;
- TwoFactConjunction: 3,300,000-3,300,063.

Thus fitting sees 256 training and 256 development applicability examples.

The label for every example is the operational LIGHT-applicable target computed
from a FULL execution.

Use a FALSE incumbent and `FitPolicy::default()`.

Frozen expected relation:

- tasks without Fact events do not require keyed associative memory;
- tasks with Fact events do.

Therefore the expected hard table is NOT(left), independent of the nuisance
case-parity input. The experiment records the actual selected table and does
not change the feature pair if this hypothesis fails.

Development can reject but cannot retrain/reselect.

## Entirely held-out applicability test families

The applicability learner never sees either of these families before final
evaluation:

1. `CopyAfterMarker`
   - expected operational class: LIGHT applicable;
2. `DistractorRejection`
   - expected operational class: LIGHT not applicable.

Use exactly:

- CopyAfterMarker case IDs 4,000,000-4,000,063, with payload length
  `1 + (case_id mod 16)`;
- DistractorRejection case IDs 4,100,000-4,100,063, with distractor count
  `1 + (case_id mod 16)`.

This yields 64 held-out task instances per family. Crossing both recurrent
state fixtures produces 256 scored routed rows.

Before scoring, execute FULL on the held-out tasks and retain its exact
task-result and keyed-memory counters as evaluation evidence only.

The held-out task-family claim applies only to applicability fitting; it does
not imply that the underlying trained SML candidate is family-blind.

## MOR routing control

For every held-out task, choose the MOR-favored expert using only case-id parity:

- even case_id -> favor LIGHT/PageId(0);
- odd case_id -> favor FULL/PageId(1).

This makes route choice independent of TaskKind, expected expert, task oracle
and applicability truth.

Use the native DKV-4f MOR geometry:

- first route margin 40,000 ppm;
- precision escalation true;
- two selected experts;
- second route under PreviousRoute hysteresis margin 70,000 ppm;
- precision escalation false;
- one selected favored expert.

All geometry must be measured through native
`MorRouterRegistry -> MixtureOfRouters`.

## Recurrent control

For every routed task evaluate both semantic recurrence fixtures:

- STABLE;
- CORRECT_ONCE.

Use the same bounded DKV-2b target and DKV-4d dynamic MOR controller as DKV-4f/h.
The recurrence is a control-plane gate for route resolution; task execution
itself occurs only after an expert is selected and permitted.

## Inference arms

For every held-out task/state row compare:

### DYNAMIC_UNGUARDED

Dynamic MOR resolves to a singleton. Execute the selected LIGHT or FULL expert
without the learned gate.

### DYNAMIC_LEARNED

Dynamic MOR resolves to a singleton.

- FULL -> always permitted;
- LIGHT -> evaluate the frozen learned applicability table using
  `has_fact_event` and `case_parity`;
- permit=true -> execute LIGHT;
- permit=false or feature unavailable -> abstain.

The gate never substitutes FULL after rejecting LIGHT.

### FIRST_PRIMARY_LEARNED

Use the first native MOR primary after only the existing stability-only
recurrence. Apply the **same** learned gate:

- FULL always permitted;
- LIGHT requires learned permit.

### FULL_REFERENCE

Always execute FULL. This is an accuracy/resource reference, not a routed arm.

### UNIVERSAL_ABSTAIN

Answer nothing. Its selective risk must be explicitly undefined.

## Scoring

Task correctness uses the existing TaskInstance expected answer only after the
arm emits an answer.

Report separately:

- task exactness;
- coverage;
- errors among answered rows;
- selective risk with explicit defined flag;
- LIGHT/FULL execution counts;
- keyed memory reads/writes performed by executed experts;
- DKV recurrent cycles/reads;
- held-out family subgroup metrics.

An abstention is not counted as a correct task answer.

## Missing-feature safety

Add 16 inference-only rows where `has_fact_event` is explicitly unavailable
to the learned LIGHT gate.

Required behavior: LIGHT selection abstains. There is no fallback to FULL,
case parity, MOR confidence or a default feature value.

These rows are outside the held-out task-quality denominator.

## Frozen hypotheses

H1: multi-task SML training converges and its FULL candidate is exact on all
held-out CopyAfterMarker and DistractorRejection test instances.

H2: the applicability fit is accepted on development and selects a table
equivalent to NOT(has_fact_event), independent of case parity.

H3: on held-out CopyAfterMarker, the learned LIGHT gate permits every normal
LIGHT route.

H4: on held-out DistractorRejection, it rejects every normal LIGHT route.

H5: DYNAMIC_LEARNED eliminates task errors caused by unsafe LIGHT routing
without using test task oracle or execution counters at inference.

H6: FULL routes are never suppressed by the gate.

H7: FIRST_PRIMARY_LEARNED has the same task-quality/coverage pattern as
DYNAMIC_LEARNED at no greater recurrent cost. If so, dynamic MOR still has no
task-value advantage under the same verifier.

H8: FULL_REFERENCE has maximal task coverage and establishes the resource cost
of always using the conservative expert.

H9: all missing-feature LIGHT controls abstain.

H10: Missing/Conflict provenance remains zero-read abstention and Unverified
never StopCandidate.

H11: all scored routed rows retain the native 40,000 -> 70,000 ppm MOR
transition.

All failed hypotheses remain reported as failed.

## Interpretation boundary

This experiment is stronger than DKV-4h because the applicability gate is
trained on four task families and evaluated on two entire families that were
absent from applicability fitting. The features are pre-execution structural
properties rather than task answers.

It remains a synthetic task-suite experiment. The FULL underlying candidate is
trained through the existing SML multi-task curriculum, which includes current
task micro-operation semantics. A successful applicability holdout therefore
does not establish open-domain language generalization, calibrated uncertainty
or model-level superiority.

If the learned gate succeeds but FIRST_PRIMARY_LEARNED matches it more cheaply,
the correct conclusion remains that the verifier generalizes across the held-out
task families while dynamic MOR resolution adds no task value on this panel.

No context/model promotion follows.

**Scientific/model promotion remains BLOCKED.**
