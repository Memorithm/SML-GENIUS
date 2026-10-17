# RTLR-ELASTIC — demand-allocated exact-local RTRL for paged MOR/ANF (v1)

## Motivation from native evidence

The same-physical-atlas RTRL-3 test [Dell #39042340643](https://github.com/Memorithm/RemoteOps/actions/runs/39042340643) preserved **50,000,896 addressable f32 expert slots** and the identical sparse experts and MOR gate. RTRL-2 continuous delivered **3.616609952 DEV bits/octet**; RTRL-3 stochastic UORO at H=8 delivered 3.652215017, and H=16 delivered 3.658475041 — both worse. The H=1 bridge exactly reproduced RTRL-2.

The research alternative, named **RTLR-ELASTIC** by project convention, deliberately reduces stochastic variance rather than assuming wider recurrent states perform better. It does **not** claim to be standard exact RTRL for arbitrary densely coupled networks.

## Mechanism: anchor + demand-allocated local traces

At the beginning, allocate just one scalar recurrent cell `j=0`, identical to the original continuous RTRL-2 recurrence and optimizer. At any later training document boundary, permit the controller to allocate another **independent** recurrent cell, up to `max_active`. No global H×3H Jacobian and no UORO random-sign estimator.

Each active cell carries only a 3-dimensional exact local eligibility:
```text
x_t      = (popcount(b_t)-4)/4
phi_t    = (-1)^(1+popcount(a_t AND b_t))
h_{j,t}  = tanh( a_j h_{j,t-1} + b_j x_t + c_j phi_t )
J_{j,t}  = (1-h_{j,t}^2) *
           ([h_{j,t-1}, x_t, phi_t] + a_j J_{j,t-1})
```
For fixed local recurrent weights, these are **exact** sensitivities because the cells are uncoupled. With changing online weights they are online forward eligibilities, not gradients through the full optimizer history.

When there is one active cell `m=1`, the feature delivered to the *existing* MOR gate is `h_0`, and the updates use the original left-associative RTRL-2 expression. For `m>1`, use a trainable but bounded scalar residual mixture:
```text
extra_mean = mean(h_1, ..., h_(m-1))
feature    = h_0 + g * (extra_mean - h_0)
g in [0,0.5], initialized 0.125
```
This avoids diluting the known-working scalar anchor by naively averaging a new untrained wider state. The original MOR experts and hashed gate are updated per target bit **unchanged**. Sum their original recurrent credit across the 8 predicted target bits as `c_t = dL/dfeature`.

Local recurrent gradients and the additional readout gate gradient are:
```text
dL/dtheta_0 = c_t (1-g) J_0
dL/dtheta_j = c_t (g/(m-1)) J_j   for j>0
dL/dg       = c_t (extra_mean-h_0)
```
Then apply recurrent online SGD with exactly the inherited 0.005 rate and clamp recurrent weights to [-2,2]; gate to [0,0.5]. When `m=1`, its exact RTRL-2 gradient is `c_t J_0`, no additional gate trainable or applied.

## Actual elastic scheduling, no DEV-dependent branching

Once per completed TRAIN document, the controller observes only the **mean TRAIN loss bits/byte**. The current experimental preset is:
- Start `m=1`; cap `m<=1/8/16` in matched ablation arms.
- Warm up 2 completed documents before any topology decisions.
- Grow one cell after **two consecutive** completed TRAIN documents with loss >4.1 bits/byte, if below max; reset the counter.
- Prune one newest cell after **two consecutive** completed TRAIN documents with loss <3.8 bits/byte, if above 1; reset the counter.
- Keep width unchanged between those thresholds; reset both patience counters. No topology changes inside a document.
- Newly allocated cells are deterministically seeded with different local recurrent coefficients to break the symmetry that hurt the RTRL-3 fully equal initialization. Pruned cell weights are discarded; this **is destructive**, not a retention or resurrection bank.

The thresholds are **exploratory a priori controls**, not independently tuned. They are neither guaranteed to activate on SEC nor supported by a fresh sealed heldout; report actual growth, shrink, peak and active cells so an inactive-elastic run cannot be falsely reported as elastic learning.

## Memory and compute

Actual recurrent state/eligibility payload is `7m` f64 values for 3 weights, one hidden state and three local sensitivities per active cell, plus one f64 readout gate, i.e. `8*(7m+1)` bytes. The Vec allocator may reserve additional capacity; this count does not measure RSS or the separate paged expert cache. Per-step recurrent compute scales O(m), without either a dense O(m²) Jacobian or an O(Km) UORO factor bank. All code Rust-only; no NVIDIA software.

## Native four-arm physical 50M validation

Reuse exact SHA-verified SEC source and **identical sampled prefixes** from the previous 50M pilot: 64,512? No: **64 documents ×1,024 bytes =65,536 TRAIN bytes (65,408 predicted positions)** and 32 ×1,024 bytes =32,768 read-only DEV bytes (32,704 positions). Each independent atlas has 48,829 pages×1024 =50,000,896 f32 addressable expert slots, up to 32 cached pages.

Compare:
- `--pilot-rtrl2-continuous` = native scalar RTRL-2.
- `--pilot-rtlr-elastic1` = exact one-cell scalar RTLR-ELASTIC control.
- `--pilot-rtlr-elastic8` = dynamic capacity 1..8.
- `--pilot-rtlr-elastic16` = dynamic capacity 1..16.

All source selection SHA manifest files must match bit-for-bit. Holdout evaluation must not alter weights, topology, gradient counters or training scheduler; H1 must exactly reproduce RTRL-2 logged train/DEV losses and expert/router/recurrent counters. The new recurrent local weights and topology are written as an **audit sidecar** `rtlr_elastic_weights.tsv`, with separate `rtlr_elastic_stats.tsv`. The existing paged checkpoint **cannot fully reopen RTLR-ELASTIC**, because this branch has no verified integrated versioned model reloader. Do not deploy, resume training or claim full checkpoint capability from the sidecar.

## Strict interpretation and forward plan

The prior 32KiB SEC DEV has been seen repeatedly. This four-arm test is exploratory engineering only; never use it to select thresholds/widths and then claim an unseen generalization gain. Validate on **fresh sealed out-of-sample data**, multiple training seeds, independent TRAIN-only hyperparameter tuning and explicit model compute/memory budgets. Track true dynamic activation and loss change versus the scalar anchor. G0 SEC rights remain **NOT_ADMITTED**; 2024 TEST remains **UNOPENED**. There is no 25B-trained or production language-model claim.

See follow-up [SML issue #603](https://github.com/Memorithm/SML-GENIUS/issues/603) for UORO variance and optimizer attribution.
