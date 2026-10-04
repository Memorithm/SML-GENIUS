# Delta-KV projection-agreement input review

Status: owner-supplied research input triage. This document records what is
useful from the supplied `dkv-4.patch` without importing obsolete source
contracts or treating supplied results as current repository evidence.

Input identity:

- file: `dkv-4.patch`;
- size: 197,055 bytes;
- lines: 4,639;
- SHA-256: `8a3b27e5f8da743d7bfbe74ffe88bb4765d1c436ecca580b6f6cb08af0155847`;
- supplied surface: 21 files covering DKV-3a sidecar/projection experiments and
  an older DKV-4 controller experiment.

## Triage

### Rejected as implementation input

The supplied `sml-core::delta_kv_context` adapter is not imported.

Current `sml-store::delta_kv_context` is stronger: it uses bounded admission
and staging plans, exact full-key indexing, ledger snapshot binding, private
SHA-256-verified receipts, complete forward/reverse provenance checks and
fail-closed record/subject binding. Reintroducing the supplied FNV-key sidecar,
caller-shaped verification states or unbounded evidence vectors would regress
those guarantees.

The supplied DKV-4 controller is also not imported. DKV-4a through DKV-4g on
current `main` already provide typed Refine/StopCandidate/Abstain control,
zero-read Missing/Conflict handling, Unverified fail-closed behavior, executed
DKV-2b signals, MOR actionability, dynamic MOR recomputation and downstream
applicability separation. The older controller experiment is therefore
historical evidence, not a replacement.

The supplied key-alias finding is useful but no longer novel. Current
Counterpart B/C integration already demonstrates that compressed/projected key
identity is not semantic-record identity and that source attribution must be
bound to an independently qualified record/subject identity.

### Retained research mechanism: separately seeded projection agreement

The distinct mechanism worth carrying forward is agreement across multiple
separately seeded DKV-2a projections before treating a Delta-KV result as useful
derived semantic evidence.

The supplied experiment compares one Delta-KV member with unanimity across 3,
5 and 9 members and with exact/nearest-neighbour controls. Its supplied
observation reports:

- a single member answered many deliberately novel/boundary states, so one
  resolved Delta-KV read is not a miss/presence signal;
- unanimity converted many wrong/fabricated answers into abstentions;
- five-member unanimity still had non-zero false accepts in the supplied grid,
  reaching 13.0% on novel queries at C=32, K=256;
- nine-member unanimity reduced but did not eliminate false accepts in all
  configurations;
- a matched Hamming nearest-neighbour control dominated quality/coverage on the
  synthetic Hamming-ball panel;
- the only plausible remaining advantage for the Delta-KV ensemble is bounded
  memory/compute at write counts where an explicit neighbour store becomes
  materially more expensive.

These are **supplied patch observations**, not independently reproduced results
on the current Rust 1.89/current-main stack. In particular, differently seeded
projectors are not assumed statistically independent; agreement may remain
correlated through common SBG/MOR structure.

## Admissible use

Projection agreement may be evaluated only as a research-only compute or
abstention signal. It must not mean:

- source presence;
- semantic-record identity;
- calibrated confidence;
- correctness;
- authorization to accept an answer or external action.

For source-required tasks, current `sml-store` verification and complete
record/subject binding remain authoritative. Agreement cannot repair a wrong
latent result or bypass Missing/Conflict/Unverified boundaries.

A future controller integration, if qualified, should expose agreement only as
an observable bounded signal that may request more work or suppress a
candidate. It must not add an `Accept` action.

## Required current-stack replay

Before any runtime primitive is added, freeze and execute a current-stack
replay with:

1. Rust 1.89 and exact current repository APIs;
2. native DKV-2a projection and DKV-2b recurrent reads, without the supplied
   legacy context adapter;
3. separately seeded member counts 1/3/5/9 with explicit aggregate memory,
   projection and read-work accounting;
4. matched exact and nearest-neighbour controls, with the neighbour threshold
   frozen before execution;
5. written-neighbour, novel, boundary/conflict and overload classes;
6. current provenance/record/subject gates evaluated separately from latent
   retrieval quality;
7. write counts extending into the hundreds so a storage-matched crossover can
   actually be measured;
8. key-width sweeps that retain the unexpected K-dependence rather than assume
   wider keys improve false-acceptance behavior;
9. logical work plus physical latency/RSS/traffic measurements before any
   deployment claim;
10. immutable protocol/code/output identities and retained negative results.

Primary decision criterion: projection agreement is worth integrating only if
it improves a preregistered selective-risk/coverage frontier against simpler
controls at matched resource cost. Lower error obtained solely by collapsing
coverage is not a win.

## Relationship to active DKV-4 work

Do not insert this mechanism into DKV-4h learned applicability before its
current frozen experiment is completed. DKV-4h tests a different evidence
class: learned decision applicability from independently observed metadata.
Projection agreement concerns latent memory retrieval. Combining them before
separate qualification would destroy attribution.

If replayed successfully, projection agreement should enter a later independent
memory-evidence slice and then be compared with DKV-4/applicability under a
factorial ablation.

## Non-claims

This review does not authenticate the supplied preregistration chronology,
reproduce its Rust 1.97 executions, establish language/model quality, promote
Delta-KV context integration or establish probabilistic independence between
projectors. It intentionally preserves the current stronger provenance and
adaptive-control contracts.
