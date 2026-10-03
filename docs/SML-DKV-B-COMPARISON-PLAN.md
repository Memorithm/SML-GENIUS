# Counterpart B: comparison and integration plan

Base: `047b49556af0aaad9a8197c071247729094366b4`.
Uploaded `dkv-3a-0.patch`: 99,281 bytes, 2,462 lines, SHA-256
`d5ca8ee375d426e8059835b96d8230a6e5ff93308214db3ec42904d8136d2e80`.
The new part is the 355-line DKV-3a-1 projected-key probe and its two documents.
The rest is cumulative legacy DKV-3a-0 code/documentation. Do not apply the old
unchecked sml-core source gate or overwrite current bootstrap/history work.
The upload declares prior execution on Rust 1.97.0, not checked on pinned 1.89.
Its original preregistration chronology is a supplied declaration, not verified
Git history. Its known results are disclosed below, not prospective predictions.

## Comparison before new execution

B controls Hamming distance over V/M/R (192 bits), K=16/64, h=1/2/4/8/16/32,
eight seeds and five adverse writes. It uses real DKV-2a and DKV-2b but constructs
a PageId(5) decision with empty router/ranking evidence. This is a fixed-route
ablation, NOT execution or qualification of native MOR. A and A2 execute MOR.
A2 complements V[0..48), doubles projection calls and reports norms and aliases.
A3 already provides complete verified history indexed by exact SubjectKey.

B reports 8/8 target-latest sign successes and 0/8 target-old successes in all
12 K/h cells, plus 8/8 resolved unwritten targets. At h=1 it reports32/40 aliases
for K=16 and17/40 for K=64. These are supplied results pending reproduction.
B's overlap divides by a floored integer square root; retain it as a legacy
proxy, not exact cosine. B counts one resolved correct coordinate as
answered_correct: do not reinterpret it as complete two-bit accuracy.
H2 recency can hold while H3 distance recovery fails; the source protocol's
instruction to reject transfer when either fails is inconsistent with that
separation. Preserve the reported hypotheses separately, not that implication.

## Engineering slices

1. Freeze comparison scope and input identity before new executions.
2. Reproduce the supplied probe on workspace Rust1.89; retain exact stdout.
3. Add checked exact key geometry (signed dot and both squared norms).
4. Distinguish partial resolution from exact pair output in diagnostics.
5. Add controlled Hamming fixtures and actual MOR routing controls.
6. Bind projected-key candidate sources to an exact observed SubjectKey.
7. Test real projection aliases against receipts and the complete A3 history.
8. Run the separately identified native-MOR differential diagnostic.
9. Retain and replay output with explicit code/protocol/toolchain identities.
10. Record findings, limits and next work in bootstrap/roadmap.

No old A/A2/A3 protocol or retained output may be rewritten. No production
numeric rule, protected evaluation, threshold tuning or learned-model promotion.
Tests assert software invariants, not favorable scientific outcomes. Any fixes
following a failed run must be disclosed with the failed evidence retained.

## Frozen differential diagnostic design

Route modes: supplied PageId5; actual one-router/one-page5 engine (conformance
control); actual two-router/two-page0/1 engine with complementary even/odd R
signatures and Top1. V/M/R fields use B's seeded generator and distance grid.
Projectors: unmodified DKV-2a; complementary (p(s)-p(T(s)))/2 with T flipping
only V[0..48), as in A2. All modes retain the actual selected page per state.
Changing R may change the two-page decision; report rather than force it.
K=16/64, h=1/2/4/8/16/32, seeds0..7, target-old/target-recent/target-missing:
3 route modes * 2 projections * 2 K * 6 h * 8 seeds * 3 orders =1728 rows.
Full-rate Q12, target(+4096,+4096), five opposite-value distractors, default128
ambiguity threshold, frozen query, native DKV-2b max depth2. Log exact pair and
partial outcomes, keys/geometry, aliases, duplicate input states, counters and
route/projection work. This is a new diagnostic, not a blind confirmation or
headline accuracy comparison with the differently constructed A/A2 panels.

## Subject binding contract

An identical compressed vector is a source candidate, never a subject identity.
Binding requires a complete verified A3 history on the SAME full ledger snapshot,
an explicit exact SubjectKey independent of that vector, and all source versions
for the subject. Unrelated candidates are excluded and counted. Missing subjects,
omitted matching versions, forked snapshots and budget violations reject.
Conflicts remain conflicts. Byte identity does not establish authenticity,
entailment, calibrated confidence or permission to act. No observed value may
replace a failed latent output. Screening may only preserve or suppress it.

## Execution and reuse

Use RemoteOps, pinned fresh task workspaces, Rust1.89, two Cargo jobs and no GPU.
Validate exact-head CI before each PR merge. No host services or credentials are
changed. Logical work/storage limits are not RSS/latency enforcement evidence.
Diagnostics may benefit SciRust/TDI/Forge; subject binding may benefit CCOS.
Destination-specific qualification is required before propagation; no external
runtime dependency is introduced. Context promotion remains BLOCKED.
