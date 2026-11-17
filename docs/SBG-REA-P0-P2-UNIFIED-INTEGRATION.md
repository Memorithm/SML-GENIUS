# SBG-REA: unified P0/P1/P2 experiment and fail-closed implementation

**Branch status:** opt-in research; one PR for P0 integration, P1
qualification surfaces and P2 safe fallback. The original SML-GENIUS
SBG→MOR→expert→Delta-KV model is still the sole execution authority.
No Transformer Q/K/V is imported into SML.

## P0-1 — integrate the previously off-main #933 example

`crates/sml-core/examples/sbg_rae_dirty_mor.rs` is now in the
main-target branch and registered with `test = true`, so the
synthetic 4,096-bit R mutation → dirty journal → real MOR →
TRAIN/HOLDOUT example must build and execute in the all-target CI.

This example is a deterministic micro-fixture, **not** realistic
natural-language training, physical memory measurement or a trained
full SML model. It demonstrates a held-high positive MOR decision
without a fresh positive front.

## P0-2 — actual runtime boundary, not a duplicate MOR engine

`RoutedSemantic::enable_rae_observation` attaches the optional native
`RaeNativeRuntimeCapture` before executing any cycles. The observer
records the *already calculated* `MorDecision.selected_experts` inside
`RoutedSemantic::rewrite`, after the original SBG→MOR engine has made
its decision. It never calls MOR twice or substitutes predicted pages.

The opt-in is exposed through `RoutedExpertSemantic` and
`CoupledSbgLoop`, so the actual selected ANF expert bodies and
Delta-KV memory keep executing normally. On an observation error, the
tentative rewritten kernel and route are not committed.

SML's existing model can rewrite all four SBG fields wholesale.
For that reason the safe compatibility path currently invokes
`SbgRoutingDirtyJournal::replace` for each whole R state: this
scans all packed words. The observer reports these scans, the distinct
dirty words, full reconciliation scans and native routes; **no end-to-end
zero-read or acceleration claim** is permitted. A later real write-time
ownership refactor must replace whole-state adaptation, without losing
update notifications or native runtime correctness.

A newly required MOR page may appear with zero R fronts, because
V/M/A, registry, recurrent state, memory or runtime hints change;
the native route always executes, including on empty dirty streams.

## P0-3 / P1 — reproducible, disjoint model evaluation

`semantic::rae_qualification::qualify_native_rae` trains only on an
explicitly named TRAIN trajectory and runs a frozen comparison on
separately identified HOLDOUT trajectories. It rejects empty or
overlapping case IDs, malformed frames and invalid geometry. Each
HOLDOUT case is validated independently: session resets do not
fabricate a continuous R stream.

The report includes selected, required, missed and extra original MOR
page counts for rising-only, persistent and context-recheck controls,
including frames with no new rising front. The native model and learned
candidate execute on the SAME trace. It reports exactly what was
measured; matching MOR labels is NOT model task quality.

A real coupled-runtime test uses the existing full
`CoupledSbgLoop` kernel in three independently identified runs:
one TRAIN and two HOLDOUT. The instrumented and baseline runs must
produce identical final SBG state, recurrent trace, expert/MOR pages,
memory results and logical work accounting. Shadow replay is checked
for all HOLDOUT frames; none can bypass MOR.

These tasks remain deterministic model *fixtures*, not held-out
natural-language/real-user workloads. Replace with independently
collected, provenance-controlled runs to close a real task-quality
gate. A pinned source string is an assertion, not a Git attestation.

## P1 — quantitative and mathematical quality/resource gates

The report has a declared page-miss and extra-work budget, plus
separate indicators for measured task quality, paired physical
savings and dependency-certification. Missing evidence blocks
promotion. The runtime recorder explicitly accounts for whole-state
word scans and reconciliation reads: a low dirty-word count alone
does not establish memory savings.

FLAT-ATTENTION RAE-6 has a distinct existing scalar QK norm/mass
oracle and incremental K-norm ledger. **This SML repository must
not treat native MOR page agreement as a numerical softmax bound.**
A QK model's norm metadata/causal counts require an independent
FLAT adapter and separate exact-head qualification; they are not
silently imported into this SML-only PR.

Physical comparisons of complete baseline vs observed runtime,
with training/holdout case IDs, exact source SHA and paired
repetitions, must execute on admitted machines under the sole
Memorithm/RemoteOps control plane. Until observed and archived,
physical latency/RSS/cache/DRAM/power savings are **not measured**.
This PR adds model-level counters and exact parity tests, not
fabricated hardware results.

## P2 — activation mode and rollback

Two public dispatch modes exist:

- `Disabled`: use native MOR, no candidate decision;
- `Shadow`: calculate a frozen candidate diagnostic, but return
  the **original MOR pages** as the executed result.

`dispatch_native_shadow` rejects unexpected code revision and
malformed native page truth. Disagreement is flagged as a fallback.
`native_mor_bypassed` is always false. The qualification report's
`production_skip_authorized` is always false, even if a caller
claims measured performance or quality: a complete fail-closed,
model-owned runtime certificate and fallback execution would need
separate demonstrated engineering evidence before any real skip
could be made available.

**This is a safe P2 shadow rollout foundation, not production
optimization enabled.**

## Required verification

```bash
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked -p sml-core semantic::rae_runtime_capture
cargo test --locked -p sml-core semantic::rae_qualification
cargo test --locked -p sml-core real_coupled_model_train_and_two_heldout_cases_are_shadow_only
cargo test --locked -p sml-core --example sbg_rae_dirty_mor
cargo test --locked --workspace
```

SML AGENTS.md and the off-main roadmap govern changes; exact
HEAD CI and external Trusted RSI controls are mandatory before merge.

## Residual acceptance blockers

1. Full ownership and notification of every mutation in the
   production SBG `R` field instead of this conservative full-snapshot
   compatibility adapter.
2. Provenance-verified task-level TRAIN/HOLDOUT from an actually
   trained SML model, plus multi-seed model-quality comparisons.
3. Pinned RemoteOps paired physical cost evidence and accepted gates.
4. Formal runtime dependency/causal safety and, for FLAT, correct
   norm/mass metadata + numerical rounding proof.
5. Model-owned promotion/revocation controls that can fail closed
   under live K/V, state, version, registry or memory changes.

Until those blockers are closed, do not advertise native MOR bypass
or industrial speedup.
