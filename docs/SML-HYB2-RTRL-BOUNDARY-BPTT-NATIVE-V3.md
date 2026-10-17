# HYB-2 — RTRL boundary eligibility and reverse BPTT adjoints (research)

This is a **true chain-rule hybrid**, distinct from HYB-1's 3R/1B optimizer schedule. Source remains Rust-only, sparse and physically disk-paged. **No 25B training or conversational readiness is claimed.**

## Mechanism

At each block entry, carry the scalar recurrent state `h_entry` and its three-parameter RTRL forward sensitivity `J_entry = ∂h_entry/∂θ`. A block of at most 512 predicted-byte positions is evaluated at frozen weights, storing only block activations and two expert predictions per bit. During reverse mode, ordinary BPTT computes `g_local`, the block's recurrent parameter gradient assuming the entry state is frozen, and its boundary adjoint `λ_entry = ∂L_block/∂h_entry`.

The exact chain rule for a **single frozen-weight sequence** is:

`g_block = g_local + λ_entry * J_entry`

Both derivatives use the same parameter snapshot. The `J_entry` term must be applied once, **not once per timestep**. `Trace::advance` propagates state and forward sensitivities to the next block; neither is reset at the boundary. Sparse expert and MOR gate gradients are applied to the actual physical atlas exactly as in the paged BPTT baseline. The model is still a fixed-ANF-topology, fixed-hashed-route scientific proxy.

### Important qualification

The native training method performs a gradient update **after every block**. The carried `J_entry` then reflects past sensitivity under previous weights and becomes an **online approximation** for the evolving-parameter trajectory. Mathematical gradient equivalence must not be claimed across updates. Unit tests **separately** freeze recurrent weights and verify blockwise RTRL-boundary + BPTT reverse credit equals full-sequence reverse BPTT, forward RTRL and central finite differences across >3 blocks. A negative control intentionally omits the boundary credit and must fail the equivalence. Additional tests assert real paged expert touches, unique target accounting and read-only evaluation.

## Equal-source native four-arm experimental protocol

Pre-registered CLI modes:
- `--pilot-boundary-rtrl`: established online RTRL, reset every 16 predicted-byte positions.
- `--pilot-boundary-bptt`: established BPTT, reset every 16 predicted-byte positions.
- `--pilot-boundary-hyb1`: established alternating 3R/1B windows, reset every 16 positions.
- `--pilot-boundary-hyb2`: RTRL boundary sensitivity + BPTT reverse adjoints, **state continuous** across windows.

All four arms use 48,829 physical pages × 1024 f32 slots = **50,000,896 allocated weights**, no more than 32 resident pages, equal sparse routing and hyperparameters, and **independent initially identical** disk snapshots. They verify the full immutable SEC corpus SHA `c890b0cd08e9d67d4ea4ca9a88e760282c6de6c1a8f74ebfad6c0d1a2c740ed9`, each consumed SEC document's provenance and hash, and train on identical first 64KiB of verified prefixes (64 docs). Separate 32KiB DEV (32 distinct docs) uses the **same uninterrupted, read-only recurrent document evaluator** in *all four* arms. HYB-2 has a longer training-state horizon by design: the comparison tests its effect, not equality of train-time effective credit horizon. Record the train/DEV fingerprint manifest `BOUNDARY_V3` and require byte-identical equality across the four models.

The four-branch comparison has a **new heldout protocol**. Its DEV scores should **not** be directly compared to the earlier [128-byte-reset experiment](https://github.com/Memorithm/RemoteOps/actions/runs/39028781561), which used a different readout horizon and sample count. Measure actual touched parameters, update counts, wall time, SHA checksums and finite losses rather than claiming capacity means trained weights.

The current BPTT/HYB-2 gradient is normalized per block while online RTRL updates more frequently; learning-rate/update-count confounding remains. Keep the initial 16-byte block length fixed; do not tune this ratio on DEV. No hyperparameter selection on this heldout.

**Rights: NOT_ADMITTED. 2024 TEST: UNOPENED.** This is not a cleared financial/production model.

Next step is controlled update-frequency and gradient-scale ablation [issue #593](https://github.com/Memorithm/SML-GENIUS/issues/593). HYB-2 mathematical scope is [issue #595](https://github.com/Memorithm/SML-GENIUS/issues/595).
