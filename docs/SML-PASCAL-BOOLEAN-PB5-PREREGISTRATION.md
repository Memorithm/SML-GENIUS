# PB-5 — composed Pascal-Boolean functional pipeline preregistration

Status: preregistered functional-composition protocol.

Prerequisites are the retained PB-1..PB-4 contracts in
`SML-PASCAL-BOOLEAN-SUITE-OBSERVATION.md`. This experiment does not reopen
PGF2 representation mixing and does not qualify a model architecture.

## Question

Can P-FEISTEL, P-ZETA, P-ROUTER and P-ANF be composed in one deterministic
K=2048 Boolean pipeline while preserving an independently computed direct
oracle for every address?

## Frozen pipeline

Logical state width is K=2048 = 2^11.

For each deterministic source state `x`:

1. encode `x` with the retained four-round P-FEISTEL construction;
2. decode it back to `x'`;
3. compute `z = P_ZETA(x')`;
4. for each eleven-bit address `n in 0..2048`, P-ROUTER defines
   `S(n) = { k | k subseteq n }`;
5. the native parity result is `z[n]`;
6. the direct parity oracle enumerates `S(n)` and XORs `x'[k]`;
7. evaluate the fixed ANF address gate;
8. final output is `parity AND gate`.

The direct end-to-end oracle uses the original unencoded source state,
exhaustive subset enumeration, and a direct Boolean expression for the same
gate.

## Frozen ANF address gate

Let `a0..a10` be the bits of address `n`.

The gate is:

```text
g(n) = a0 XOR (a1 AND a3) XOR (a2 AND a5 AND a8) XOR (a7 AND a9)
```

Expected nonzero ANF monomial masks are therefore exactly:

```text
1
(1<<1)|(1<<3)
(1<<2)|(1<<5)|(1<<8)
(1<<7)|(1<<9)
```

No coefficient may be selected after observing PB-5 output.

## Frozen corpus

- 64 deterministic distinct K=2048 Boolean states;
- all 2,048 addresses are evaluated for every state;
- total end-to-end answer comparisons: 131,072;
- P-FEISTEL depth: exactly four rounds.

The corrected non-aliasing deterministic stream from PB-1..PB-4 is reused.

## Matched evidence

The harness must report separately:

- source-state uniqueness;
- Feistel encoded-state uniqueness;
- Feistel inverse failures;
- P-ZETA versus direct P-ROUTER parity mismatches;
- ANF-gate versus direct-expression mismatches;
- final end-to-end answer mismatches;
- exact query counts;
- exact structural operation/accounting fields for the retained component
  implementations;
- implicit route representation versus a fully materialized 2048x2048
  membership relation;
- deterministic replay identity.

Cost reports are descriptive. PB-5 is not permitted to declare a speed winner
without a measured matched wall-clock benchmark.

## Hypotheses

### C1 — reversible storage compatibility

Across all 64 source states:

- source collisions = 0;
- Feistel encoded-state collisions = 0;
- Feistel inverse failures = 0.

### C2 — subset routing / zeta equivalence

Across all 64 states and all 2,048 addresses:

`P_ZETA(x)[n]` exactly equals direct XOR over the P-ROUTER submask set.

Required mismatches: 0.

### C3 — ANF gate exactness

Across all 2,048 addresses:

- transformed truth-table coefficients equal the four frozen monomial masks;
- canonical `AnfPolynomial` evaluation equals the direct Boolean expression.

Required mismatches: 0.

### C4 — end-to-end composition exactness

For every state/address pair, the composed final output exactly equals the
independent direct oracle.

Required mismatches: 0 out of 131,072.

### C5 — deterministic replay

Two complete `cargo run --locked --release` executions must be byte-identical.

### C6 — accounting completeness

The output must expose the structural costs and representation sizes needed to
audit the composition. This gate does not require PB-5 to beat every control.

## Decision

If C1..C6 hold:

`PB5_DECISION,RETAIN_COMPOSED_FUNCTION`

Otherwise:

- a mathematically meaningful mismatch with valid fixtures -> `CLOSE_COMPOSED_FUNCTION`;
- a broken fixture/oracle/shared primitive -> `INVALID`.

A `RETAIN_COMPOSED_FUNCTION` result means only that the four retained
functions compose exactly under this frozen task. Any SBG, MOR, Delta-KV,
context, training, language-quality or model-level experiment requires its own
prospective protocol.

## Reproducibility

CI must:

- run the complete PB-5 harness twice under `--locked --release`;
- compare stdout byte-for-byte;
- print the first replay;
- retain the replay and SHA-256 manifest only after a green execution PR.
