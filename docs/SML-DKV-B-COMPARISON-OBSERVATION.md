# Counterpart B comparison: retained observations and integration

Input identity: `research/dkv-b/INPUT.json`. Prospective comparison plan:
`SML-DKV-B-COMPARISON-PLAN.md`, commit `472d2f6805873ef8078f0fbe1f566a06b53a09ac`.
The new upload was cumulative; its old unchecked source gate was NOT applied.
Existing A/A2 examples, experiment protocols and all retained A/A2/A3 outputs
remain unchanged. A3's observed_history library is extended by subject binding
and internal ledger accessors; its existing screening behavior is unchanged.

## What the supplied code contributes

B varies exact state Hamming distance across V/M/R before DKV-2a, rather than
handcrafting only numerical keys. It compares target-old/target-recent writes
and probes DKV-2b on an unwritten target. These are useful controlled diagnostics.
Its important untested implication was that two distinct states with an identical
projected key could obtain the same source candidates. The integration now tests
that implication with a real DKV-2a alias and actually executed fixed-page MOR.

The full source probe is archived as supplied; executable changes are rustfmt
and a later replay regression only. Its reported Rust 1.97 results were not
assumed portable. The Rust 1.89 reproduction code is
`5241c2f46bdcfe1ed2b478b1a75e3d8b446f58e3`; retained tables are
`research/dkv-b/reproduced-1.89.txt`. All 12 geometric rows, 24 recency rows,
12 resolution rows and five hypothesis flags reproduce the supplied values.
In particular target-latest sign recovery is 8/8 throughout; target-old is 0/8;
all 96 absent-target trials resolve at least one bit. This is the supplied
classification, not a calibrated false-acceptance rate in deployed language.

## Differences from A/A2/A3 that prevent a headline ranking

| Aspect | Supplied B | Existing A/A2/A3 |
| --- | --- | --- |
| MOR | Constructed PageId5 decision, no router/ranking evidence | A/A2 execute native routers; A3 verifies observed history |
| State | Random V/M/R, 64 bits each; ambiguity initially zero | A/A2 address-only V plus reserved scratch/output |
| Query task | Recover two positive values directly in V[0,1] | A/A2 use a two-stage hand-authored expert Boolean circuit |
| Distance | Exact flips among 192 populated positions | A/A2 near-address generator and measured projected geometry |
| Payload | Fixed positive target, adverse negative distractors | A2 crosses all four payloads independently of seed |
| Missing | Any resolved bit called fabricated | Latent outputs kept distinct from source/history screening |
| Sources | Proposed alias risk, not exercised in B | PR99 receipts plus A3 exact SubjectKey history |

B's floor-isqrt overlap is retained as a legacy proxy, not exact cosine. New
KeyGeometry retains signed dot and both squared norms; no magnitude is an
authority signal. New Resolution2 separates Unknown/Partial/Complete so a
single resolved bit cannot be scored as an exact pair. The original partial
classification is not silently relabeled. B's H2 recency support and H3 failed
distance-recovery hypothesis can coexist; neither is evidence of model quality.
The supplied post-hoc extended-distance experiment is not reproduced here.

## New paired diagnostic

Executed code: `4fa5cc60d30b1a6764e0f7bba3563f9be9ce7383` on `debian`,
`rustc 1.89.0 (29483883e 2025-08-04)`.
Output SHA-256: `9a8eca9e3787db092c52250ca19be1762c1a140dfb9128757e3a5cce03eee4ef`.
Two explicit outputs were byte-identical. See `research/dkv-b/COMPARISON.json`
for exact provenance and the full CSV.

Routes: supplied fixed-page stub, native one-page5 conformance control, native
2-page MOR. Projection: standard and A2 complementary V[0..48) difference.
K=16/64, h=1/2/4/8/16/32, eight seeds, old/recent/missing: 1,728 rows.
This retains B's memory-output task, not A's downstream expert circuit. The
native one-page control executes the engine but cannot prove useful expert
competition. Two-page routing may change when R changes, and this is logged.
No source resolver participates in the diagnostic's latent score.

```
SUMMARY,native_dual,contrast,missing,rows=96,correct=0,partial=0,missing_resolved=96,rejected=0,reads=156,query_macs=12480
SUMMARY,native_dual,contrast,old,rows=96,correct=3,partial=0,missing_resolved=0,rejected=0,reads=157,query_macs=12512
SUMMARY,native_dual,contrast,recent,rows=96,correct=96,partial=0,missing_resolved=0,rejected=0,reads=180,query_macs=14400
SUMMARY,native_dual,standard,missing,rows=96,correct=0,partial=0,missing_resolved=96,rejected=0,reads=156,query_macs=12480
SUMMARY,native_dual,standard,old,rows=96,correct=0,partial=0,missing_resolved=0,rejected=0,reads=156,query_macs=12480
SUMMARY,native_dual,standard,recent,rows=96,correct=96,partial=0,missing_resolved=0,rejected=0,reads=180,query_macs=14400
SUMMARY,native_fixed,contrast,missing,rows=96,correct=0,partial=0,missing_resolved=96,rejected=0,reads=156,query_macs=12480
SUMMARY,native_fixed,contrast,old,rows=96,correct=3,partial=0,missing_resolved=0,rejected=0,reads=157,query_macs=12512
SUMMARY,native_fixed,contrast,recent,rows=96,correct=96,partial=0,missing_resolved=0,rejected=0,reads=180,query_macs=14400
SUMMARY,native_fixed,standard,missing,rows=96,correct=0,partial=0,missing_resolved=96,rejected=0,reads=156,query_macs=12480
SUMMARY,native_fixed,standard,old,rows=96,correct=0,partial=0,missing_resolved=0,rejected=0,reads=156,query_macs=12480
SUMMARY,native_fixed,standard,recent,rows=96,correct=96,partial=0,missing_resolved=0,rejected=0,reads=180,query_macs=14400
SUMMARY,supplied,contrast,missing,rows=96,correct=0,partial=0,missing_resolved=96,rejected=0,reads=156,query_macs=12480
SUMMARY,supplied,contrast,old,rows=96,correct=3,partial=0,missing_resolved=0,rejected=0,reads=157,query_macs=12512
SUMMARY,supplied,contrast,recent,rows=96,correct=96,partial=0,missing_resolved=0,rejected=0,reads=180,query_macs=14400
SUMMARY,supplied,standard,missing,rows=96,correct=0,partial=0,missing_resolved=96,rejected=0,reads=156,query_macs=12480
SUMMARY,supplied,standard,old,rows=96,correct=0,partial=0,missing_resolved=0,rejected=0,reads=156,query_macs=12480
SUMMARY,supplied,standard,recent,rows=96,correct=96,partial=0,missing_resolved=0,rejected=0,reads=180,query_macs=14400
PROMOTION,BLOCKED,no_source_or_model_qualification
```

Counts in each summary refer to 96 repeated synthetic cells, not 96 independent
real-world questions. Partial and missing-resolved metrics are separate.
Correct missing requires all output coordinates unknown. No threshold, grid or
key was retuned after output observation. Numerical keys, exact Gram values,
MOR/projector work, rejected keys, duplicate input states, per-cycle Q12 values
and V/M/A/R deltas are retained. Target-write/query reuse shares the deterministic
projected target key; projector counts reflect executed calls, not assumed ones.
The experiment moves one owned matrix into DKV-2b; report buffers, router/state
allocations, projection scratch and source infrastructure are outside matrix
coefficient bytes. Logical counters do not establish time or process RAM.

## Implemented source/subject boundary

`observed_history::bind_projected_subject` requires both the projected candidate
receipt and complete A3 VerifiedHistory from the SAME full ledger snapshot.
It binds an explicit independently observed SubjectKey, excludes unrelated
alias candidates, and requires every matching subject version. Missing subjects,
omitted versions, changed snapshots and limits fail closed. Conflicts remain
conflicts; a binding is neither correctness, source authenticity nor an action
grant. Existing screen_candidate can suppress unsupported outputs but must not
replace a wrong latent output with an observed value. The alias regression
shows why even full compressed-key equality is insufficient, beyond FNV issues.

Source payload parsing is still the bounded synthetic ObservationEvent grammar.
No general natural-language subject extraction or source entailment is claimed.
The complete-history verification cost remains explicit and is not a free
miss detector. Sidecar crash durability and physical resource limits remain open.

## Qualification and next work

Each slice requires full workspace Rust 1.89 checks and exact-head CI before
software integration. Frozen outputs gain regression tests after observation,
not retrospective scientific confirmation. No independent external review is
implied by these tests. Context promotion remains BLOCKED.

The next useful experiments should vary payloads, decision tasks and realistic
learned states under a new protocol, while retaining exact subject provenance.
Do not simply enlarge this grid until a favorable cell appears. Diagnostics
are reusable candidates for SciRust/TDI/Forge and binding rules for CCOS, but
this batch makes no cross-repository runtime change or qualification claim.
