# Native ANF / SBG / MOR / Delta-KV integration

Date: 2026-10-10. Scope: synthetic functional integration, not model qualification.

## Baseline and execution path

PR #461 introduced `unified_anf_sbg_loop` in
`crates/sml-core/src/anf_sbg_unified.rs`. Its merge commit is
`f359766a8df9626a411627cbca8c298be6380afa`.

The executable `sml-unified-loop-smoke` calls that constructor, then the existing
`run_semantic_recurrent` driver. It does not reimplement the production cycle.
The requested minimum and maximum depths are equal, so a successful smoke run
must commit the requested number of cycles rather than stop after one stable
cycle. Every final memory output is fed into the following cycle.

The production order is:

`LargeBankRecurrent -> RoutedSemantic/SBG/MOR -> selected PVP6 expert pages -> route-conditioned Delta-KV -> next state`.

The executable is a diagnostic entry point in sml-core, not the future
SML-HARNESS, not a serving endpoint, and not a training implementation. No runtime
dependency on SciRust, FLAT-ATTENTION, NNIS or a vendor GPU SDK is introduced.
RemoteOps remains responsible for host execution and resource admission.

## Reproduce

Run from an authorized checkout containing this change:

```bash
git rev-parse HEAD
cargo test --locked -p sml-core --bin sml-unified-loop-smoke --test unified_native_cli
cargo run --locked -p sml-core --bin sml-unified-loop-smoke -- 8 165 255 toggle
```

Arguments are cycles (1..64), initial value (0..255), initial routing (0..255),
and ANF program (`identity` or `toggle`). Defaults are `4 0 0 toggle`.
The toggle program complements bit 7; the identity program preserves every
value bit. Both programs execute the real large-bank recurrent adapter.
Malformed inputs exit with code 2 and do not emit successful trajectory records.

The output is newline-delimited JSON with schema `sml.unified-loop-smoke/v1`:
a header, one record per committed cycle, and a summary. Cycle records include
the input value, actual ANF packed output, selected page IDs, executed page
count, expert gate/query work, post-expert value, actual Delta-KV key and read
work, and final value. The summary includes the independent stage trace counts.
There are no timing-derived performance fields.

For deterministic comparison, capture two invocations with the same arguments
and exact executable revision and compare their standard output byte-for-byte.
The integration tests perform that comparison using the actual Cargo-built
executable, not a mock.

## Test scope

The binary tests cover input bounds; twelve combinations of initial value,
routing and ANF program compared against explicit stage-by-stage execution;
a literal independent ANF oracle at every cycle; late memory-budget rollback
and rejected depth reuse after a committed cycle; and full-state/report replay
at the maximum supported diagnostic depth.

The CLI integration tests execute the native binary for eight cycles, compare
two independent invocations, check all stage counts, demonstrate a changed ANF
first-stage output under the identity/toggle control, and require invalid
bounds to produce no success evidence. These tests run under the existing
`cargo test --locked --workspace` CI command without changing the CI trust model.
Compilation, linting and tests must pass on the exact PR head before merge.

## What this does not establish

The diagnostic has eight semantic value bits and eight large-ANF gates declared
on a 20-bit address domain (`K = 1,048,576`). Its input values use only eight
bits: this run does not exhaust that domain, instantiate a million expert pages,
or establish million-parameter model capability. The native large-bank adapter
currently accepts semantic widths only from 1 through 31 bits.

MOR selects from two fixed PVP6 expert pages. Those page bodies have not been
replaced by large-ANF banks. The ANF control changes value rewriting; the smoke
study does not establish learned routing or task utility. The memory payload,
router/expert signatures and provenance IDs are deliberately synthetic.

No training, language-model quality, attention-replacement superiority,
throughput, GPU speedup, 25B-scale execution or disk checkpoint/resume claim is
made. Logical budget accounting is not a physical RAM/VRAM bound; transactional
clones and retained evidence still need separate resource qualification.

## Next integration gates

1. Add large-ANF selected expert page bodies with explicit SBG input/output
   projection, inactive-page exclusion, independent oracles and preflight work
   budgets. Preserve the existing PVP6 control rather than silently relabel it.
2. Qualify complete-state continuation/checkpoint ownership across ANF, MOR,
   expert revisions and Delta-KV; ANF history alone is not a complete checkpoint.
3. Exercise a model-aligned data path and preregister controls before making
   training, language utility or learned-routing claims.
4. Measure the complete loop on an admitted host through RemoteOps, separating
   CPU execution from optional qualified SIMD/GPU paths and retaining exact
   source, input, output and device evidence.

The trace schema and stage-by-stage reference strategy are candidates for reuse
in SciRust, FLAT-ATTENTION and NNIS qualification. They are not evidence that
those repositories already execute this SML loop; destination requalification
is required before any cross-repository promotion.
