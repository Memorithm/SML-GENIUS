# Counterpart B: identity-preserving screening follow-up

This is post-observation engineering, not a new scientific preregistration.
Input patch SHA-256: d5ca8ee375d426e8059835b96d8230a6e5ff93308214db3ec42904d8136d2e80.
Baseline: 23552de1d76bcd87642c8c3c4e423cc2a342f57b.
The matching integration already exists in PRs 111 through 120; the audit
JSON records their actual merged heads and exact-head successful CI.
No duplicate application of the cumulative patch is appropriate.

## Additional API composition

`BoundProjectedSubject::screen(candidate)` consumes the verified subject
binding and uses that SAME HistoryQuery and immutable snapshot. There is
no second SubjectKey argument that could accidentally select an agreeing
alias instead of the conflicted subject originally bound. The existing
`screen_candidate(history, key, candidate)` remains available unchanged.
Both paths share one private screening implementation, preventing policy drift.

This reduces caller composition mistakes; it is not evidence of a deployed
exploit or an authentication boundary against arbitrary code in the process.
Missing subjects still fail at binding. Conflict suppresses the candidate.
Agreement preserves it even when wrong; unknown remains unknown. No observed
value is substituted. Candidate provenance/entailment is not certified by
passing a value into this method; it does not emit an action grant.

Three new tests use actual projected aliases, all four two-bit candidates
plus unknown, conflicting/agreeing neighboring subjects, and supersession.
A compile-fail doctest prevents supplying a replacement subject argument.
Existing retained probes and the paired comparison are replayed without
changing their output, hypotheses, seeds, thresholds or classification.

## Scope and remaining work

A/A2/B research outputs, numeric Delta-KV, dependencies and workflow rules
in SML-GENIUS are unchanged. Complete verified history has a cost; this is
not free source validation, a latency result, or a learned-model improvement.
Context promotion stays BLOCKED. Subject-bound APIs may benefit governed
memory elsewhere, but require destination review and qualification before reuse.
