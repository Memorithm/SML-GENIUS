# SML-0 executable architecture contract

Status: research specification. No model-quality, novelty, or performance claim is made by this document.

## Goal

SML-GENIUS explores a non-attention autoregressive architecture whose capacity is primarily stored as Boolean / low-bit circuits and direct-address memory structures. The design target is a 25-billion-learned-parameter artefact whose inference path is bounded to 8 GiB system RAM and 2 GiB VRAM while targeting at least 40 decoded tokens/s on a future frozen reference machine.

Those numbers are engineering targets, not achieved measurements.

## Prohibited candidate mechanisms

The SML candidate core must not introduce any of the following as a hidden substitute for attention:

- Q/K/V projections as the sequence interaction primitive;
- QK^T or another all-pairs learned token similarity matrix;
- softmax over token-pair scores;
- all-pairs Hamming / POPCOUNT ranking;
- a dense token-by-token score or relation matrix;
- a KV cache whose semantics are those of Transformer attention;
- an address lookup implemented by scanning all historical tokens.

External baselines may use these mechanisms. Candidate SML implementations may not.

## State transition

For token t, SML maintains:

- S[t]: bounded persistent recurrent state;
- M[t]: bounded persistent associative memory;
- W[t,r]: disposable reasoning workspace at recurrent depth r;
- R[t,r]: direct Boolean/algebraic route decision;
- P[t,r]: selected circuit-atlas pages.

A token is committed only after one or more internal reasoning cycles:

    (token[t], S[t-1], M[t-1])
        -> W[t,0]
        -> cycle 1 -> W[t,1]
        -> ...
        -> cycle R[t] -> W[t,R[t]]
        -> COMMIT -> (S[t], M[t], token[t+1])

The working hypothesis is that recurrent depth can increase reasoning compute without increasing the physical parameter count. This must be tested against fixed-depth controls.

## Recurrent-depth ladder

The experimental ladder is RD1, RD2, RD4, RD8, RD16, RD32, plus an adaptive policy. The reference implementation begins with deterministic stability-based halting. Learned halting is a later experiment, not part of SML-0 evidence.

Persistent memory must not be mutated speculatively at every internal cycle. A later commit protocol will define which workspace information is allowed to cross the token boundary.

## Multi-algebra research split

The initial hypothesis assigns distinct roles rather than summing algebraic systems indiscriminately:

- Boolean logic: admission, masks, gates and control;
- GF(2): binding, signatures and address transforms;
- algebraic normal form / Zhegalkin: compositional Boolean relations;
- max-plus: bounded competition, temporal readiness and priority after candidate reduction.

Each contribution requires an ablation. No component is promoted solely because it is mathematically attractive.

## Exact accounting

A learned physical parameter is counted once. Deterministically generated connectivity is not inflated into virtual parameter count.

The initial 25B representation hypothesis is:

- 21.25B Boolean parameters at 1 physical bit;
- 2.50B ternary parameters at 2 physical bits;
- 1.25B residual parameters at 4 physical bits.

This is exactly 31.25 billion packed bits = 3,906,250,000 packed bytes before metadata and alignment.

Every experimental report must separately record:

1. total learned parameters;
2. physical packed bits;
3. active learned parameters per recurrent cycle, with p50/p95/p99 where applicable;
4. total active parameter touches per emitted token;
5. Boolean primitive operations;
6. RAM, VRAM and storage traffic;
7. recurrent depth;
8. latency and tokens/s.

## Memory hierarchy target

    SSD / durable atlas
      -> RAM WARM cache (hard target <= 8 GiB)
      -> VRAM HOT cache (hard target <= 2 GiB)
      -> LIVE circuits / workspace

The normal decode path should ultimately avoid synchronous SSD reads. That is a target to validate, not a current property.

## Stage-0 exit gate

SML-0 exits only when the repository can demonstrate, with deterministic tests:

- exact bit/parameter accounting;
- reference Boolean circuit execution;
- direct routing without history scans;
- bounded recurrent-depth execution and deterministic halting;
- structural rejection or audit hooks for prohibited attention-like mechanisms;
- reproducible CI on the pinned Rust toolchain.
