# RTRL-2: continuous recurrent eligibility and norm-stabilized online updates

## Motivation and previously measured evidence

On native Dell [four-arm run #39029821597](https://github.com/Memorithm/RemoteOps/actions/runs/39029821597), RTRL with a **16-predicted-byte reset** reached 3.622322114 heldout bits/byte, ahead of HYB-1 3.762424049 and both BPTT/HYB-2 ~7.2818. This is a small same-domain SEC study with unmatched optimizer-step counts, **not** independent model convergence or language capability.

RTRL-2 isolates two potential improvements without modifying the MOR, ANF feature algebra, fixed hashed page address mapping, sparse f32 expert gradients, or base optimizer rates:

1. **Longer temporal credit:** carry online eligibility `(h, ∂h/∂θ)` through the entire consumed document prefix rather than resetting every 16 predicted bytes. Baseline `train_document_rtrl_windowed(text,16)` vs continuous `train_document(text)`.
2. **Bounded online recurrent updates:** clip the global L2 norm of the three-dimensional instantaneous recurrent gradient `g_t = (Σ_bit dL_bit/dh_t) × ∂h_t/∂θ` to a fixed threshold before applying `θ ← clamp(θ - 0.005 × clip(g_t), -2,2)`. **Only** recurrent gradients are clipped; expert and gate updates remain exactly the original online RTRL. Test both independently pre-registered clip thresholds 0.125 and 1.0, including count of actual clipped positions and mean/maximum preclip norm. No dense 50M Jacobian, no Adam, and no altered routing.
3. A very large threshold such as `1e30` must produce **bit-identical learned recurrent head parameters and equal heldout predictions** to the legacy continuous online RTRL.

## Preregistered Dell arms

| Arm | CLI flag | Recurrent horizon | Recurrent gradient clip |
|---|---|---|---|
| Baseline | `--pilot-rtrl2-reset16` | 16 predicted positions (resets) | None |
| Continuity ablation | `--pilot-rtrl2-continuous` | Whole consumed SEC source prefix | None |
| Stable-small | `--pilot-rtrl2-clip0125` | Whole prefix | L2 ≤0.125 |
| Stable-large | `--pilot-rtrl2-clip1` | Whole prefix | L2 ≤1.0 |

Each of the four independent models starts from identical f32 master initialization with **50,000,896 addressable slots** (48,829 pages of 1024) and maximum 32 resident pages. Exact SHA-pinned SEC 10GiB source `c890b0cd08e9d67d4ea4ca9a88e760282c6de6c1a8f74ebfad6c0d1a2c740ed9`, actual 64 KiB TRAIN source report prefixes (64 distinct docs) and disjoint 32 KiB exploratory holdout (32 other source reports). All four consume identical SHA-verified document/prefix selections; compare ordered `BOUNDARY_V3` manifests bit-for-bit. The heldout evaluator is **uninterrupted-document and read-only** for all models. Capture expert parameter changes, gate and recurrent update counts, measured wall time, heldout bits/byte, and bounded clip diagnostics.

**Scientific gate:** This 32KiB SEC holdout was already viewed in previous experiments. Report outcomes for exploratory diagnostics only. Do **not** select 0.125 vs 1.0 based on this holdout then claim independent benefit. Before promotion, tune on TRAIN-only internal splits and evaluate preregistered improvement on a *new*, unseen, sealed and ideally out-of-time corpus with repeat seeds and uncertainty bounds.

## Limitations and safety

RTRL forward sensitivities are exact for fixed recurrent weights and an individual finite state recurrence, but **online parameter updates alter the weights between timesteps**, so eligibility across such updates is approximate and not differentiation through the full optimizer trajectory. The 50M atlas stores *addressable* f32 master slots; sparse files may not have all pages physically allocated, and a small pilot never trains all weights. Norm clipping affects only 3 scalar recurrent coefficients, not a general 25B-dimensional recurrent Jacobian. Corpus rights G0 = `NOT_ADMITTED`, 2024 TEST = `UNOPENED`. No production or conversational model claims.
