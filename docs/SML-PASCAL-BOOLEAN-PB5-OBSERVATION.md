# PB-5 — composed Pascal-Boolean functional observation

Status: retained positive functional-composition evidence under
`SML-PASCAL-BOOLEAN-PB5-PREREGISTRATION.md`.

PB-5 combines the four retained functional primitives from PB-1..PB-4 at
K=2048. It does not reopen the closed PGF2 representation-mixing branch and it
does not qualify a model architecture.

## Execution identity

- protocol PR: #213;
- protocol merge: `0988de1d1e15babb9ffd48406eac0436cc964f8a`;
- implementation PR: #214;
- executed head: `78d91c20bcb7daaf9d6ea38e6418517b0a223665`;
- implementation merge: `9e5888fdbe23c69eda08803c6b52c182dca7e1b6`;
- GitHub Actions run: `37435044606`;
- job: `112174676286`;
- runner: `tarek-scirust-arm64-01`, machine `tarek`;
- Rust 1.89.0, target `aarch64-unknown-linux-gnu`;
- retained replay: `research/pascal-boolean-pb5/CURRENT-REPLAY.csv`;
- replay SHA-256:
  `24929c525983f15accee1663ac316c5ba1613315dad4c4d985d3e61f61066e86`.

CI ran the complete release harness twice and required byte-identical stdout.

## Result

All preregistered composition gates hold.

- C1 reversible storage: zero source collisions, zero encoded collisions, zero
  inverse failures and zero deterministic Feistel replay failures across 64
  states.
- C2 P-ROUTER/P-ZETA equivalence: zero parity mismatches across 131,072
  state/address comparisons.
- C3 P-ANF gate: exact coefficient masks `1|10|292|640`, zero coefficient
  mismatches and zero direct-expression mismatches across all 2,048 addresses.
- C4 end-to-end: zero final mismatches across 131,072 independent oracle
  comparisons.
- C5 deterministic replay: enforced by byte-for-byte CI comparison.
- C6 accounting: all frozen structural fields are present and internally
  consistent.

The retained decision is:

`PB5_DECISION,RETAIN_COMPOSED_FUNCTION`

## Structural accounting

Per source state, the composed path reports:

- P-ZETA: 11,264 logical XOR operations and 272 packed-word updates;
- four-round Feistel encode + decode: 57,344 logical XOR operations, 8,192
  logical AND operations, 1,280 packed-word updates and 16,384 coordinate-map
  operations;
- direct exhaustive subset oracle: 177,147 source reads per state;
- native transformed query path: one transformed-state bit read per address,
  131,072 query reads for the full 64-state panel.

The implicit submask router requires zero learned route bits. A materialized
2048x2048 membership relation would contain 4,194,304 Boolean entries. The
frozen ANF gate has four active coefficients versus a 2,048-bit truth table.

These are structural counts, not measured latency claims.

## Interpretation

PB-5 establishes exact interoperability of P-FEISTEL, P-ZETA, P-ROUTER and
P-ANF under the frozen synthetic composition task. It is stronger than testing
the four components independently because the final answer is checked against
an independent end-to-end oracle after encode/decode, transformed subset
aggregation and nonlinear address gating.

It still does not establish:

- better language-model loss or perplexity;
- superior semantic retrieval on SBG;
- better MOR expert selection;
- Delta-KV improvement;
- lower wall-clock latency than matched alternatives;
- trainability or learned usefulness;
- a reason to promote the construction into the 25B model.

The next justified Pascal experiment must test measurable task utility against
matched controls rather than another exactness-only composition.
