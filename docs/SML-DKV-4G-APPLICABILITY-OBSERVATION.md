# DKV-4g — independent applicability gate observation

Status: Development observation under the frozen protocol in
`SML-DKV-4G-APPLICABILITY-PREREGISTRATION.md`.

Frozen protocol commit: `9e93c37fc81415e2de1ebb780cce59a9f522ff86`.
Protocol copy on the execution stack:
`844dc880423e244431c0a683a7c110bef3024afc`.
Executed code: `7dc897a896cca57965c4a4a707ae8b3a864fab6e`.
Retained-evidence head before this observation:
`bfaaab66438a993855ce311a95ca861f0edc500f`.
RemoteOps run: `37220626949`.
Host: `debian`, x86_64. Rust: 1.89.0.
Rows: 8 scored task/state rows. Two executions were byte-identical.
Output SHA-256:
`f3befcec45d793f78b0ce6efbf936b1572ace6edf083562ad84cd449008fc3cd`.

No task oracle, expected Boolean answer, MOR score or Delta-KV correction enters
the applicability API. The gate sees only the selected expert page and a
bounded independently supplied allowed-expert set.

## Result

| arm | answered | coverage | correct answered | errors | selective risk | cycles |
|---|---:|---:|---:|---:|---:|---:|
| dynamic DKV-4f, no gate | 8/8 | 100% | 4/8 | 4 | 50% | 16 |
| dynamic + applicability | 4/8 | 50% | 4/4 | 0 | 0% | 16 |
| FIRST_PRIMARY + applicability | 4/8 | 50% | 4/4 | 0 | 0% | 12 |
| universal abstention | 0/8 | 0% | n/a | 0 | undefined | 0 |

The machine-readable output carries `risk_defined=false` for the universal
abstention arm. Its serialized `risk_ppm=0` is a placeholder value and must
not be interpreted as measured zero empirical risk.

## Aligned versus deliberately misaligned rows

The allowed-page constraint agrees with the MOR-favored page on the four aligned
rows and disagrees on the four misaligned rows.

The dynamic applicability arm:

- preserves all four aligned proposals and all four match the evaluation oracle;
- suppresses all four misaligned proposals;
- never substitutes the allowed expert's Boolean output for the routed proposal.

Thus the reduction from four errors to zero is attributable to **additional
independent applicability information**, not to stronger MOR certainty or an
answer repair.

The unguarded dynamic arm remains at the retained DKV-4f 4/8 correctness pattern.

## Matched aggressive control

FIRST_PRIMARY receives the same independent allowed-page constraint. It obtains
the same 4/8 coverage, four correct answered rows and zero answered errors while
using 12 recurrent cycles versus 16 for dynamic route resolution.

On this fixture, therefore, the applicability boundary is useful but the extra
dynamic MOR cycle is not.

This is an important separation:

- DKV-4f established that route resolution alone is not a correctness signal;
- DKV-4g establishes that independent task/schema applicability information can
  reject the deliberately mismatched route;
- neither result establishes a task-quality advantage for additional dynamic MOR
  recurrence over FIRST_PRIMARY when both receive the same independent guard.

## Software boundary

`sml-decision::applicability::AllowedExpertSet` is bounded and canonical:

- empty sets fail closed;
- duplicate pages fail closed;
- out-of-range pages fail closed;
- maximum allowed-set size is explicit;
- `screen` only preserves or suppresses the caller's proposal.

The decision layer owns this constraint. It does not modify MOR, Delta-KV or
source provenance.

Missing/Conflict provenance remains a zero-read abstention boundary in the
development panel. Unverified evidence never reaches StopCandidate.

## Frozen hypotheses

- **H1 holds.** Applicability is downstream of the retained route/recurrent
  evidence and does not modify those traces.
- **H2 holds.** Dynamic + applicability answers all four aligned rows correctly.
- **H3 holds.** It abstains on all four misaligned rows.
- **H4 holds.** Coverage is 4/8 with zero errors among answered rows.
- **H5 holds.** FIRST_PRIMARY + the same gate has the same coverage/risk pattern
  with lower recurrent cost: 12 versus 16 cycles.
- **H6 holds.** Universal abstention has zero coverage and explicitly undefined
  selective risk.
- **H7 holds.** Removing the applicability gate reproduces 8/8 coverage and four
  errors.
- **H8 holds.** The fail-closed provenance boundary remains intact.

## Decision

The bounded applicability primitive is retained as a useful downstream
composition mechanism. It demonstrates why route confidence and decision
applicability are separate evidence classes.

It is **not** a calibrated risk estimator, learned verifier or language result.
The synthetic allowed-page input is stronger information than MOR alone and
requires independent provenance in any real deployment.

The next scientific step is not to tune MOR thresholds against this panel. It
is to evaluate a separately sourced or learned decision-risk/applicability
signal with train/development/untouched-test separation on representative tasks,
and to compare dynamic MOR against FIRST_PRIMARY under exactly the same verifier.

No context/model promotion follows.

**Scientific/model promotion remains BLOCKED.**
