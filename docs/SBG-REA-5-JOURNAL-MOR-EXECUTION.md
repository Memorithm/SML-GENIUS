# SBG-REA-5 — executable native dirty-word → MOR → learned replay

Status: runnable research comparison on top of
`SbgRoutingDirtyJournal`, `SbgRaeNativeTrace` and
`RaeTrainedEventRouter`. No default model changes or inference bypass.

## Actual software path exercised

`crates/sml-core/examples/sbg_rae_dirty_mor.rs` instantiates one native
SBG 4096-bit Boolean state, the **real** `SemanticMorBridge`,
`MorRouterRegistry` and `MixtureOfRouters`, and a model-owned R-field
journal. It performs this pipeline on eight deterministic cycles:

```text
single logical R-bit set
      ↓
SML model-owned packed dirty-word journal (sorted index → new u64)
      ↓
independent R replay of changed words + full-state parity check
      ↓
real SBG→MOR expert page selection on EVERY cycle
      ↓
native trace TRAIN frames 1..6 / HOLDOUT frames 7..8
      ↓
frozen event→expert map, rising-only and persistent policies
      ↓
per-frame/multi-frame native MOR page omissions
```

The final two frames intentionally keep bit 0 active and show that
rising-only candidates can miss a native MOR-required page on the held
frame. Persistent-active replay recovers it **for this fixture**, but
this is not a guarantee on other contexts or model snapshots.

A single coordinate is changed in a **64-word** SBG R field. The
producer therefore emits one original packed word on a changed tick,
and no packed words on a held tick. The example prints exact dirty
notifications and actual native MOR page decisions for all cycles.

### Engineering and scientific qualification boundaries

The fixture deliberately invokes full authoritative reconciliation
and clones a full SBG state for MOR on each cycle. This establishes
correctness of the notification pipeline, not reduced end-to-end
memory traffic or native MOR compute. The reference MOR still runs
eight times. Full dense SBG signature comparisons and native model
decisions remain unmodified.

This is a synthetic one-feature, two-expert, two-router **structure**.
It is not measured language-model quality, held-out natural text,
physical DRAM bandwidth, lower-latency decode, or evidence of novel
scientific architecture.

One-to-one co-occurrence on this example does not imply that
learned front→page links cover real recurrent SML dependencies. The
producer's scope covers only model-native R mutations **through the
journal interface**, not external V/M/A, memory generations or
rewritten expert/registry policy.

The next implementation step must anchor dirty production to the
actual SML state mutation surface and export independently identified
real TRAIN/holdout traces with source-level provenance. Only then
measure combined producer + consumer + reconciliation cost via
Memorithm/RemoteOps on a pinned compute target.

## Reproduction

```bash
cargo run --locked -p sml-core --example sbg_rae_dirty_mor
cargo test --locked -p sml-core --example sbg_rae_dirty_mor
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
```

The executable is registered with `test = true`, so its actual
pipeline runs as a test in all-targets qualification, rather than
being represented only by an unexecuted snippet.

Related: `docs/SBG-REA-1-RISING-FRONT-MOR.md`,
`docs/SBG-REA-2-NATIVE-TRACE.md`,
`docs/SBG-REA-3-LEARNED-EDGE-ROUTING.md`,
`docs/SBG-REA-4-NATIVE-DIRTY-WORDS.md`, and the FLAT
`docs/research/RAE_5_DIRTY_WORD_ROUTING.md`.
