# DKV-2a-N8 — fixed-width PAGE/PAGE_VALUE hybrid protocol

Status: frozen before implementation/execution.

Parent observation:
`SML-DKV-2A-N7-VALUE-DOMAIN-OBSERVATION.md`.

## Motivation

N7 established both sides of the current tradeoff:

- PAGE preserves one-bit V continuity but retains same-chain key correlation
  around 0.68 and the N4/N5/N6 crosstalk ceiling;
- PAGE_VALUE reduces same-chain cosine to about 0.02 and restores K=1024
  teacher/free-running capability to 100%, but one V-bit perturbation makes the
  key nearly orthogonal and gives 0/96 next-value recovery.

N8 asks whether a fixed-width hybrid can retain enough PAGE geometry for local
continuity while using enough PAGE_VALUE coordinates to suppress correlated
same-chain writes.

No core API is introduced.

## Hybrid construction

For each semantic state, compute two current-library projections:

1. PAGE:
   `page_seed = mix64(base_seed ^ page * PAGE_C)`;
2. PAGE_VALUE:
   `value_seed = mix64(page_seed ^ V_u64 * VALUE_C)`.

Both use:

- `DeltaKvAmbiguityProjection::ActiveOnly`;
- current default semantic/MOR weights;
- PageId(5), one selected expert;
- the existing `DeltaKvSemanticProjector`.

For fraction numerator q in {0,1,2,3,4}, build one K-dimensional hybrid key:

- coordinate i comes from PAGE_VALUE when `i % 4 < q`;
- otherwise it comes from PAGE.

Therefore q/4 of coordinates are value-domain conditioned and total key width
remains exactly K.

The harness does **not** renormalize the stitched key. DeltaKvMemory already
normalizes each update by the actual key norm. Exact key norm is reported.

Both source projections are computed for every q so logical projection work is
comparable across fractions. This Development harness is therefore intentionally
more expensive than a future specialized endpoint implementation.

## Frozen grid

- Rust 1.89.0;
- SemanticState<64>;
- K in {256,1024};
- q in {0,1,2,3,4};
- G/load in {4,16};
- 16 deterministic seeds;
- historical N4/N6 chain generator and shuffled write order;
- PageId(5);
- full-rate Q12 writes;
- read ambiguity threshold 128.

## P1 — low-load crosstalk and geometry

G=4.

For every q, K and terminal class L0/L1/L2/L3:

- SELF_SAME terminal exact+clear recall;
- FULL terminal exact+clear recall;
- same-chain mean absolute key cosine;
- cross-chain mean absolute key cosine;
- exact-key collisions;
- mean squared key norm.

## P2 — teacher-forced retrieval

For K in {256,1024}, G in {4,16}, q in {0..4}:

- every exact advancing source hop h0/h1/h2;
- terminal self-hop separately.

Report exact signs, exact+clear, wrong coordinates and minimum absolute read.

## P3 — six-cycle free-running capability

For every K/G/q:

- derive the hybrid key from the current free-running state each cycle;
- read the current DeltaKvMemory;
- apply the public threshold rewrite rule exactly as in N7;
- retain F1/F2/F3/F6 and non-causal ORACLE terminal correctness.

## P4 — local V robustness

G=4, exact written non-terminal sources.

For h in {1,2,4}:

- deterministic V-only perturbation;
- mean absolute cosine to the exact-source hybrid key;
- exact-key equality;
- exact+clear retrieval of the exact true next value.

## P5 — logical work

Report for each K/q:

- semantic coordinates read;
- bucket accumulations;
- quantized coordinates produced by the two source projections;
- final key width K.

All q arms must have equal projector-work counters by construction.

## Frozen hypotheses

H1 — q=0 and q=4 reproduce the retained N7 PAGE and PAGE_VALUE controls at
K=1024/G=4, including terminal FULL counts and one-bit perturbation behavior.

H2 — at K=1024/G=4, same-chain mean absolute cosine is non-increasing with q
for L1/L2/L3.

H3 — at K=1024/h=1, perturbed-query mean absolute cosine is non-increasing with
q.

H4 — at least one interior q in {1,2,3} at K=1024/G=4 satisfies all:
- F6 >=90%;
- teacher-forced advancing exact+clear >=90% at h0/h1/h2;
- one-bit perturbed-query exact+clear retrieval >=20%.

H5 — at least one interior q in {1,2,3} at K=1024/G=16 satisfies:
- F6 >=90%;
- one-bit perturbed-query exact+clear retrieval >=20%.

H6 — no exact-key collision occurs in any written-state K/q/load cell.

H7 — logical projector-work counters are identical across q for fixed K and
number of hybrid-key constructions.

## Selection rule

If H4 has one or more qualifying q values, the low-load candidate is the
**smallest q** satisfying H4, preserving the largest PAGE fraction.

If H5 has one or more qualifying q values, the high-load candidate is likewise
the smallest qualifying q.

If no interior q satisfies a gate, report **no hybrid candidate**. Do not relax
thresholds after execution.

## Decision boundary

N8 is a harness-only tradeoff study.

No core domain API follows without:
- canonical multi-expert semantics;
- unbalanced-page/cross-page sharing qualification;
- DKV-3a exact record/subject/provenance replay;
- physical cost accounting.

No controller, context or model promotion follows.
