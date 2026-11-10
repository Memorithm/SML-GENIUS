# SBG-REA-4 — native dirty-word source for rising-front routing

Status: **opt-in research implementation** within `sml-core`.
Not a replacement for native SBG/MOR, memory or recurrent inference.
The existing `SemanticState` representation is unchanged.

## Why this matters

SML's native SBG routing field `R` is a packed Boolean bitfield.
The existing rising-front observer evaluates every packed word on each
sampled cycle. Counting sparse events alone does not remove that scan.

`semantic::rae_dirty_journal::SbgRoutingDirtyJournal<BITS>` wraps an
owned `SemanticBitField<BITS>` with an ordered journal of the words
changed **at mutation time**:

- `set(index, value)` reads and writes one native packed word. When a
  change occurs, it remembers the word's value at the last acknowledged
  tick and never scans unrelated feature words.
- Multiple changes to the same word collapse into a single pending word.
  If operations undo the previous value before acknowledgement, the
  pending entry disappears.
- `flush(tick, sink)` delivers unique, ascending original word indices
  and their current packed values. On sink failure it leaves the journal
  and logical clock unchanged for a retry. On successful acknowledgement,
  pending entries are cleared.
- `replace(next)` supports whole-field mutations, but explicitly scans
  all words once; there is no sparse-read claim for this path.
- When no word changed, a successful flush does not scan the R bitfield;
  the sorted pending set is empty.

This is a concrete **model-owned upstream producer** whose neutral
`SbgDirtyRoutingWord { index, next }` shape can be mapped to the
independent FLAT RAE-5 dirty-word consumer without imposing a required
FLAT runtime dependency on SML.

## Failure and integration boundaries

The wrapper tracks only mutations performed through its own `set` and
`replace` methods. Other SBG state updates, external memory updates,
MOR policy/registry changes, implicit decoder steps or direct mutations
to unrelated `SemanticState.routing` fields are **not automatically**
tracked. The caller must choose and preserve an owning mutation
boundary before relying on this producer.

In particular, the module does NOT permit skipping native MOR when no
front appears. Events are positive scheduling signals, not exhaustive
relevance or page-residency certificates. Any event-informed scheduling
must preserve the fully qualified model execution until an independent,
versioned, fail-closed route-admission and actual downstream quality
contract is established.

The journal uses `BTreeMap` metadata proportional to the number of
dirty words. Each `flush` currently allocates a temporary sorted delta
vector; zero allocation, CPU cache savings, DRAM bandwidth reduction,
latency and energy improvements are **not yet measured**. The `replace`
operation performs a full scan. When `R` is derived by rebuilding the
full bitfield each cycle, total savings may be absent.

## Validation

- Multiple scalar writes to same packed word coalesce.
- 64-bit word boundary and 130-bit tail padding are preserved.
- Write reversals before flush do not create spurious events.
- Failed sinks retain pending notifications and cycle state.
- Invalid coordinates fail without mutation.
- Whole-field replacement correctly emits complete word updates.
- Full snapshot reconciliation in FLAT RAE-5 is the downstream
  integrity check against omitted dirty words.

```bash
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked -p sml-core semantic::rae_dirty_journal
cargo test --locked --workspace
```

The next qualification step is real SML model R-field mutation
instrumentation: compare the tracked producer against full-snapshot
mutation traces on pinned TRAIN/holdout tasks, including V/M/A changes,
MOR registry/hint changes and memory invalidation, then measure
the **combined** producer+consumer cost with RemoteOps on supported
CPU targets. Do not infer end-to-end model superiority from low
event counts.

Related: `docs/SBG-REA-1-RISING-FRONT-MOR.md`,
`docs/SBG-REA-2-NATIVE-TRACE.md`,
`docs/SBG-REA-3-LEARNED-EDGE-ROUTING.md`; FLAT
`docs/research/RAE_5_DIRTY_WORD_ROUTING.md`.
