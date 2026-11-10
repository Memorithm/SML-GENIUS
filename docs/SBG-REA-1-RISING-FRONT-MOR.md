# SBG-REA-1 — native rising-edge observation beside MOR

Status: research-only; native model-owned SBG/R-to-MOR observation.
No default decoder, attention, learned control, page selection, or recurrent
halt behavior is changed.

## Hypothesis

Observe rising-front transitions in the existing SBG routing field R,
rather than inventing Transformer Q/K/V vectors:

```text
rise[t] = R[t] AND NOT R[t-1]
fall[t] = R[t-1] AND NOT R[t]
```

The existing `SemanticMorBridge::route` remains authoritative and is
executed at every requested cycle *before* the observer's state advances.
This is a necessary control: an unchanged R field does not prove that
recurrent state, model tasks, data dependencies, persistent memory or
MOR runtime hints are unchanged. Zero rising fronts **never skip**
native routing here.

The model-owned `semantic::rising_edge::SbgRisingMorProbe` keeps:
- previous packed R state;
- current rising/falling bitplane masks;
- strictly increasing model-owned cycle IDs;
- rising, falling and active-bit counters;
- the independently computed native `SemanticMorRoute`.

A bad cycle or native MOR error leaves the observer unchanged. The hot
front arithmetic is packed Boolean word operations and uses no allocations
in the observer after initialization, but native MOR route evaluation
can allocate and is **always** executed. The observer is not a
recurrent runtime optimization.

## Scientific controls and next functional milestone

The next model-owned stage must test event-triggered *candidate page
preparation* (not elimination of native reasoning) with paired conditions:
- always-route current SBG/MOR path;
- rising-only hints;
- rising+falling hints;
- persistent active-state hints;
- matched-density random hints;
- adversarial unchanged-R, changed-V/M/A, changed memory, or changed
  runtime hint examples.

Only then consider an explicit, versioned model-internal page activation
contract with bounded persistent state and complete work/memory counters.
If an event-predicted page set differs from native MOR's required set, no
work may be suppressed. No downstream model quality, speed or novelty
claim follows from a trace count alone.

The FLAT-ATTENTION RAE-1/RAE-2 experimental primitives are independent
cross-repository references, not runtime dependencies. SML remains a
Q/K/V-free architecture, as stated in `README.md` and `AGENTS.md`.
Physical hardware experiments, if later authorized, must route through
Memorithm/RemoteOps and retain exact SHA and device evidence.

Run:
```bash
cargo test -p sml-core semantic::rising_edge
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
```

This is a correctness/control research slice, not an SML-25B gate
completion or a claim that SML beats a trained external model.
