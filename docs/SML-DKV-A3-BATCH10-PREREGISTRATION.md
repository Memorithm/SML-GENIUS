# DKV A3: observable history and frozen-read engineering qualification

Status at creation: prospective protocol, before implementation or execution of
this tranche. Development/conformance only. Parent is
`84eb36cecb389999c5fbfdaee9ddccb6f557fd08` (A2, PR #101).
The creating Git commit is the protocol identity. Do not edit this protocol
post-observation; disclose any amendment separately and preserve failed runs.

## Known evidence and question

A and A2 are known, retained diagnostics, not blind confirmation. A2's standard
and complementary projections still emitted determined outputs for missing and
contradictory records. Latest-only direct memory missed contradictions too.
Repeated static-key/static-matrix reads in A did not improve state trajectories.

This tranche asks whether an explicitly budgeted, complete *observed* history
can identify absence/conflict independently of a model's numeric output, and
whether an immutable-borrow cache can eliminate redundant numeric reads without
changing any numeric value. These are different questions from improving latent
memory or learning semantic representations. Old/near-key retention failures are
kept visible, not repaired by secretly substituting source values.

The owner's forthcoming implementation remains independent and is not consulted.
No change to A/A2 examples, original protocols or retained outputs is admitted.
No DKV-2c re-keying, attention, training, new final/protected evaluation, source
truth claim, action authorization or scientific/context promotion is in scope.

## Ten independently reviewable slices

1. Freeze this protocol and the scope/merge contract.
2. Bounded observation log with typed exact subject/group identity, append-only
   records and explicit supersession; all ordinary errors preserve logical state.
3. Exact-key indexed, generation-borrowed history queries and complete active
   support/conflict summaries; missing is relative to the admitted history only.
4. Strict canonical raw-event wire grammar and limits, with independent parser
   tests and exact numeric/string boundaries.
5. Complete raw-ledger verification adapter: preflight source/byte budgets,
   SHA-256/length checks, canonical event/semantic-link binding, private receipts.
6. Candidate screening: missing/conflicting verified histories suppress a latent
   candidate; agreement never repairs, authenticates or authorizes that candidate.
7. Immutable borrowed Delta-KV numeric read cache with explicit hit/fresh-read
   accounting and compile-fail mutation/forgery boundary tests.
8. Matched A3 diagnostic over the frozen A2 latent output, generated anew by the
   unchanged A2 executable, plus canonical independently generated source events.
9. Retain complete evidence, replay/SHA/integrity tests and cache/retention controls
   with numerical error and workload counts separated from physical measurements.
10. Reproducible comparison handoff, machine-readable blocked promotion evidence,
    bootstrap/roadmap reconciliation and cross-project qualification boundaries.

Each PR must have a scoped diff, working tests and successful required SML CI
on its exact head before merge or opening the next PR. Failed software tests
block that slice; a negative scientific finding does not get tuned into success.
No empty or cosmetic PR is created to reach the batch count. A blocked slice
is reported honestly rather than bypassed or merged with failing checks.

## Observation contract

Subject identity is (positive u64 subject, u32 group), compared exactly, not by a
compressed projection or diagnostic hash. Each observed record has a positive
monotonic u64 identity, the subject/group key, two Boolean source values, and
an optional explicitly superseded record identity. A superseded record must be
an earlier, currently active record with the same exact key. An event cannot
silently replace an unrelated record. Multiple agreeing active records are
retained; distinct active values are a conflict even when a later downstream
Boolean function would map them to the same output.

Wire grammar, one record per raw segment:
`record_id<TAB>subject<TAB>group<TAB>bit0<TAB>bit1<TAB>predecessor_or_dash`.
Integers are canonical unsigned decimal (no leading zero except group zero),
bits are exactly 0 or 1, predecessor '-' means none. Reject trailing fields,
whitespace, noncanonical integers, invalid UTF-8 and oversized records. The raw
segment's semantic forward list must be exactly the parsed record identity.

The verified adapter processes *all* segments of one admitted ledger snapshot.
Only a successful complete verification may mint its private receipt. Bounds
are checked before the first reader call; no partial success escapes. The reader
is a trusted bounded I/O boundary. The receipt proves byte identity against the
admitted SHA-256 ledger, not source authenticity, entailment or world truth.
Missing means no matching record in that verified finite snapshot, not global
nonexistence. A receipt/query borrows its owner and cannot survive owner mutation.

Limits cover event count, exact-key count, total logical element payload, raw
segment count and raw bytes. They do NOT claim allocator/RSS/cgroup limits,
crash durability, persistence of the new observation index or constant-time
history reconstruction. Retaining source values is real additional memory and
information; it must not be described as cost-free metadata.

## Prospective diagnostic design

Reexecute the unchanged A2 report, requiring its existing frozen output identity.
Use all 576 episode identities (dimensions 32/64/128, seeds 0/1/2, groups 0/1,
four payloads and eight families) and its three arms: 1728 paired rows. Decode
candidate output only from the latent field, never from the gold field. Raw
observations are generated from the declared A2 fixture rules and payload, then
serialized, parsed and independently verified. The decision code receives only
the exact query identity, verified history and candidate; it never receives the
family, payload/gold label, expected outcome or raw answer as a replacement.

For each row preserve latent output/quality, observed status (missing,
agreement, conflict), screened candidate, and separate evaluator correctness.
Missing/conflict -> unknown; an already unknown candidate stays unknown;
agreement -> unchanged candidate, including wrong candidates. This is evidence
screening, NOT a learned rejection policy or an improved latent Delta-KV result.
The absence/conflict benefit is expected largely by construction and is not an
independent generalization finding. Do not claim superiority over a direct store
that has access to the same complete history.

Retain all negative old/near-old cells. Distinguish correct-known, wrong-known,
correct-unknown and wrong-unknown partitions. Correct and unknown totals may
overlap; their union must not be mistaken for disjoint outcome categories.
Retain per-arm and per-family counts, source events/bytes and verification work.
All arms share this additional source information and its costs. Original latent
scores and A2 hashes must remain unchanged. No threshold selection is allowed.

## Cache and retention controls

A cached read borrows both matrix and key immutably for its full lifetime.
One numeric read supplies an immutable cached value; later hits do not invoke
matrix arithmetic. Fresh-read counters, cache hits and cached element bytes are
separate. A source receipt is never synthesized from that cached numeric value.
Tests compare exact Q12 values, including zero keys, interference, rounding and
invalid shapes; byte-for-byte equality is required, not just sign agreement.
Mutation requires dropping the cache and obtaining a new one. Numeric work is
reported, not translated into a speedup or physical memory claim.

Retention tests must include a target overwritten by later correlated writes
and a direct exact-history control with the same admitted records. A successful
cache test proves preservation of the existing value, even when that value is
wrong. It is not an improvement to old-memory retention. Any algorithm claiming
to fix that failure requires a separate prospective study with matched budgets.

## Evidence and gates

Protocol/code/output SHA-256 identities, runner/toolchain, complete output and
repeatability checks are retained. Rust only for repository implementation;
RemoteOps is the remote execution plane. Task-local checkout, at most two Cargo
build jobs, no GPU/training, no production services or shared checkout changes.
Software builds may use the existing standard CI regression suite unchanged.

Passing conformance is not context promotion. The emitted decision remains
BLOCKED while learned-key/recurrent quality, source-safe integration, independent
confirmation, crash recovery and full physical resource costs remain unqualified.
The final comparison handoff must enumerate known limitations and actual merges,
not equate ten PRs with scientific progress. Reuse candidates are typed-history
contracts for CCOS, evidence validators for TDI/Forge/SciRust and truthful cost
accounting for ElasticXxx. Destination contract review/tests precede propagation;
this tranche introduces no speculative cross-repository runtime dependency.
