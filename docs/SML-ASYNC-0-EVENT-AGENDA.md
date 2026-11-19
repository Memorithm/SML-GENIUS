# SML-ASYNC-0 — bounded event agenda and direct dirty-word ingress

Status: opt-in **model-owned experimental executable**. No default SML decoder,
native MOR, training algorithm or physical executor is replaced.

## Architectural direction

The future SML model shall be asynchronous, event-driven and hyper-sparse at
the **native model layer**, not merely in an external service or an attention
emulation. The objective is to execute only causally activated circuits and
memory work while retaining a complete fallback for dependencies that cannot
yet be proved irrelevant. There is no Q/K/V, all-history score matrix, or
mandatory runtime dependency on FLAT, SciRust or NNIS.

`crates/sml-core/src/async_sparse.rs` implements an independently testable
initial substrate:

- `AsyncSparseAgenda`: bounded logical-time agenda, stable (time, sequence)
  ordering, atomic burst admission, explicit backpressure and single-event
  polling; no artificial dense per-tick full-network traversal.
- `SbgSparseEventIngress`: consumes the existing native
  `SbgRoutingDirtyJournal::flush` output, detects exact rising/falling
  coordinates using **only changed packed words**, and submits events as an
  indivisible burst. Absent writes produce no bitfield scan.
- Non-R events (`ContextInvalidation`, `MemoryInvalidation`, `Wake`) are
  explicit; a held-high R with no new rising edge is **not** a licence to
  suppress native MOR/recurrent/memory work.
- Failure in event admission preserves both ingress baseline and journal
  pending state; caller-owned handlers must be transactional on error.
- The logical single-thread agenda is a deterministic reference for a
  **future** asynchronous executor. It is not parallel execution and has
  no hardware-performance claim.

## SML-ASYNC-1 — opt-in causal generation guard

\`async_sparse::versioned::VersionedSparseAgenda\` is a functional,
bounded wrapper around the existing deterministic agenda. Before submission,
the model owner registers a monotonic \`(source_id, generation)\`. A complete
batch is accepted only for that source's current generation, preserving its
logical time and event order. An old queued event becomes stale if the source
advances; the guarded poll retires it **without calling the handler** and
increments a separate \`stale_retired\` counter. Other sources remain eligible.
The original \`AsyncSparseAgenda\` API and unguarded semantics are unchanged.

The wrapper reports committed events separately from stale retirements and
preserves a live event and its stamp on handler failure. Regressing generations,
unknown sources and stale submissions are rejected without partial admission;
a full queue remains bounded. Five new Rust tests cover stale retirement,
independent sources, retry, rejection and empty batches.

**Safety boundary:** advancing a generation is only valid when the caller owns
the *complete* invalidation set for that source and will reemit any required
memory/context/recurrent work in the new generation. Source stamping alone
does not justify skipping native MOR or any model dependency. This is not yet
wired into the actual SBG mutation boundary and is not a physical speed or
quality result. Model parity and a complete mutation-source audit are required
before enabling it in the runtime.

## SML-ASYNC-2 — native tracked R writer to causally stamped ingress

The optional `async_sparse::causal_bridge::SbgCausalEventBridge` now
composes **actual model-owned R point writes** via
`SbgRoutingDirtyJournal::set`, exact sparse rising/falling edge extraction,
and `VersionedSparseAgenda` with causal source generations. Unlike a
full-snapshot compatibility adapter, point writes record only their changed
packed word at mutation time; whole-R `replace` explicitly scans every word.
The original unversioned ingress and native MOR execution remain available
and unchanged.

The journal, previous acknowledged packed words and bounded stamped queue
advance **only after successful admission**. A full queue, invalid padding,
out-of-order word metadata or failed source generation admission therefore
preserves the original journal for retry. Five focused tests exercise
ordered true R edges, stale-generation retirement, full-queue rollback,
explicit memory invalidation and handler retry.

The public bridge also admits `ContextInvalidation`,
`MemoryInvalidation` and `Wake` events independently of R fronts, and
rejects manual rising/falling events that could bypass the writer.

**Promotion boundary:** this bridge owns its own R field; the existing
`RoutedSemantic` / `CoupledSbgLoop` mutator has **not** yet been
replaced with it. In particular, generation changes do not automatically
prove that held-state, memory and non-R invalidations have been reemitted.
Model-level equivalence, complete mutation ownership and a safe fallback to
full native MOR are required before using stamped event retirement in
production. No speed, memory-bandwidth, energy or trained-model gain is
claimed.

## Gaps before architectural promotion

1. **Real model integration**: place ownership at the actual mutator of SBG R
   and every V/M/A, memory, registry and runtime-policy invalidation source.
   Dense full-snapshot conversion cannot be renamed sparse computation.
2. **Causal correctness**: compare native always-route/always-reason against
   event-driven execution with source-generation identities, stale-event
   protection, deterministic replay and no required page omissions.
3. **Bounded fan-out**: admit neuron/synapse/circuit activation only under
   explicit work, queue, memory, fairness and backpressure limits. No silent
   event coalescing that loses causal transitions.
4. **Learning**: evaluate local event-conditioned update and credit assignment
   against matched dense / fixed-sparsity controls and held-out task quality.
5. **Physical evidence**: on RemoteOps-governed machines measure model-quality,
   actual bytes read/faults, page residency, active parameter touches,
   end-to-end latency, energy when available, and queue overhead. Compare
   idle, sparse, adversarial dense-burst, held-state and memory-invalidated
   workloads. Establish a practical active-fraction envelope before scale.

The active fraction is `parameters_touched_per_emitted_token /
total_learned_parameters` (with repeated touches counted separately).
A design target, **not measured evidence**, is 0.01%-0.1% unique active
capacity per token for a 25B-class candidate, but total touches, read traffic,
model accuracy and recurrence must also satisfy independently frozen gates.

## Reproduction

```bash
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked -p sml-core async_sparse
cargo test --locked --workspace
```

Merge requires green required CI at the exact head. Nothing in this module
qualifies 25B scale, physical speedup, energy savings or model-output parity.

Related: [REA-1](SBG-REA-1-RISING-FRONT-MOR.md),
[REA-3](SBG-REA-3-LEARNED-EDGE-ROUTING.md),
[REA-4](SBG-REA-4-NATIVE-DIRTY-WORDS.md).
