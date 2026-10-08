# A4 — explicit SBG/ANF destination integration

Date: 2026-10-08. Status: staged functional integration, not model promotion.

PR237's `pvp6_recurrent::execute` remains a historical sequence-of-evaluations
reference: its output state is not fed into subsequent input queries. It must not
be described as a closed recurrent state/routing/memory loop.

The new path uses the existing `SemanticState` and `SemanticRewriter` contracts,
without adding a second halting engine or importing an external runtime.

## Wiring contract

`SbgAnfPlan<S,D>` projects explicit coordinates of V/M/A/R into log2(D)-bit ANF
addresses. It stores one source lane per query and query-major destinations
(lane * gate_count + gate). Every result has a unique destination; old-state
reads precede all writes. Unwritten coordinates and padding remain unchanged.
Invalid domains, ranges, duplicate inputs within a lane, duplicate destinations,
empty or inconsistent shapes and foreign batch shapes are rejected.

The present monomial carrier is u16; this contract accepts D=64..65536 powers
of two only. It does not claim qualification at D=1,048,576. Repeated queries
from identical projected addresses are valid. The plan is declared wiring,
not a learned feature extractor or a claim of semantic benefit.

## Ten-slice sequence

1. Checked four-field input/output projection.
2. State-fed ANF kernel implementing the native SBG rewriter and halting rules.
3. Atomic coefficient replacement with revision-bound cycle provenance.
4. Transactional composition of native semantic rewriters.
5. Explicit operation/trace budgets and no-mutation rejection.
6. Integration with the existing SBG-to-MOR engine.
7. Native Delta-KV/ANF/routing trajectory comparison against a direct control.
8. Bounded versioned four-field trace exchange and negative parser controls.
9. Model-owned checkpoint/continuation with identity and depth preservation.
10. Deterministic full qualification panel, replay evidence and integration report.

Each slice is independently tested and reviewed through a PR. Existing CI gates,
fixture identities and negative results are retained. New measurements, learning,
matched architectural ablations and capacity scaling remain separate gates.

## Ownership and reuse

SML owns the declared state-update semantics. A generic composition or trace
contract can serve other native SML mechanisms and external qualification
consumers, but does not force SciRust, FLAT-ATTENTION or NNIS runtime dependencies.
RemoteOps remains the reference physical control plane. No GPU, vendor SDK,
resident model service or separate SML-HARNESS is required for these CPU tests.

## Implemented functional qualification

The ten slices now have code and local tests; merge and hosted/native CI status
must be read from the exact PR/run receipts rather than inferred from this text.

`SbgAnfKernel` implements `SemanticRewriter`, regenerates addresses from the
current four-field state, and records queries, full outputs, Hamming deltas and
bank revision per cycle. `run_semantic_recurrent` remains the sole halting engine.
Every ANF rewrite uses unchanged-query reuse R=1; depth is actual state feedback.
Coefficient replacement validates the whole bank first. Identical replacements
are no-ops; a changed bank increments a checked local revision. No truth cache
survives an update. The revision is not a cryptographic artifact identity.

`SemanticChain` composes native stages by cloning both before execution and
committing them only after both return successfully. This rolls back owned state
on returned errors, not external side effects or aliasing through shared handles.
The qualified ANF and Delta-KV stages own their mutable vectors. The composition
is reusable by other native SML semantic mechanisms without PVP dependencies.

`SbgAnfBudget` checks cycle, gate-query and retained query/output payload-word caps
before evaluating a cycle. Counters are exact logical work/accounting; they are
not measured physical RAM, VRAM, instruction counts, or elapsed time. Container
metadata, allocator overhead, temporary clones and other stages are outside the
payload-word counter. RemoteOps retains host-level resource enforcement ownership.

`RoutedSemantic` applies the existing `SemanticMorBridge`, `MorRouterRegistry`
and `MixtureOfRouters` after a successful rewrite. It keeps the previous decision
for native MOR hysteresis and records the full route. A route error rolls back
the owned inner stage. It does not execute the selected expert page body.

The integration test `pvp6_sbg_dkv` compares native Delta-KV -> ANF -> MOR with a
literal-by-literal ANF control over 48 trajectories: three memory targets, four
initial value patterns and depth limits 1/2/4/8. It compares complete four-field
outcomes, halt reasons, deltas, routes, memory-read evidence, counters and ANF
outputs. It also checks downstream-failure rollback. Keys remain frozen within
each trajectory (DKV-2b semantics); provenance remains in the existing key evidence.

The `sml.a4.trace/v1` codec exchanges full V/M/A/R states and depth/revision
metadata in little endian. It checks bounds before allocation, exact lengths,
width, depth order, nondecreasing revisions and zero padding. It is a structural
codec, not authenticated storage: a valid payload edit requires expected-content
comparison or authenticated transport to detect. Tests include all truncations
of the small fixture, trailing bytes, huge counts and malformed padding.

The owned **in-memory ANF checkpoint** freezes state, bank, wiring, diagnostic
profile, budget, usage, cycle history and halting policy. Continuation keeps
absolute depth and the complete trace. Changed bank/wiring are rejected by the
checked resume API; already-completed stable snapshots add no work. This is not
serialization of a checkpoint, nor a checkpoint of the combined MOR/Delta-KV
pipeline. The earlier trace codec is not presented as that missing capability.

## Complete deterministic public-API panel

Example: `pvp6_sbg_qualification`. Golden report:
`research/evidence/a4-sbg-20261008/trajectory.csv` (915 lines, 310,218 bytes).
SHA-256: `5d7ef8fb5ec5e574e1a32fe0c94b67a6aafc41c7a3a94d00a5c73c80e63b8346`.

The declared panel contains 192 cases: lanes 1/32, two ANF families, three
**policy hints**, four fixed seeds, and depths 1/2/4/8. SBG width and ANF domain
are both 64 here. Inputs read all four fields; outputs feed back into V.
Fixed-depth runs use min_depth=max_depth, while separate unit tests exercise
adaptive stability. Every intermediate state, query and full output is compared
against an independent literal evaluator. The native recurrent runner must match
manual stepping exactly. Every case also checks owned checkpoint continuation
and complete trace encoding/decoding.

Expected complete counts are 720 cycles, 2,880 four-field state words, 11,880
canonical output words, 192 checkpoint replays and 192 trace roundtrips, with
zero mismatches. All three execution paths are exercised: 540 Pascal, 120 sparse
and 60 submask cycles. These counts are **not winner frequencies** or evidence
that the hinted populations match a physical measurement distribution.

The mandatory existing fixture script now runs this example twice, requires
byte-identical full reports, verifies the frozen golden hash and compares every
byte. The original PVP6 fixture hash and old five-arm/executor modes remain
unchanged. No CI gate, security rule or native timing protocol is relaxed.

## Remaining A4 and scientific gates

This completes only the declared synthetic integration checks. Dynamic key
reprojection, selected expert-page execution, broader model-owned/learned banks,
full pipeline checkpoint persistence, controlled end-to-end costs, learning,
matched architectural ablations and capacity scaling remain open. It does not
freeze a production architecture, reopen the rejected global/block-local PGF2
representation-mixing result, or establish attention-replacement quality.
The implementation stays Rust-owned and self-contained; partner repositories
remain optional qualification consumers, and the future SML-HARNESS stays separate.

The independent `scripts/verify-a4-sbg.rs` program reconstructs all golden states,
queries and outputs using plain four-word states and direct subset-membership
parity, without linking sml-core or using its projection, executor or recurrent
APIs. Its four negative-control tests reject missing/extra/reordered records,
changed state/query/output words and changed case/summary metadata. It verifies
trajectory content, not checkpoint behavior; the public-API panel separately
executes checkpoint continuation. Both checks remain mandatory in fixture CI.

Checkpoint guard: a changed bank revision that has not produced a completed
cycle cannot be captured against the previous state/stability trace. The API
returns `PendingBankUpdate`; a dedicated test verifies rejection without mutation.
This prevents an old stable delta from incorrectly skipping a new bank's execution.

Additional destination boundary target: `pvp6_sbg_boundaries` checks seven domains
(D=64,128,256,512,1024,2048,65536), four initial seeds, 65 simultaneous gates and
three cycles with coefficient replacement: 84 full cycles against a literal
oracle. It also writes the last bit of 65-bit V/M/A/R fields and rejects a mixed-
density replacement atomically. The target passed all three local tests. These
are exactness checks, not memory scaling or native latency evidence. The u16
carrier still does not qualify D=1,048,576.

Local final-batch qualification on native Thor/aarch64 with Rust 1.89.0 completed:
771 workspace tests across 110 successful suite summaries, zero failed or ignored;
workspace/all-target Clippy with warnings denied; formatting; unchanged PVP6
fixture and complete-output checks; two byte-identical A4 golden reports; and
four standalone independent replay tests. These are local functional observations;
exact-head hosted CI and integrated-native run receipts must still be consulted
before calling a particular PR or integration revision qualified.
