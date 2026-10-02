# DKV-3a-R1-A — independent projected-key recurrent input experiment

Status at creation: protocol frozen before implementation/execution of this new
panel. Development only, not a protected confirmation or model promotion.
Owner request: develop an independent implementation for later code comparison.
Base: `9a8828ba3f009822f040b01d6e1dca05cc85489e` (corrected source boundary, PR #99).

The Git commit creating this document is the protocol identity. Before running
the experiment, retain that commit, the exact candidate commit and a SHA-256 of
this document. Subsequent bug fixes require a disclosed amendment and retained
failed-run evidence; do not rewrite this original protocol after inspecting results.
No result is predicted or asserted by this file.

## Prior knowledge and ownership

The uploaded DKV-3a-0 patches and their negative results are known. Their random
key sweeps, source-lookup confounds, four-block distractor degeneracy and Q12
rounding limits are recorded in `SML-DKV-3A-0-LEGACY-OBSERVATIONS.md` and
`SML-DKV-3A-0-HARDENING.md`. This is a new panel, not a reproduction, replacement,
blind test, learned language benchmark or retrospective preregistration of those runs.
The owner's forthcoming implementation is not read or incorporated.

SML owns memory semantics and evidence. Rust-native, no new dependency, no Q/K/V
attention, no history attention scan, no DKV-2c dynamic re-keying. No RSI process
or protected final dataset is modified or executed. RemoteOps is the execution
control plane; branch-local bounded tasks record their host and exact revisions.

## Questions

Q1: does actual DKV-2a projection of observed SBG plus an actual native MOR
selection supply useful task information through DKV-2b recurrence compared with
no memory and direct keyed memory, on the same admitted input records?

Q2: with a frozen query, static matrix and deterministic expert, does repeated
matrix reading add anything beyond one cached reading? A cached-read control
is mandatory; a longer computation alone is not reasoning progress.

Q3: how do target recency, overwrite, conflict, missing records and nearby
semantic addresses affect latent errors, ambiguity and stopping?

Q4: does source verification remain independent of latent accuracy? Report
source-resolved answers separately, never replace a failed latent answer with a
raw-source answer in the latent quality metric.

## Frozen reference construction

- SBG width: 64 bits per V/M/A/R field. Identity occupies V[0..48); memory scratch
  coordinates are [48,49], decision coordinates [50,51]. Scratch/output initially
  false and ambiguous. The address constructor never receives the payload or
  expected answer. Meaning relevance is not treated as an observation mask.
- Two complementary R signatures route through SemanticMorBridge, the actual
  MorRouterRegistry and MixtureOfRouters. Two routers each specialize in one of
  PageId(0)/PageId(1); Top-1, min/max selected experts 1. PreviousRoute is reset
  per episode. The selected page must come from that engine, not a fabricated
  MorDecision. Routing occurs before projection and remains frozen within an episode.
- DKV-2a projector: existing DeltaKvSemanticProjector, default semantic/MOR
  weights (1,8), key dimensions [32,64,128], projector seeds [0,1,2]. Source
  identity is carried separately and must not change the projected key.
- Each record's write key is projected from its observed address-only state.
  The query is projected once from the initial address-only state, then frozen.
  No per-cycle re-projection and no answer bits in routing/projection inputs.
- DKV values: two Q12 components, +/-4096; full-rate normalized delta updates.
  DKV-2b default ambiguity threshold 128. Do not infer source existence or
  calibrated confidence from this threshold or the absence of ambiguity.
- Use the existing DeltaKvRecurrentKernel for fresh DKV reads. A wrapper performs
  the same deterministic two-input decision in every arm, using the PREVIOUS
  cycle's scratch values, so signal propagation and repeated reads are separable.
  Page 0 returns (x XOR y, x AND y); page 1 returns (x OR y, NOT(x XOR y)).
  Output stays ambiguous if either previous scratch bit is ambiguous.
- Adaptive halting: min 1, max 4 cycles, zero total V/M/A/R change threshold;
  unresolved scratch/output blocks stability but not the hard cap. Fixed arms
  have min=max=4. No parameter or threshold is tuned after this freeze.
- Direct control: native BoundedAssociativeMemory, 32 slots, 2-bit payload,
  ReplaceExisting, full observed subject/group identity as key. Expose/reject
  unintended hash collisions rather than silently changing the panel. Exact
  identity is available to all arms; direct versus compressed addressing is
  the declared representation difference. Latest-only conflict blindness is
  reported as a limitation, not evidence of universal DKV superiority.

## Development panel and arms

For every key dimension, seed and routing group [0,1], execute these eight
families, each with at most nine writes and a single subject-1 query:

1. isolated target;
2. target before seven unrelated subjects 2..8;
3. the same target after those subjects;
4. target overwritten by a later source with explicit predecessor identity;
5. two disagreeing sources for the target without supersession;
6. target missing; only subjects 2..8 are written;
7. target before seven address-near distractors;
8. target after those same address-near distractors.

Unrelated addresses use a fixed deterministic mixer of subject identity over
48 bits. Near addresses share the target identity pattern but flip two declared
identity coordinates depending on subject. Log the actual projected Gram
statistics; do NOT claim prescribed or dimension-invariant cosine. Distinct raw
addresses may collide after projection: count exact projected collisions.
Payloads are declared source data, including adverse distractors, not inferred
from expected outcomes. Supersession uses raw record identifiers, never substrings.
Fixture serialization/generator details are pinned in the implementation before
execution; changes to these details after execution are protocol amendments.

Six arms: no-memory adaptive, direct adaptive, direct fixed, DKV adaptive,
DKV fixed, DKV cached adaptive. This gives 3*3*2*8*6 = 864 paired rows. No-memory
is an ablation, not the strongest baseline. Cached DKV calls DKV-2b once and
reuses only its scratch value/ambiguity; all subsequent expert/halting logic is
identical. Fixed/adaptive comparisons use the same initial states and memory.

## Source authority and instrumentation overhead

Reuse the PR #99 exact-key, bounded source-evidence adapter in sml-store and the
CONTEXT raw-artifact parser/hash path. A verified receipt is not truth,
entailment, confidence or execution permission. No derived-only acceptance gate
is introduced. The task resolver reads typed verified bytes, checks subject and
record identity, retains unresolved conflict and applies explicit supersession.
A separate raw-only resolver over all admitted records is the authority control.
Expected answers are evaluated only AFTER recurrence; no oracle is a kernel input.

The harness may maintain one numeric matrix for the unchanged DKV-2b owner API
and one shadow matrix in the source adapter, plus direct memory and raw artifacts.
This duplication is instrumentation, not a deployment design: report all matrix
instances and shared setup work/storage separately. Verify the two numeric paths
agree before recurrence; never claim a one-matrix whole-system memory cost.
Source work shared between arms must not be disguised as per-arm savings.

Bound reference inputs: max 16 writes, 16 ledger segments, 32 source links,
32 semantic links, at most 4 sources per write, 1 MiB retained element and
1 MiB write-staging admission each. Verification max 16 sources/64 KiB raw bytes.
These are logical element/input bounds, not RSS/cgroup enforcement. Remote task:
2 Cargo build jobs, no GPU/training, 20-minute workflow cap. No production
services or shared working tree are modified.

## Evidence and predeclared checks

Retain deterministic CSV per row and per-cycle V/M/A/R deltas, actual selected
pages and frozen key identity. Record latent task correctness, wrong-known
answers, unknowns, depth/halt reason, Q12 read values, fresh read count/MACs,
projection/MOR calls, source outcomes, source bytes/segments and shared setup
costs. Record projected-key dot products and norms (exact integers), collisions,
matrix bits and separate source-adapter element plan. Logical work is not latency.

Assertions are software invariants only: actual MOR route and projection;
no target leakage; unchanged M/R and identity bits; frozen key; at most four
cycles; accounting; no forged proof; deterministic rerun; source isolation;
fresh/cached equivalence under this static-matrix contract; Q12/direct equivalence
on an isolated stored key (allowing numeric rounding tolerance for Q12 values).
Do NOT assert favorable quality, a winner or a speedup.

Quality hypotheses, reported even when false:
H1 DKV adaptive improves over no-memory on answerable cases.
H2 DKV adaptive is non-inferior to direct adaptive on every family/seed/group.
H3 cached and fresh adaptive paths have identical states/traces while cached
uses no more than one matrix read per episode.
H4 fixed depth cannot correct errors of a converged static-key adaptive path.
H5 near/older writes can cause latent wrong-known results even when raw authority
correctly abstains or resolves the source. Do not claim calibrated uncertainty.

Report all cells, including null/negative outcomes. No isolated seed determines
promotion. Repeated dimensions/seeds over the same tiny fixtures are not
independent language tasks; do not publish significance or generalization claims.

## Comparison handoff and promotion

Candidate A remains individually identifiable by code/protocol/output hashes for
comparison with the owner's implementation. Compare interfaces, source trust,
key construction, actual MOR execution, information access, recurrence, controls
and full costs before comparing headline accuracy. No counterpart code is needed
to execute A, and no winner is selected in advance.

Machine-readable context promotion remains BLOCKED: this bounded development
panel alone does not qualify full context integration, physical resources,
sidecar crash durability, learned representations or independent confirmation.
CI success is software conformance only. Preserve the original DKV-3a contract.
Any reusable lesson for TDI/SciRust/Forge/ElasticXxx/CCOS first requires destination
contract review; no speculative cross-repository dependency or code propagation.
