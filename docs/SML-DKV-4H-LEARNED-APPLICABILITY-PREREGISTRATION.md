# DKV-4h — learned expert-applicability preregistration

Status: frozen before implementation/execution.
Base: `e8d0fa2778c5a3593ed93db8961edc6871b9d2fe`.

## Motivation

DKV-4f showed that native MOR route resolution can increase coverage while
remaining confidently wrong on deliberately misaligned controls. DKV-4g then
showed that an independently supplied allowed-expert set can suppress those
mismatches, but that set was explicit fixture metadata rather than a fitted
decision rule.

DKV-4h tests the next narrower question: can a bounded applicability rule be
selected from training data only, pass a separate validation gate, and then
suppress incompatible routed proposals on an untouched test panel without
receiving the task oracle, expected answer, MOR margin or Delta-KV correction?

This is still a finite Boolean identification experiment. It is not language
generalization or calibrated risk estimation.

## Ownership boundary

Training and inference live in the typed decision layer.

- Delta-KV supplies bounded recurrent state repair only.
- MOR supplies native route evidence only.
- DKV-4 supplies bounded recurrent-compute control only.
- The applicability learner receives independently observed semantic metadata
  and page-specific applicability labels on training/development data.
- Test-time task oracle and final Boolean correctness are evaluation-only.

The learned gate may preserve or suppress a routed proposal. It may not alter
MOR, rewrite the selected page, substitute another expert output, repair the
Boolean answer or authorize an external action.

## Candidate semantic features

Use one immutable `SemanticProjection<64>` with eight declared
`ContextValue` coordinates, in this slot order:

```
slot 0 -> V[0]
slot 1 -> V[1]
slot 2 -> V[2]
slot 3 -> V[7]   # hidden applicability feature A
slot 4 -> V[9]
slot 5 -> V[13]  # hidden applicability feature B
slot 6 -> V[21]
slot 7 -> V[34]
```

All projected coordinates are independently observed, relevant and unambiguous
on train/development/test rows unless a row belongs to the explicit missing-
metadata safety control.

Training is allowed to search only among these eight declared features.

## Latent data generator

Every underlying metadata record has four balanced latent bits `a,b,c,d`.
Each split covers all 16 `a,b,c,d` combinations exactly four times, for
64 records per split.

The hidden applicability features are fixed:

- V[7] = a;
- V[13] = b.

The remaining six candidate coordinates are nuisance transformations of
`c,d,repeat,split` and mixed functions involving one hidden bit plus nuisance.
They must not contain a deterministic duplicate of the pair (a,b).

Training, development and test use disjoint record-id ranges and distinct
split-specific nuisance transforms. The hidden applicability relation is not
changed between splits.

All V/M/A observation masks required by `ContextValue` are present for normal
rows. M is relevant and A is clear on all declared candidate coordinates.

## Page-specific applicability target

Two experts are used.

For PageId(0):

```
applicable_0 = (a == b)
```

For PageId(1):

```
applicable_1 = (a != b)
```

These labels are available only in the training/development applicability
datasets and as evaluation truth on the untouched test set.

The runtime gate is not hard-coded with XNOR/XOR. It receives the proposal
returned by training-only feature/table selection.

## Training-only selection and development gate

For each page independently:

1. run `search_feature_pair_expert` on the 64 training records;
2. the search may choose one feature pair among the eight declared candidates
   and one of the 16 native two-input truth tables;
3. use a fixed deliberately weak baseline consisting of slots [0,1] with AND;
4. only after the pair/table is frozen, evaluate the 64 development records;
5. development may accept or reject the proposal, but may not select a
   different pair/table.

Use `FeatureSearchPolicy::default()` and `ProjectionBudget::default()`.
No threshold or candidate set may change after observing development/test
results.

The frozen hypothesis is that training selects slots [3,5] for both pages,
with XNOR for PageId(0) and XOR for PageId(1), and that development accepts
both proposals with zero errors and zero missing selected features.

If a different pair/table is selected, that result is retained; the panel must
not be edited to recover the expected pair.

## Untouched routed test panel

The untouched test split contains 64 metadata records and is crossed with both
semantic recurrence states:

- STABLE;
- CORRECT_ONCE.

Total routed test rows: 128.

For each metadata record:

- expected applicable page is PageId(0) when a==b, else PageId(1);
- route alignment is determined by latent bit c:
  - c=true -> MOR favors the applicable page;
  - c=false -> MOR favors the opposite page;
- therefore exactly half of test metadata records are aligned and half
  deliberately misaligned;
- the evaluation task oracle is the Boolean output of the expected applicable
  expert:
  - PageId(0) -> true;
  - PageId(1) -> false.

The oracle is not visible to feature search, the learned applicability gate,
MOR, Delta-KV or DKV-4.

## Native MOR and recurrence

Reuse the DKV-4f native route geometry and policy:

- native `MorRouterRegistry -> MixtureOfRouters`;
- two singleton router groups;
- favored proposal = 1,000,000 ppm;
- other proposal = 920,000 ppm;
- `base_expert_mass_threshold_ppm = 530,000`;
- first route expected margin 40,000 ppm, precision escalation=true,
  selected-expert count=2;
- second route after `PreviousRoute` hysteresis expected margin 70,000 ppm,
  precision escalation=false, selected-expert count=1.

Every route geometry value must be measured from the native engine.

The DKV target remains V[0]=true. PageId(0) emits true and PageId(1) emits
false. Routed task answer remains `V[0] AND expert_output`.

## Learned applicability inference

For a routed singleton page:

1. choose that page's accepted frozen `FeatureExpertProposal`;
2. project only its two selected semantic features from the untouched test
   record;
3. if either selected feature is unavailable, abstain;
4. evaluate the frozen truth table;
5. true -> preserve the routed proposal unchanged;
6. false -> abstain as predicted inapplicable.

No test label, test oracle or final task correctness is visible to this path.

## Comparison arms

For every untouched test row compare:

1. DYNAMIC_UNGUARDED:
   dynamic native MOR + DKV-4d, no learned applicability gate;
2. DYNAMIC_LEARNED:
   same dynamic route/recurrent output + learned page-specific gate;
3. FIRST_PRIMARY_LEARNED:
   stability-only semantic recurrence + first MOR primary + the exact same
   learned page-specific gate;
4. UNIVERSAL_ABSTAIN.

The learned gate object is identical between dynamic and FIRST_PRIMARY arms.

## Missing-metadata safety control

Add 16 extra inference-only rows outside the scored 128-row task panel.

For these rows, clear observation of at least one selected applicability
feature while keeping route evidence available.

Required behavior:

- learned applicability must abstain;
- no fallback to MOR margin, expert identity, task oracle or a default value;
- these rows do not enter the 128-row task-quality denominator.

## Frozen hypotheses

H1: both page-specific training searches are accepted by development without
retuning.

H2: selected proposals are [3,5] + XNOR for PageId(0) and [3,5] + XOR for
PageId(1).

H3: on untouched test data, DYNAMIC_UNGUARDED answers all 128 rows and is wrong
on the 64 deliberately misaligned rows.

H4: DYNAMIC_LEARNED answers the 64 aligned rows, suppresses the 64 misaligned
rows and has zero errors among answered rows.

H5: FIRST_PRIMARY_LEARNED has the same coverage/error pattern as
DYNAMIC_LEARNED at no greater recurrent cost.

H6: UNIVERSAL_ABSTAIN has zero coverage and explicitly undefined selective risk.

H7: the 16 missing-metadata safety rows all abstain.

H8: Missing/Conflict provenance remains zero-read abstention and Unverified
never StopCandidate.

H9: native MOR first/second route geometry remains the preregistered
40,000->70,000 ppm transition for all scored routed rows.

All failed hypotheses are retained as failed.

## Measurements

Retain:

- training/development record counts and disjoint id ranges;
- candidate features/pairs and selection-evaluation work;
- selected feature slots/coordinates/truth tables per page;
- training/development errors, missing counts and coverage;
- untouched test rows and route geometry;
- answered count, coverage ppm, correct answered, errors, selective-risk ppm;
- explicit `risk_defined` for zero-answer arms;
- aligned/misaligned subgroup metrics;
- recurrent cycles and matrix reads;
- missing-metadata abstention counts;
- complete protocol/code/output identities.

## Interpretation boundary

Even if all hypotheses hold, the learned component identifies a finite Boolean
compatibility rule over synthetic metadata. It does not establish language
generalization, probabilistic calibration, semantic truth, long-context quality
or a model-level gain.

Because both pages are trained over complete Boolean input coverage, untouched
test success is evidence of pipeline separation and robustness to the frozen
nuisance shift, not evidence of extrapolation beyond the finite function class.

If FIRST_PRIMARY_LEARNED matches DYNAMIC_LEARNED more cheaply, the correct
conclusion remains that the learned applicability boundary helps while the
additional dynamic MOR cycle does not add task value on this fixture.

A stronger later experiment must use representative learned states and a
genuinely held-out task distribution whose compatibility labels are not a
complete finite truth-table enumeration.

No context/model promotion follows from this protocol.

**Scientific/model promotion remains BLOCKED.**
