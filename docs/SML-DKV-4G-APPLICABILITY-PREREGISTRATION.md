# DKV-4g — independent applicability gate preregistration

Status: frozen design before implementation/execution.
Base: `b7ce286d2c295e0ebd40fb630dd8712dad75136c`.
Depends for interpretation on the retained DKV-4f selective-decision evidence
currently under review in PR #156. This protocol must not merge before that
evidence is merged unchanged.

## Motivation

DKV-4f connects a genuinely resolving native MOR route to a downstream Boolean
decision. On its frozen development panel, route resolution increases
SELECTIVE_SINGLETON coverage from 0/8 to 8/8, but the resolved singleton is
wrong on all four deliberately misaligned controls. FIRST_PRIMARY produces the
same correctness pattern with fewer recurrent cycles.

Therefore, MOR route certainty cannot serve as a correctness or calibration
certificate.

DKV-4g tests a separate question: can a downstream, independently supplied
**applicability constraint** reject expert-domain mismatches without receiving
the evaluation oracle?

## Ownership boundary

The new gate belongs to the typed decision layer, not MOR and not Delta-KV.

Its input is an explicit bounded set of expert pages permitted for the decision
record by an independently observed task/schema constraint. The constraint is
not learned from the test oracle, route margin, MOR precision flag, Delta-KV
value, expected answer or evaluation label.

For this synthetic panel the allowed expert set is frozen as fixture input
before execution. In a future real system it would need provenance of its own
(e.g. schema, policy or independently verified task metadata).

MOR remains responsible only for routing. DKV-4 remains responsible only for
bounded recurrent compute. The decision layer may abstain even when MOR has a
resolved singleton.

## Frozen task construction

Reuse the four DKV-4f task families and both STABLE/CORRECT_ONCE semantic
fixtures, with the same native MOR scores and same Boolean expert semantics:

- PageId(0) emits true;
- PageId(1) emits false;
- V[0] target is true;
- routed answer = V[0] AND expert_output.

Add an independent allowed-page constraint:

1. ALIGNED_TRUE:
   - MOR favors PageId(0);
   - allowed set = {PageId(0)};
   - evaluation oracle = true.
2. ALIGNED_FALSE:
   - MOR favors PageId(1);
   - allowed set = {PageId(1)};
   - evaluation oracle = false.
3. MISALIGNED_FALSE:
   - MOR favors PageId(0);
   - allowed set = {PageId(1)};
   - evaluation oracle = false.
4. MISALIGNED_TRUE:
   - MOR favors PageId(1);
   - allowed set = {PageId(0)};
   - evaluation oracle = true.

The allowed-page set is visible to the applicability gate but the oracle is
evaluation-only.

This is deliberately a typed compatibility fixture, not a learned-language
verifier.

## Applicability rule

A routed decision may be emitted only when all of the following hold:

1. source/provenance state is independently VerifiedPresent;
2. the recurrent controller terminates with StopCandidate;
3. the current MOR decision selects exactly one expert;
4. that selected expert is present in the independently supplied allowed set.

Otherwise the decision layer abstains.

The applicability gate must not repair or substitute an answer. It only
preserves or suppresses the routed proposal.

The allowed set is bounded, duplicate-free and non-empty. Unknown/out-of-range
pages fail closed.

## Comparison arms

For each task/state pair compare:

1. dynamic DKV-4f SELECTIVE_SINGLETON without applicability gate;
2. dynamic DKV-4g + applicability gate;
3. stability + FIRST_PRIMARY + the same applicability gate;
4. universal abstention control.

All routed arms use the same native MOR inputs, DKV memory, recurrent budget and
expert-output semantics.

## Frozen hypotheses

H1: the independent applicability gate does not alter native MOR geometry or
DKV recurrent traces.

H2: dynamic DKV-4g answers all four aligned rows and those answers are correct.

H3: dynamic DKV-4g abstains on all four deliberately misaligned rows.

H4: dynamic DKV-4g therefore has 4/8 coverage and zero errors among answered
rows on this synthetic panel.

H5: FIRST_PRIMARY plus the same applicability gate has the same coverage/risk
pattern at no greater recurrent cost. If so, the applicability gate is useful
but dynamic MOR resolution still has no task-quality/cost advantage on this
fixture.

H6: universal abstention has zero coverage; it is retained so zero selective
risk cannot be reported without the coverage denominator.

H7: removing the applicability gate reproduces the DKV-4f 8/8 coverage and
4/8 errors.

H8: Missing/Conflict provenance remains zero-read abstention; Unverified never
StopCandidate.

All failed hypotheses must be retained as failed.

## Measurements

Report per arm and subgroup:

- records;
- answered count and coverage ppm;
- correct answered count;
- errors and selective-risk ppm;
- abstentions by reason;
- recurrent cycles and matrix reads;
- terminal MOR margin, selected expert and precision flag;
- allowed expert set and whether selected page passed applicability;
- final Boolean output where emitted.

For a zero-answer arm, selective risk is undefined. Serialized machine-readable
output must carry an explicit denominator/defined flag rather than silently
presenting zero risk as an empirical rate.

## Required implementation properties

- typed bounded allowed-set API in `sml-decision` or an equally appropriate
  downstream decision module;
- no dependency from `sml-core` back into `sml-decision`;
- no oracle, expected output or task-family label enters the gate;
- deterministic Rust 1.89 implementation;
- complete all-targets Clippy/tests before execution;
- two byte-identical executions with retained code/protocol/output identities.

## Interpretation boundary

Success would establish only that **new independent applicability information**
can reject the particular expert-domain mismatches that MOR certainty alone
cannot detect. It would not establish calibration, learned verification,
language-model accuracy or a general unknown detector.

If FIRST_PRIMARY + applicability matches DKV-4g at lower cost, the correct
conclusion is that the applicability boundary helped while the extra dynamic
MOR cycle did not add task value on this fixture.

A later learned-risk experiment would require train/development/test separation
and must not reuse this synthetic panel as an untouched final evaluation.

No context/model promotion follows from this protocol.

**Scientific/model promotion remains BLOCKED.**
