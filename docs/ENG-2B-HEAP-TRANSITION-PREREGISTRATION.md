# ENG-2b/H1 — transactional in-process heap representation swap

**Frozen on 2026-10-13, BEFORE introducing ENG-2b/H1 implementation, fixtures,
tests or workflow.** SML-GENIUS owns model feasibility; ElasticXxx owns generic
resource-control contracts. This increment uses the already pinned
ElasticXxx core/EIR from sml-elastic-bench only; no dependency is added to
sml-core or to the SML default inference/runtime route.

## Purpose, distinct from ENG-2a

ENG-2a (PR #535) validates and unconditionally rolls back a shadow
VersionFrontier proposal, without mutating a resident model. ENG-2b/H1
shall actually **allocate a new Rust heap-resident model, swap the active
Box<ResidentNGram> to the new representation, verify post-swap, and either
commit the ElasticXxx VersionFrontier or restore the ORIGINAL resident Box
and previous frontier on failure**. This is a deliberately narrow,
single-threaded, in-process, opt-in model research adapter, not a generic
mmap/COW/page-cache/GPU actuator, a production service or a certified
memory-pressure controller.

Both representations are existing, immutable, exact same-Witten–Bell
estimators:
- ENG-1a complete suffix map, ElasticNGramLong.
- ENG-1b reversed-suffix trie, ElasticNGramTrie.
The target is **Recompute from retained identical TRAIN bytes**, not
Reencode. A true in-memory codec translating serialized counts between
formats is NOT part of this experiment. Context depth must remain fixed
at each commit; any change of depth is a model semantics change.

## Frozen scope, source ownership and admission limits

- One owned immutable TRAIN byte buffer, one active heap Box of the
  selected model representation, and a versioned ElasticXxx frontier.
  Source IDs cannot be guessed: the transition caller supplies exact
  TRAIN bytes for full byte-for-byte comparison and an expected epoch.
  The verifier's source is immutable between commits. There is no
  cryptographic external source/provider signature and no host identity.
- Validated depth 0..32, up to **8192 TRAIN next-byte positions**, plus
  all existing model-specific node/follower/context-key budgets.
  Same TRAIN layout: 64 contiguous lanes, positions t>=1, no cross-lane
  history. A full FIT that exceeds any cap must fail without a published
  resident snapshot.
- One independent 64-lane probe with nonidentical bytes to TRAIN and no
  more than **8192** scored positions. It is used only to cross-check
  identical 256-way predictions at the same fixed depth.
  This is **not** a generalization TEST dataset and is not used to select
  a model or depth.
- Reject source mismatch, zero/underlength corpora, stale expected epoch,
  epoch overflow, invalid/changed depth and any requested
  Reencode/Reinterpret as **typed errors before allocation and stage**.
  Same-format target is a no-op; no epoch increment, no heap replacement.
- On a valid FullSuffix <-> SharedTrie switch, build a fresh target model
  using EXACT retained source bytes. Verify every 256-class prediction
  (f64 bit patterns) over every TRAIN and independent probe target
  against the active source, plus exact probe score denominator/bpb/
  accuracy and finite positive normalized distributions. A mismatch
  must refuse BEFORE physical swap.
- Instantiate a VersionFrontier proposal using a new, depth-bound
  representation ID at schema 1 and Recompute, validate structural
  capabilities/source availability. Attestation only says source
  exists; it does **not** attest quality, physical budget or codec.
- **Physical mutation:** replace active Box with newly allocated target
  while retaining the prior Box as rollback buffer. After the swap,
  recompute all TRAIN/probe full-vector checks against the saved prior
  model, including exact probe score. Commit the matching frontier
  only if verification succeeds.
- Failure at **after-swap**, **after-postverify-before-commit**, and
  **after-frontier-commit** must restore the original Box object
  and original VersionFrontier (including epoch), leaving inference
  behavior and model/source identity unchanged. Injection hooks are
  private to test module, NOT part of the public non-test API.
  A poisoned target fitted on different bytes must fail equivalence
  before any physical swap. No unsafe memory operations.
- Success releases the old Box only after successful commit, advances
  epoch exactly once, and exposes the new target representation through
  actual subsequent inference. A reverse switch back is a separate
  Recompute from the retained immutable source; no prior model state
  or lost count precision is silently recovered by Reencode.
- This is **one mutable object behind &mut self**, not a live,
  lock-free, multi-reader process or a crash-durable transaction.
  Panic/OOM/power loss, process crash, unsaved payload, concurrent
  readers and external devices are explicitly UNQUALIFIED.

## Predefined scientific & engineering fixtures / falsifiers

1. Synthetic 64x40 byte patterned TRAIN and distinct 64x40 probe;
   depth 12; initial FULL, switch to TRIE with Recompute; confirm
   old/new measured distributions exactly equal on every TRAIN/probe
   context, active format changed, epoch 0->1; verify direct
   subsequent inference uses the trie.
2. Reverse TRIE->FULL, epoch 1->2; full probability equality and
   source bytes retained; same-format no-op retains both active
   resident pointer and frontier epoch.
3. At depths [0,4,7,12,32], verify full 256-way parity
   and post-swap inferability. For >12 fixture history must be long
   enough to exercise the requested depth (64x40).
4. Change TRAIN byte, change proposed depth, stale epoch, u64::MAX
   epoch, Reencode or Reinterpret, same-as-source/missing/invalid
   probe and >8192 observed positions MUST fail-closed and leave
   resident pointer, format, epoch and predictions unchanged.
5. Inject three independently staged failures AFTER actual Box swap,
   AFTER post-swap successful verification and AFTER frontier
   COMMIT. For each verify same prior resident allocation address,
   prediction bits and epoch, with NO pending transition. Repeated
   valid switch must remain possible after rollback.
6. Poison freshly materialized target model using different
   (same-size) source bytes before the pre-swap oracle: MUST
   fail on probability mismatch, never temporarily become active.
7. Original EL-NG1, ENG-1a, ENG-1b and ENG-2a oracles unchanged and
   green on exact PR head. Frozen A6-E4/E5 scientific content unchanged.

## Data / resource measurements and interpretation

Report resident representations, epoch and checked probability cells.
Logical context/follower counts are declared as logical accounting.
No hard RSS budget, no VmHWM/malloc measurement or savings claim in H1:
double-residency during switch can INCREASE peak RSS. A later
ENG-2b/H2 would need a trusted provider and hard budgeting for transient
old+new resident memory, version/source authenticity, independent
physical allocation/fault tracking, fail-closed crash-durable restore
and safe live-reader synchronization.

Model quality must not change at fixed depth; model semantics changes
such as adaptive depth, lossy precision/reencoding or quality-budget
tradeoffs need separate preregistration and independent heldout loss.
Fresh FR/EN corpus full TRAIN currently exceeds old <=500000-event
ENG-1b capacity; this 8192-event gate DOES NOT solve it.

## CI and staged promotion

Pin Rust 1.89. Dedicated PR workflow runs on GitHub-hosted Ubuntu only:
format, check, strict Clippy, full ENG-2b tests, old ENG-2a and
SML-CORE ngram regressions; also full repository CI. Neither
workflow runs code from a PR on persistent Thor runners. Merge
only after all general/dedicated tests are GREEN on the exact final SHA.

A pass authorizes only **in-process opt-in heap swap, observed with
explicit rollback under tested error paths**. No default inference
route, no generic ElasticXxx core modification, no physical RAM-budget
admission, no production or SML25B promotion.
References:
- https://github.com/Memorithm/SML-GENIUS/pull/535
- https://github.com/Memorithm/ElasticXxx/issues/244
- https://github.com/Memorithm/SML-GENIUS/issues/530
