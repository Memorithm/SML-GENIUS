# DKV-4f — selective downstream decision observation

Status: Development observation under the frozen protocol in
`SML-DKV-4F-SELECTIVE-DECISION-PREREGISTRATION.md`.

Protocol commit: `84d8bc4e3ee5eb09472fb21e045c9df0eb542e0a`.
Executed code: `056405709f1310c45c2a3c82c8357bcaf640e5bb`.
Retained-evidence head before this observation:
`e42f41f060fe682d74f58cf5cb69524111718114`.
RemoteOps retained-evidence run: `37219410385`.
Host: `tarek`, aarch64. Rust: 1.89.0.
Rows: 8 scored task/state rows, plus 4 MOR geometry rows and 4 safety rows.
Two explicit executions were byte-identical.
Output SHA-256:
`e368fd9e8a14da6c569624596b3585d07b3ae87c384e8315f7b74f6a0a84b119`.

Complete retained evidence is in `research/dkv-4f/`. No MOR threshold,
proposal score, task oracle, recurrent limit or decision rule was changed after
observation.

## 1. Frozen native MOR geometry occurred

All four task families use the native
`MorRouterRegistry -> MixtureOfRouters` path.

For every task family the first route measured:

- margin: 40,000 ppm;
- precision escalation: true;
- selected experts: 2.

After carrying the favored primary and selected set through `PreviousRoute`,
the second native route measured:

- margin: 70,000 ppm;
- precision escalation: false;
- selected experts: exactly the favored singleton.

The preregistered route-resolution mechanism therefore occurred for both
PageId(0)-favored and PageId(1)-favored tasks.

## 2. Downstream decision result

The downstream expert semantics are intentionally simple:

- PageId(0) emits true;
- PageId(1) emits false;
- the memory target makes V[0] true;
- the emitted task answer is V[0] AND expert_output.

The task oracle is evaluation-only.

| arm | answered | correct among answered | errors | selective risk | recurrent cycles |
|---|---:|---:|---:|---:|---:|
| stability + SELECTIVE_SINGLETON | 0/8 | n/a | 0 | n/a | 12 |
| frozen DKV-4c + SELECTIVE_SINGLETON | 0/8 | n/a | 0 | n/a | 32 |
| dynamic DKV-4f + SELECTIVE_SINGLETON | 8/8 | 4/8 | 4 | 50% | 16 |
| stability + FIRST_PRIMARY | 8/8 | 4/8 | 4 | 50% | 12 |

The retained CSV stores `risk_ppm=0` for zero-answer arms as a serialization
sentinel because its denominator is zero. That value must not be interpreted as
measured zero error risk; those arms have no answered population.

Dynamic DKV-4f therefore increases coverage from zero to complete coverage under
the SELECTIVE_SINGLETON rule, but complete coverage is accompanied by four
errors.

## 3. Aligned and deliberately misaligned controls separate route certainty from truth

For the four aligned rows, dynamic DKV-4f answers all four and all four match
the oracle.

For the four deliberately misaligned rows, dynamic DKV-4f also answers all
four, but all four answers are wrong.

| dynamic subgroup | answered | correct | errors | selective risk |
|---|---:|---:|---:|---:|
| aligned | 4/4 | 4 | 0 | 0% |
| misaligned | 4/4 | 0 | 4 | 100% |

The terminal MOR evidence is equally resolved in both groups: 70,000-ppm margin,
precision escalation false and one selected expert. Consequently, native route
resolution is not a correctness or calibration certificate.

This is the principal negative result of DKV-4f.

## 4. Aggressive FIRST_PRIMARY control matches the same correctness pattern more cheaply

FIRST_PRIMARY uses the first native MOR primary after only the semantic
recurrence required by the stability control. It does not wait for singleton
route resolution.

It answers all eight rows and produces exactly the same four correct/four wrong
pattern as dynamic DKV-4f, while using 12 recurrent cycles instead of 16.

Thus the additional recurrent work that changes the route from a two-expert
selection to a resolved singleton does not improve task correctness on this
panel. It only makes the route evidence stricter before emitting the same
favored expert decision.

DKV-4f therefore does not establish an accuracy/cost advantage over this
aggressive matched control.

## 5. Stable versus one-repair recurrence

For every task family:

- STABLE uses depth 1 under stability-only and depth 2 under dynamic DKV-4f;
- CORRECT_ONCE uses depth 2 under both stability-only and dynamic DKV-4f.

The extra STABLE cycle is spent solely to resolve the native MOR precision
signal. In CORRECT_ONCE, route resolution fits inside the second cycle already
needed to confirm semantic stability.

Frozen-decision DKV-4c never receives the updated MOR evidence, so it consumes
the hard depth 4 and abstains on every row.

## 6. Provenance boundary remains unchanged

For every task family:

- Missing provenance: zero matrix reads and immediate abstention;
- Conflict provenance: zero matrix reads and immediate abstention;
- Unverified provenance: bounded depth 4 and abstention, never StopCandidate.

The downstream decision experiment does not weaken the source/provenance gate.

## 7. Frozen hypotheses

- **H1 holds.** Every first route measures 40,000 ppm / escalation=true /
  two selected experts, and every second route measures 70,000 ppm /
  escalation=false / one favored expert.
- **H2 holds.** First-route SELECTIVE_SINGLETON abstains on all eight rows.
- **H3 holds.** Frozen DKV-4c SELECTIVE_SINGLETON abstains on all eight rows.
- **H4 holds.** Dynamic DKV-4f answers all four aligned rows and all four are
  correct.
- **H5 holds.** Dynamic DKV-4f also answers all four misaligned rows and all
  four are wrong.
- **H6 holds.** FIRST_PRIMARY has the same aligned/misaligned correctness
  pattern and uses no greater recurrent depth; aggregate cycles are 12 versus
  16 for dynamic DKV-4f.
- **H7 holds.** STABLE dynamic rows spend one additional cycle over
  stability-only; CORRECT_ONCE route resolution fits within the two semantic
  repair/stability cycles.
- **H8 holds.** Missing/Conflict are zero-read and Unverified never
  StopCandidate.

## Decision

DKV-4f shows that genuine native MOR resolution can be connected to a bounded
downstream decision and can turn abstentions into answers. It also shows why
that fact is insufficient for model promotion: the same resolved route is
confidently wrong on every deliberately misaligned control.

The controller must therefore keep route resolution separate from decision
correctness. MOR margin, singleton selection and precision-escalation clearance
cannot substitute for an independent decision/calibration/verifier signal.

A stronger next experiment should add an independently sourced decision-risk
gate and test whether it can reject the misaligned rows without simply returning
to universal abstention. The risk gate must not receive the evaluation oracle,
and coverage must always be reported together with errors among answered rows.

No context/model promotion follows from this result.

**Scientific/model promotion remains BLOCKED.**
