# BAF-2 — Joint Boolean memory focus by task-loss coordinate search

**Scope:** scalar Rust experiment, native SML-owned semantics. No attention vectors, Q/K/V, similarity rankings, softmax or token-pair matrices. No model default or external-runtime change.

## What changes from BAF-1

BAF-1 fits one page-local Boolean ANF activation. BAF-2 fits **several pages in one global TRAIN objective**, using the same BAF-0 page-execution oracle for all candidates. Page selection, strict deterministic priority/tie-breaks, conflicting write masks and fixed page payloads are faithfully evaluated **together** rather than pretending page activation errors add independently.

An activation for page j is a GF(2) polynomial of the raw event and recurrent SBG state bits. Each candidate flips one ANF coefficient for one editable page. The best **strict** total-TRAIN error reduction is adopted per round. Search order is deterministic (page ID ascending; constant term, degree-1, degree-2 features). All other page properties remain frozen.

The observed objective is complete next-state Hamming loss:

`L_train = sum_{(e,s,target) in TRAIN} popcount(BAF_0(model,e,s).next_state XOR target)`

The held-out validation panel is opened only after TRAIN candidate discovery has terminated; a candidate must improve TRAIN strictly and not regress validation against the untouched baseline. Otherwise it is rejected atomically and the original BAF-0 model is returned.

## Hard research bounds

- Initial monomials of editable pages must be empty; frozen, non-editable pages may contain declared ANFs.
- Max total raw event + state input width 16 bits; page field widths remain those of BAF-0.
- Configured max editable page count (default 4), max degree 2, max rounds 8, max monomials per page 8, max feature bank 136, max TRAIN+VALIDATION examples 8192 and max worst-case TRAIN candidate evaluations 5000.
- Candidate-budget preflight fails before search, including overflow; no hidden truncation.
- IDs of TRAIN/VALIDATION examples and exact (event,state) inputs must not overlap. This is **not** a proof against semantic aliases across different input representations, correlated examples, shared teachers, or temporal leakage.
- Search sees only TRAIN target next-state words. It never receives teacher activation labels or TEST/OOD sequences. Validation is a final veto; it must not be used iteratively to retune hyperparameters.
- Due to greedy single-coefficient moves, jointly necessary changes with no independently beneficial step can remain undiscoverable. This is a **negative capability boundary**, not a learned global optimum.
- Page competition is real: higher-priority writes win bits claimed before lower-priority pages; the model's selected page order is observable.

## Exact tests

1. Synthetic two-page non-linear tasks with disjoint write bits and final-state-only supervision; held-out nuisance event bit; require zero train and validation error.
2. Entire-model validation regression forces rollback even if multiple learned pages improve train.
3. Duplicate page IDs, unknown editable page IDs and overlapping exact train/validation input pairs fail closed.
4. Candidate compute-cap preflight fails closed.
5. Priority conflicts: a frozen high-priority Boolean rule wins overlapping writes while a lower-priority activation is learned from task outcome rather than teacher route labels.

These are correctness/expressivity fixtures only. They do not substantiate downstream model quality, language modeling, OOD generalization, lower memory, speed or energy claims.

## Next qualification

- Independently frozen train/dev/test/OOD generators and multi-seed baselines using the existing `hybrid_logic_engine` (Hamming page routing), unconditional pages, native MOR/ANF, and degree-1 BAF controls under matched task information and training budgets.
- Report out-of-sample errors, false activations, interference, overwritten bits, selected-page sparsity, total model bytes (including priority, monomial masks, metadata and alignment), end-to-end latency, RSS and reliable energy evidence.
- Introduce any learned page payload or writable priority policy only in a separately qualified stage.
- Publish benchmark evidence via RemoteOps; SciRust/FLAT helpers require versioned contracts and independent semantic qualification before internalizing performance candidates.

## Reproduction

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo test --locked -p sml-core boolean_associative_joint
```

Merge only if all exact-head required checks are green. SML-25B claims and inference-default changes remain blocked pending independent evidence.
