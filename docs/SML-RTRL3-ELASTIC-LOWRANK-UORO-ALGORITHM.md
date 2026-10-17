# RTRL-3 — Elastic low-rank online recurrence (executable Rust algorithm v0)

**Status:** scientific reference kernel, Rust-only. This is not a trained 25-billion-parameter language model and is not yet wired into the two-expert paged MOR head. It is designed to demonstrate a bounded-memory alternative to the dense RTRL Jacobian, which must be qualified independently before integration. RTRL-2 remains the measured production *research baseline*.

## 1. Recurrent state and Boolean input

For hidden dimension `H`, input byte pair `a_t,b_t`, GF(2) parity `φ_t=(-1)^(1+popcount(a_t AND b_t))` (equal to +1 for odd parity) and normalized population-count signal `x_t=(popcount(b_t)-4)/4`, write the state:

```text
z[k]     = sum_j V[k,j] h_prev[j] / H                         k=0..M-1
pre[j]   = a[j] h_prev[j] + b[j] x_t + c[j] φ_t
           + sum_k U[j,k] z[k]
h[j]     = tanh(pre[j])
```

Here `a[j],b[j],c[j]` are the **only trainable recurrent parameters** (3H), and `U,V` are deliberately **fixed** deterministic rank-M coupling factors in this research version. A small binary readout uses eight learned logistic `gain[bit],bias[bit]` pairs on the mean hidden state to predict the next byte. The readout is a toy supervised head, not the existing physical expert/MOR optimizer.

## 2. Gradient and the exact RTRL baseline

Let `θ∈R^(3H)` and `J_t=∂h_t/∂θ`, shape `H×3H`. At frozen weights:

```text
D[j,j] = 1 - h_t[j]^2
A_t    = D (diag(a) + U V / H)
B_t    = D * sparse_direct(h_prev, x_t, φ_t)
J_t    = A_t J_(t-1) + B_t
```

Each row of `B_t` contains just three nonzero local derivatives. Computing or retaining full `J` scales as O(H²), even though trainable recurrent parameter count is only O(H). **H=1 and M=0** uses the analytically exact scalar eligibility trace to recover the existing RTRL form. **H=2** has an independent *dense exact* reference inside tests only, compared with central finite differences; the production algorithm does not allocate it.

## 3. Rank-K UORO approximate eligibility

Represent `J_t≈(1/K) Σ_(k=1)^K u_k v_kᵀ`, each `u∈R^H`, `v∈R^(3H)`. For each rank-one estimator and time step:

```text
p       = A_t u_prev                       # O(H M), using U,V
nu[j]   = Rademacher(±1)                   # independent per hidden row
b       = nuᵀ B_t                          # exactly O(3H), sparse direct
xi      = Rademacher(±1)                   # independent sign
alpha   = balanced_scale(norm(v_prev), norm(p))
beta    = balanced_scale(norm(b), norm(nu))
u_next  = alpha * p + xi * beta * nu
v_next  = v_prev / alpha + xi * b / beta
```

For independent fresh signs, conditional expectation satisfies:

`E[u_next v_nextᵀ | previous estimator] = A_t u_prev v_prevᵀ + B_t`.

This is an **unbiased Monte Carlo estimator of the frozen-weight Jacobian**, not an exact gradient of the changing-parameter training trajectory. Its variance can be large: rank, step sizes, and clipping require independent empirical calibration. Balancing factors improve numerical behavior, but no variance guarantee is claimed.

## 4. Online update

For the eight bitwise Bernoulli losses, compute `credit=Σ_bit (p_bit-y_bit)*gain_bit`. Accumulate the recurrent gradient:

`g_p = (credit / H) * (1/K) Σ_k (Σ_j u_k[j]) v_k[p]`.

With `H=1,M=0`, substitute the exact three-element sensitivity vector. Clip `g` by its full Euclidean norm to the explicitly configured limit; update local weights by `θ ← clamp(θ - η * clip(g), -2,2)`. Update 16 readout weights via ordinary logistic SGD. Never propagate gradients across the *SGD operation itself*: this is online RTRL eligibility, not the derivative of the complete optimizer trajectory.

No parameter is a discrete 1-bit learnable weight in this kernel. GF(2) parity is a *fixed input feature*. The bitwise output objective trains floating-point recurrence/readout weights.

## 5. Resource budget

Keep only `H` recurrent values, `3H` local trainable coefficients, fixed mixing matrices of `2HM`, and `K` rank-one pairs with `(H+3H)K=4HK` f64 entries. Plus a 3-value exact scalar trace and 16 head values.

`payload_bytes = 8*(4H + 3 + 2HM + 4HK + 16)`

This counts f64 payloads, not Vec metadata, temporary allocations or the host allocator. Complexity per recurrent step is O(HM + K(HM + H)) plus bit head, not O(H²) storage for the Jacobian. A bounded configuration is enforced by `H<=64`, `K<=16`, `M<=8`. There is **no attention matrix** and no external CUDA dependency.

## 6. Validation and promotion criteria

- Exact scalar (H=1,M=0) trace equality to the RTRL-2 mathematical trace on identical features.
- Exact two-state (H=2,M=1) dense reference and finite differences; this reference is only a test oracle, never the scalable runtime.
- UORO estimator averages versus exact one-step Jacobian under many deterministic independent seeds, with explicit Monte Carlo tolerance (this tests unbiasedness *empirically*, not a proof of convergence).
- H=8,16,32 bounded memory, finite online losses, deterministic read-only evaluation and invalid-configuration rejection.
- **Not yet validated:** actual SEC SHA-pinned 50M paged MOR integration, learning quality, multiple-seed generalization, memory high-watermark, and GPU/SIMD execution. These are separate acceptance gates. No speed or accuracy benefit over RTRL-2 is asserted.
- Do not tune on the previously used SEC 32KiB DEV; obtain new sealed validation before choosing H/M/K/clipping. Keep original corpus rights `G0 NOT_ADMITTED` and 2024 TEST `UNOPENED`.

Source: `crates/sml-core/src/rtrl3_elastic.rs`. Prior design goal: [RTRL-3 issue #598](https://github.com/Memorithm/SML-GENIUS/issues/598).
