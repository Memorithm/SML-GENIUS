# SML Pascal-Boolean functional suite — preregistration

Status: preregistered research protocol. No model-quality or architecture promotion may be inferred from implementation alone.

## Purpose

Evaluate useful Boolean functions induced by the Pascal matrix modulo 2 at the native SML semantic width K=2048.

This programme is distinct from PGF2-3/3B representation mixing. PGF2-3B closed the representation-mixing branch after no block-local candidate satisfied every frozen gate. The present suite evaluates Pascal/GF(2) as a functional primitive for subset aggregation, routing, ANF interaction extraction, and reversible nonlinear mixing.

## Frozen execution order

1. PB-1 P-ZETA — subset-parity aggregation.
2. PB-2 P-ROUTER — Pascal/submask sparse routing.
3. PB-3 P-ANF — Boolean interaction extraction / reconstruction.
4. PB-4 P-FEISTEL — reversible nonlinear Pascal composition.
5. PB-5 COMPOSED — only after PB-1..PB-4 have all produced valid evidence.

All PB-1..PB-4 experiments run regardless of whether an earlier candidate is retained or refuted.

An earlier result may block later experiments only if it establishes a shared-premise failure:
- K=2048 state construction is invalid;
- the packed Pascal transform disagrees with the direct GF(2) oracle;
- deterministic replay fails;
- exact accounting is internally inconsistent;
- a required primitive cannot be implemented under the repository safe-Rust / no-unsafe boundary.

Performance or quality failure of one function is not a stop condition for the other functions.

## Common invariants

- Logical width: K=2048 = 2^11.
- Representation: 32 packed u64 words, 256 bytes of state.
- Integer/Boolean execution for the evaluated primitive.
- No Transformer attention, no all-pairs token scoring.
- Deterministic fixed seeds.
- Two release executions must be byte-identical.
- Existing PGF2 packed transform is the Pascal-zeta oracle-qualified primitive.
- Every candidate reports exact logical operation counts where derivable and explicit measured wall-clock only when a physical runner measurement is used.
- No model-context promotion from this suite alone.

## PB-1 — P-ZETA subset-parity aggregation

For x in GF(2)^2048 and n in [0,2047]:

    z[n] = XOR_{k subseteq n} x[k]

Qualification:
- packed transform equals direct submask enumeration for every n;
- applying the transform twice reconstructs x exactly;
- zero collisions across the fixed deterministic corpus;
- report preprocessing cost, direct-query XOR work, transformed-query reads, and break-even query count;
- report one-bit avalanche distribution (min/median/p95/max Hamming effect).

Interpretation:
PB-1 is retained as a subset-parity aggregation primitive only if exactness and deterministic replay hold. Retrieval/noise superiority is not required because that was a different PGF2-3B hypothesis.

## PB-2 — P-ROUTER sparse subset routing

Treat each 11-bit route address n as selecting expert/page indices k satisfying k subseteq n.

Frozen route cardinalities are induced by popcount(n):
- popcount 3 -> 8 routes;
- popcount 5 -> 32 routes;
- popcount 7 -> 128 routes;
- popcount 9 -> 512 routes.

Qualification:
- selected set exactly matches exhaustive subset membership for all 2048 addresses;
- cardinality equals 2^popcount(n) for all addresses;
- no duplicate selected indices;
- direct submask enumeration work is reported;
- report active fraction and route overlap by popcount band;
- compare semantic route-bit/table storage with an explicit 2048-entry learned direct-routing table when the comparison is structurally meaningful.

PB-2 failure does not block PB-3/PB-4 unless subset enumeration itself is incorrect.

## PB-3 — P-ANF Boolean interaction transform

For 11 Boolean variables, enumerate the complete 2048-entry truth table. Use the Pascal/Mobius transform over GF(2) to convert between truth-table values and ANF coefficients.

Qualification corpus:
- constant;
- single variable;
- XOR of variables;
- pairwise AND;
- majority-like fixed Boolean fixture;
- deterministic random truth tables.

Qualification:
- exact truth-table -> ANF -> truth-table reconstruction;
- exact coefficient agreement against direct monomial evaluation on the named fixtures;
- degree histogram for every fixture;
- low-degree truncation at d<=2, d<=3, d<=4 with exact retained coefficient counts and reconstruction error over all 2048 assignments;
- report ANF monomial evaluation work.

PB-3 tests representational utility; no language-model quality claim is permitted.

## PB-4 — P-FEISTEL reversible nonlinear composition

Split the 2048-bit state into two 1024-bit halves L,R. Define a deterministic Feistel network using Boolean round functions composed from:
- Pascal-GF(2) on 1024-bit halves;
- fixed rotations/permutations;
- AND/XOR nonlinear terms.

Frozen depths: 2, 4, 6 rounds.

Qualification:
- exact inverse for every deterministic corpus item and every depth;
- zero collisions in the corpus;
- one-bit avalanche distribution;
- repeat-input determinism;
- an explicit XOR-superposition witness must demonstrate that the candidate is not a linear GF(2) map;
- report logical XOR/AND work and packed-word updates;
- compare avalanche and work against identity, fixed permutation, XOR-permutation, and linear Pascal controls.

PB-4 is retained as a reversible nonlinear primitive only; it is not promoted as a model layer from this suite.

## PB-5 — composed functional candidate

Run only after PB-1..PB-4 have valid evidence (HOLDS or REFUTED is acceptable; INVALID is not).

The composed test may combine retained functional roles, but it must not rewrite PB-1..PB-4 thresholds after observing their outputs. A separate preregistration is required before any model-quality, training, SBG, MOR, DKV, context, or 25B promotion test.

## Evidence and decision vocabulary

Each hypothesis emits exactly one of:
- HOLDS
- REFUTED
- INVALID

Each experiment emits a final:
- RETAIN_FUNCTION
- CLOSE_FUNCTION
- INVALID

A retained function means only that its frozen functional contract was satisfied.

## Stop / dependency rule

Continue automatically from PB-n to PB-(n+1) after a HOLDS or REFUTED result.

Stop the sequence only on INVALID evidence that falsifies a shared premise needed by the next experiment. The reason must be emitted explicitly in machine-readable evidence.

## Reproducibility

The implementation branch must:
- run under cargo --locked --release;
- execute the complete suite twice;
- cmp the complete stdout byte-for-byte;
- print the first execution as CI evidence;
- preserve the resulting output and a SHA-256 manifest in a later retention change after CI completes.
