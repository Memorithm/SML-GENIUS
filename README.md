# SML-GENIUS

SML-GENIUS is Memorithm's research repository for **State-Memory Lattice (SML)**: an experimental, predominantly Boolean / low-bit autoregressive architecture intended to study whether large learned capacity can be separated from per-token active compute.

The long-range target is a 25-billion-parameter model with a storage-backed circuit atlas, bounded recurrent state, bounded associative memory and adaptive **recurrent depth**. The target inference envelope is **8 GiB RAM**, **2 GiB VRAM** and **at least 40 decoded tokens/s** on a future frozen reference machine.

These are engineering targets. No trained 25B SML model, throughput result, quality result or architectural-novelty claim exists yet.

## Non-attention candidate contract

The SML candidate core is designed without Q/K/V attention, QK^T, softmax token scoring, dense token-by-token score matrices, all-history Hamming/POPCOUNT ranking, or a Transformer-semantic KV cache. External attention models remain valid baselines.

## Core research hypothesis

Instead of traversing all learned parameters for every token, SML aims to keep most capacity cold and activate a small directly routed subset during each internal reasoning cycle:

    persistent token state + associative memory
                  |
                  v
          Boolean/algebraic routing
                  |
                  v
          selected atlas pages
                  |
                  v
           recurrent workspace
                  |
            repeat 1..R cycles
                  |
                  v
                COMMIT

Recurrent depth is separate from token recurrence: a single emitted token may use multiple internal reasoning cycles while reusing the same physical parameter atlas.

## Initial SML-25B representation hypothesis

| Physical class | Learned parameters | Packed bits/parameter |
|---|---:|---:|
| Boolean | 21.25B (85%) | 1 |
| Ternary | 2.50B (10%) | 2 |
| Residual low-bit | 1.25B (5%) | 4 |
| **Total** | **25B** | **31.25B bits total** |

The raw packed weights would therefore require exactly **3,906,250,000 bytes** before metadata and alignment. The split is an experimental starting hypothesis, not a frozen final architecture.

## Workspace

- **sml-core**: dependency-free deterministic reference semantics, exact counters, Boolean circuit oracle, direct routing and adaptive recurrent-depth runner.
- **sml-cli**: small executable contract probe.
- **docs/SML-0-SPEC.md**: executable SML-0 architecture boundary.
- **docs/ROADMAP.md**: evidence-gated progression from reference primitives to 25B.
- **docs/PRIOR_ART.md**: initial prior-art anchors; explicitly incomplete.

## Build

    cargo fmt --all -- --check
    cargo check --workspace --all-targets
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    cargo run -p sml-cli -- contract

## Scientific discipline

A generated connection is not counted as millions of learned parameters merely because it expands to many edges. Total learned parameters, packed physical bits, active parameters per recurrent cycle, total parameter touches per emitted token, memory traffic, recurrent depth and latency are reported separately.

SML advances through gates. Scaling from 50M to 300M to 1.5B to 7B to 25B is permitted only when the previous stage provides reproducible quality and systems evidence.

## License

PolyForm Noncommercial 1.0.0, aligned with SciRust. Commercial licensing requires a separate agreement. See LICENSE.md.
