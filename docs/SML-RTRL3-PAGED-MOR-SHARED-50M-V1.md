# RTRL-3 with the same physical MOR/ANF paged experts — 50M pilot v1

## Why this integration

Earlier RTRL-3 standalone native Dell [run #39033899578](https://github.com/Memorithm/RemoteOps/actions/runs/39033899578) measured a **different synthetic 8-bit logistic head** and thus cannot be compared directly to RTRL-2 physical paged MOR/ANF. This patch connects **the existing H/K/M UORO online eligibility** to the **same** learned MOR gate and disk-paged expert updates, without changing expert page addressing, initial router coefficients, learning rates or prediction loss.

At each supervised byte position:
1. Advance recurrent H-dimensional RTRL-3 state and its UORO rank-K forward eligibility from the two causal byte inputs; compute mean hidden state `mean(h)`.
2. For all eight target bits, reuse `MorAnfRecurrent::train_target`, which updates two real physical sparse expert pages and the existing hashed MOR gate, using `mean(h)` as the same recurrence-dependent gate feature `feature[2]`.
3. Sum the existing gate derivative `Σ_bit (dL/d gate_logit) × gate_weight[2]` as recurrent credit, computed *before* each respective gate update, and apply it to the 3H recurrent local parameters via exact H=1 sensitivity or UORO rank-K estimate for H>=8.
4. Preserve state and eligibility across all positions of a document; reset for each document. Holdout is read-only: clone/reset recurrent state, evolve only its frozen hidden state, reuse the exact original `prediction` against the identical physical expert pages and MOR gate. No fake logits or extra supervised pass.

## Exact H=1 control

`ElasticRtrl3::for_mor` initializes local recurrent coefficients `[0.65,0.3,0.2]` to match RTRL-2 exactly. H=1, mixing rank M=0, eligibility rank K=1 and clip threshold 1e30 recovers **the scalar RTRL-2 recurrent mathematical path and left-associative SGD** with the same paged updates and readout. A deterministic unit test asserts exact loss, counters and heldout equality between independently initialized models; this is the minimum correctness gate.

## Pre-registered native 50M four-arm comparison

Same validated SEC original 10GiB SHA-256 `c890b0cd08e9d67d4ea4ca9a88e760282c6de6c1a8f74ebfad6c0d1a2c740ed9`, same 64 first verified TRAIN SEC reports (up to 1024 bytes each; 64 KiB) and disjoint next 32 reports (32KiB), same 48,829 disk pages of 1024 f32 slots per arm = 50,000,896 addressable expert weights and max 32 cached pages, same deterministic expert initializations, gate and routing. SHA-compare complete ordered per-prefix `BOUNDARY_V3` manifest for all arms. Same continuous-document heldout evaluator with frozen model weights. Four modes:
- `--pilot-rtrl2-continuous`: physical RTRL-2 scalar reference, H=1.
- `--pilot-rtrl3-paged1`: new shared-route scalar RTRL-3 exact-equivalence control, H=1/M=0/K=1.
- `--pilot-rtrl3-paged8`: H=8/M=2/K=2, fixed low-rank cross coupling.
- `--pilot-rtrl3-paged16`: H=16/M=2/K=4, fixed low-rank cross coupling.

All recurrent and expert optimizer rates unchanged, no clipping on recurrent gradients (limit 1e30); snapshot contains original expert pages and gate as before, with separate persisted `rtrl3_recurrence.tsv` of the actual trained multistate recurrent weights and config. **Reopening a fully integrated RTRL-3 model is not yet implemented; the recurrence sidecar is evidence, not a loader**. The original head's scalar recurrence file field remains stale for these new RTRL-3 modes; users must not treat `MorAnfRecurrent::reopen` as a complete RTRL-3 decoder. Add an explicit versioned loader and roundtrip test before production promotion.

## Scientific scope

This is an **exploratory repeated same-domain** SEC heldout previously inspected by RTRL-2, HYB1/2. It is **not a fresh sealed DEV** and must not be used to choose H=8 versus H=16 with an independent-generalization claim. This experiment isolates the *integration* and performs a same-atlas apples-to-apples comparison of observed logloss. However changing H,M,K together and adding 3H trainable recurrent weights means equal expert capacity but **not identical total recurrent parameter count**. UORO estimates eligibility at frozen weights and is only an online approximation when weights update per step; higher-variance gradient paths might regress. One run per seed is insufficient to estimate statistical significance; repeat seeds/fresh DEV before adoption.

Rights G0 remain `NOT_ADMITTED`; 2024 TEST stays `UNOPENED`. No claim of trained 25B parameters or of a production language model. See [RTRL-3 research #598](https://github.com/Memorithm/SML-GENIUS/issues/598).
