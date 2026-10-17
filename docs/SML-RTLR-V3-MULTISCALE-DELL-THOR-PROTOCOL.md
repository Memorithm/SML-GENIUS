# RTLR-ELASTIC v3 — exact local multiscale recurrent clocks, native Dell and Thor preregistration

## Motivation and compatibility

On the same physical 50,000,896-slot paged MOR head, RTRL-2 continuous gave 3.616609952 heldout bits/byte and RTLR-v2 gave 3.616884374 on an **already inspected** SEC DEV. The speculative advance is not to widen a uniform cell, but to create independent recurrent cells whose **temporal horizons differ**. V3 is opt-in. V2 and the original scalar RTRL-2 default and serialized checkpoint encoding are unchanged. When active capacity is one, V3's anchor is exactly the original scalar RTRL2 path.

For cell index j, define leak `lambda(0)=1` and `lambda(j)=4^{-min(j,12)}` for j≥1, a fixed nonlearned timescale. Compute
```
u[j,t] = tanh(a_j*h[j,t-1] + b_j*x_t + c_j*anf_t)
h[j,t] = h[j,t-1] + lambda(j)*(u[j,t] - h[j,t-1])
J[j,t] = ((1-lambda(j)) + lambda(j)*(1-u[j,t]^2)*a_j) * J[j,t-1]
       + lambda(j)*(1-u[j,t]^2)*[h[j,t-1], x_t, anf_t]
```
J is an analytically exact **local forward sensitivity for frozen weights**, because no cross-cell recurrent coupling exists. It does **not** differentiate through online SGD updates or an arbitrary full joint neural network. The output is the existing bounded residual gate mixing anchor and extra-cell mean; the original TRAIN-only marginal-capacity controller, SHA-backed dormant weights, physical MOR experts and read-only heldout are unchanged.

The v3 checkpoint body has **a different schema tag** `RTLR_V3_MULTISCALE_V1`. Restoring through the verified physical MOR+atlas full model loader reconstructs and asserts the same cell-rate mode. The v2 tag `RTLR_V2_FULL_V1` still reopens unchanged. Full atomic generation commits and independently proven resumed training remain issue #608.

## Two *distinct* experimental tracks

1. **Portable synthetic kernel microbenchmark**: a new Rust CLI `sml-rtlr-multiscale-bench` generates disjoint deterministic TRAIN (32,768 supervised single-bit positions) and DEV (8,192) streams, with targets requiring parity of signals at lags 1,16,128 and 1024 bytes. It runs identical code/config/data hashes on Dell Xeon X64 and Jetson Thor aarch64/ARM64. Arms are scalar, RTLR-v2 cap8, RTLR-v3 cap8, RTLR-v3 cap16, three fixed stochastic seeds each. Report independent process release training and inference times, predicted-bit throughput, train/DEV single-target bits/position, growth/prune/peak/active width, *per-process* Linux VmHWM, exact source SHA, Rust toolchain/CPU. No disk-paged MOR expert atlas is used in this microbenchmark; do not compare its scores to the earlier 3.6166 SEC eight-bit target loss.
2. **Physical MOR/ANF pilot**: new flags `--pilot-rtlr-v3-8` and `--pilot-rtlr-v3-16` let Dell qualify the *same* 50M-addressable physical experts, same original 64KiB SEC TRAIN and old 32KiB DEV source/prefix SHA and route as RTRL-2/V2. Its purpose is **mechanical integration and exploratory comparison**; old DEV is not an independently fresh generalization estimate. Do not copy non-admitted SEC data from Dell to Thor. Thor should first use synthetic self-generated inputs and the same CPU-only executable. Any future physical-corpus Thor work requires dataset availability, provenance, authorization and identical corpus SHA.

## Source, hardware and statistical limits

Both native hosts must run the **exact same Git SHA**, Rust 1.89 release profile, no NVIDIA application software, no model server startup and no GPU kernels. Dell X64 and Thor ARM64 have different cores, instruction sets, operating loads, clocks and memory subsystems: cross-host wall-time differences are descriptive only, not controlled GPU speedups. Record host CPU model, architecture and memory high-water per process; report no Thor numbers before its job returns success.

Three seeds share the same deterministic TRAIN/DEV content, so they are sensitivity samples, not dataset resamples or 95% statistical confidence intervals. The synthetic target is difficult and does **not** establish natural-language understanding; performance on unseen real text requires fresh sealed independent data and larger/multiple-seed matched compute budgets. Keep RTRL-2 baseline and exact anchor non-regression.

All code Rust, no Transformer attention, no NVIDIA runtime dependency. SEC rights G0 **NOT_ADMITTED**, original 2024 TEST **UNOPENED**. The 50M atlas is addressable f32 capacity, not 50M trained weights, and no 25B model has been trained.
