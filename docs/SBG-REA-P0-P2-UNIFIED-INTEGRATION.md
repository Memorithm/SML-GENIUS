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

A 24-pair native CPU timing harness has now been implemented as
`rae_p1_paired_native_cpu_cost_profile`, explicitly **ignored** in
ordinary CI. It executes the *same* actual coupled MOR → ANF expert →
Delta-KV model with and without the **observer plus exact MOR route
reuse**, alternates order, checks final state/route/memory parity for
each pair, and emits per-case wall-clock nanoseconds, full-scan/dirty
word/reconciliation counts **and actual MOR calls elided**. This
measures the combined observation+cache path; it does not isolate
cache benefit or establish an overall speedup without analysis.

The harness rejects missing `SML_RAE_SOURCE_SHA` (40 hex characters)
or `SML_RAE_RUNNER` identifiers. A real source SHA must be verified
from the exact checkout, not merely typed. The sole authorized physical
control plane is Memorithm/RemoteOps, and it must admit an isolated
runner before launching the harness:

```bash
SML_RAE_SOURCE_SHA="$(git rev-parse HEAD)" \\
SML_RAE_RUNNER="<authorized RemoteOps runner identifier>" \\
cargo test --locked --release -p sml-core \\
  rae_p1_paired_native_cpu_cost_profile -- --ignored --nocapture
```

Physical comparisons, including provenance, paired repetitions,
runner workload isolation, case IDs and resource counters, must execute
on admitted machines under the sole Memorithm/RemoteOps control plane.
Until run logs are observed and archived, physical latency/RSS/cache/
DRAM/power savings are **not measured**. Ordinary GitHub CI does
not substitute for this evidence.

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

## P2b — active, exact-native computational elimination (not learned skipping)

**A real computation-avoidance path is now opt-in and operational in
the SML routing loop.** `enable_rae_exact_mor_reuse()` activates a
single-entry cache for the entire `SemanticMorRoute` decision on a
fresh `RoutedSemantic`, including when wrapped by
`RoutedExpertSemantic` or the full `CoupledSbgLoop`.

The identity key is the **complete current SBG R bitfield** and the
**complete previous-route/hysteresis state**, both checked by exact
equality. The `SemanticMorBridge`, registry, routing engine and runtime
hints are owned by the instance and immutable through its public API.
The current native bridge derives all router and expert affinities
exclusively from R, while prior-route hysteresis is an explicit input.
On a cache hit the original `bridge.route()` operation is **not
executed**: both the bridge's repeated signature comparisons and MOR
ranking are avoided. Its previous result is returned unchanged.
Every ANF expert, recurrent transition and Delta-KV memory cycle still
executes exactly as in the native baseline.

An R mutation, change in the previous-route input, a first cycle or a
new kernel instance triggers the authoritative native MOR calculation.
A failed route/recurrent/observation step cannot commit speculative
cache state. Counters `mor_evaluations` and `exact_mor_reuses` distinguish
actual calls from avoided recomputations.

This exact reuse is **different from speculative learned REA page
skipping**. It makes no decision based on missing front events,
untrusted learned confidence or numerical FLAT softmax bounds. This
design has an algorithmic test asserting 4 native MOR calls avoided in
6 identical-routing cycles after hysteresis settles, versus 6 native
MOR calls for the unoptimized baseline. A changed R bit invalidates
the cache. A full coupled model comparison verifies preservation of
the model state, selected expert pages and Delta-KV contents.

**The public default is still disabled.** A caller must explicitly
enable `enable_rae_exact_mor_reuse()` before its first cycle. A
separate opt-in Shadow learned candidate can coexist, but does not
skip required native pages. This is an implemented active computational
short circuit, **not** evidence of total speedup or a license to skip
MOR decisions when the state/previous-route key differs.

## Required verification

```bash
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked -p sml-core semantic::rae_runtime_capture
cargo test --locked -p sml-core semantic::rae_qualification
cargo test --locked -p sml-core exact_native_mor_reuse_removes_real_route_evaluations_and_preserves_outputs
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

Until those blockers are closed, do not advertise **learned MOR
bypass** or industrial speedup. Exact-key MOR memoization is a
separate, implemented calculation avoidance mode.
