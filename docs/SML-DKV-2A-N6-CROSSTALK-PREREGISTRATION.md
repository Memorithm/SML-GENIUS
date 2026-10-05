# DKV-2a-N6 — terminal-write crosstalk decomposition protocol

Status: frozen before implementation/execution.

Parent observation:
`SML-DKV-2A-N5-HOP-FIDELITY-OBSERVATION.md`.

## Question

N5 localized the low-load L2/L3 capability failure to teacher-forced
associative retrieval from exact stored states.

The terminal self-write pattern at G=4 exactly reproduces the N4 ORACLE class
ceiling:

- L0 terminal: 16/16;
- L1 terminal: 16/16;
- L2 terminal: 13/16;
- L3 terminal: 8/16.

N6 tests whether this degradation is primarily caused by correlated writes from
the **same chain** (shared M/R, changing V) or by unrelated writes from other
chains.

No new projector mechanism is introduced.

## Frozen candidate and chain fixture

- Rust 1.89.0;
- current `ActiveOnly`;
- N3B outer PageId(5)-derived seed;
- current DeltaKvMemory;
- `SemanticState<64>`;
- K in {64,256,1024};
- G=4 only;
- seeds 0..15;
- historical N4/N5 chain generator unchanged;
- chain length = chain index mod 4;
- pairwise V Hamming distance >=16 inside each chain;
- M/R shared within one chain;
- terminal writes itself;
- all writes retain the original N4 shuffled global order;
- full-rate Q12 writes;
- exact+clear read requires all 64 target signs correct and every |read| >128.

Each write receives stable metadata:
`(chain_index, source_hop, terminal, global_order_position)`.

## C1 — terminal causal memory arms

For the terminal state of each L0/L1/L2/L3 chain, build and query the following
fresh memories.

### SELF

Only the terminal self-write.

### SELF_SAME

The terminal self-write plus every other write from the same chain.

Subset execution preserves the original shuffled global relative order.

### SELF_CROSS_MATCHED

The terminal self-write plus exactly L writes from **other** chains, where L is
the terminal chain length and therefore also the number of same-chain
non-terminal writes.

For each terminal query, execute four deterministic matched replicates.

Cross candidates are ranked by:

`mix64(seed ^ query_id*C1 ^ replicate*C2 ^ candidate_write_id*C3)`

with constants frozen in the implementation before first execution. Select the
lowest L scores, then execute selected writes plus SELF in their original global
relative order.

For L0, no cross write is selected.

### FULL

All writes in the original N4 shuffled order.

Report by K and terminal chain length:

- number of query/replicate observations;
- exact-sign recall;
- exact+clear recall;
- total wrong coordinates;
- minimum absolute read magnitude.

SELF, SELF_SAME and FULL have 16 observations per K/class.
SELF_CROSS_MATCHED has 64 observations per K/class except that the four L0
replicates are intentionally identical controls and are still retained.

## C2 — key geometry

Using the exact projected terminal key as the anchor, compare it with:

- every other key from the same chain;
- every key from other chains.

Report separately by K and terminal chain length:

- pair count;
- mean signed cosine;
- mean absolute cosine;
- maximum absolute cosine;
- exact-key equality count.

This geometry uses the existing projector only.

## C3 — FULL failure reproduction

The FULL arm must reproduce the retained N5 terminal exact+clear counts at G=4
for K=1024:

- L0 = 16/16;
- L1 = 16/16;
- L2 = 13/16;
- L3 = 8/16.

Mismatch invalidates the run.

## Frozen hypotheses

H1 — SELF exact+clear recall is 100% in every K/L cell.

H2 — at K=1024, mean absolute same-chain cosine exceeds mean absolute
cross-chain cosine for each L in {1,2,3} by at least 0.20.

H3 — at K=1024, SELF_SAME exact+clear terminal recall is lower than
SELF_CROSS_MATCHED by at least:
- 5 percentage points for L2;
- 20 percentage points for L3.

H4 — at K=1024, at least 80% of FULL terminal failures in L2/L3 are reproduced
by SELF_SAME for the same terminal query.

H5 — K widening does not eliminate same-chain interference: SELF_SAME L3
exact+clear recall at K=1024 remains below 90%.

H6 — FULL K=1024 reproduces C3 exactly.

A failed hypothesis remains failed. No projector weight, seed rule, chain
generator, match count or threshold may change after first execution.

## Interpretation

If H2-H5 hold, the next mechanism should target same-chain key orthogonality or
write interference while preserving the source/provenance boundary.

If they fail, same-chain crosstalk is not sufficient and cross-chain/global
capacity or update-order effects remain primary candidates.

N6 does not authorize another field-weight sweep; the owner-supplied DKV-4j
value-weight result is already negative.

## Evidence retention

CI must execute the release panel twice, require byte-identical output and print
the first output.

No core API, controller, context or model promotion follows.
