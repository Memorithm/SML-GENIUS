# Pascal-Boolean functional suite — K=2048 observation

Status: retained positive functional evidence under
`SML-PASCAL-BOOLEAN-SUITE-PREREGISTRATION.md`.

This suite is distinct from the closed PGF2-3B representation-mixing objective.
It tests Pascal/GF(2) as a set of explicit Boolean functions, not as the default
SBG representation mixer.

## Execution identity

- protocol PR: #210;
- protocol merge: `e9c771ab411c48bedb72fc54f5acda1e72fe7d16`;
- implementation PR: #211;
- executed head: `fa29a7fd6ce82736daeb3b7f1dce8d5892fbdbb9`;
- implementation merge: `ed9eae2c66e9936827660814a8da7d0b2e8500d8`;
- retained GitHub Actions run: `37429866442`;
- job: `112157954175`;
- runner: `tarek-scirust-arm64-01`, machine `tarek`;
- Rust 1.89.0, target `aarch64-unknown-linux-gnu`;
- retained output: `research/pascal-boolean-suite/CURRENT-REPLAY.csv`;
- retained SHA-256:
  `6d9f0b515ac2f31304811b9f603afd151c79458626b94bc8da16523ba52d2cf9`;
- manifest: `research/pascal-boolean-suite/MANIFEST.json`.

The CI executes the suite twice under `--locked --release` and requires
byte-identical stdout before printing the retained replay.

## Harness correction before classification

An earlier implementation run (#720, run `37429522943`) stopped PB-1 with 32
collisions in a 64-item corpus. The Pascal oracle and inverse checks were both
zero-failure. Inspection showed that the deterministic stream constructor used
`seed | 1`, which aliases every even/odd seed pair. The harness was corrected
before classifying PB-1 by removing this seed alias and by reporting source
collisions separately from output collisions.

The corrected frozen replay has:

- source collisions: 0;
- output collisions: 0;
- oracle failures: 0;
- inverse failures: 0.

No candidate threshold or decision rule was changed.

## PB-1 — P-ZETA

PB-1 HOLDS and is retained as an exact subset-parity aggregation primitive.

At K=2048:

- 11 butterfly stages;
- 11,264 logical XOR updates;
- 272 packed-word updates;
- 256 state bytes;
- zero scratch words in the underlying packed transform;
- direct enumeration of all output subsets requires 177,147 source reads;
- a precomputed transformed state provides the 2,048 outputs as direct reads.

For uniformly distributed single-coordinate subset-parity queries, the frozen
structural accounting crosses the preprocessing break-even at 132 queries.

Single-input-bit propagation is highly nonuniform, as expected from the
Sierpiński/Pascal structure:

| statistic | output bit flips |
|---|---:|
| minimum | 1 |
| median | 48 |
| p95 | 256 |
| maximum | 2,048 |
| sum over all 2,048 basis inputs | 177,147 |

This is exact algebraic evidence, not a claim that such propagation is optimal
for semantic noise robustness.

## PB-2 — P-ROUTER

PB-2 HOLDS and is retained as an implicit subset router.

Across all 2,048 eleven-bit addresses:

- exhaustive membership failures: 0;
- cardinality failures: 0;
- duplicate-selection failures: 0;
- the selected cardinality is exactly `2^popcount(address)`.

Frozen routing bands:

| address popcount | addresses | selected routes | active fraction |
|---:|---:|---:|---:|
| 3 | 165 | 8 | 0.3906% |
| 5 | 462 | 32 | 1.5625% |
| 7 | 330 | 128 | 6.25% |
| 9 | 55 | 512 | 25% |

The subset rule itself requires zero learned routing bits. Materializing the
complete 2048x2048 Boolean membership matrix would require 4,194,304 bits.
The existing single-target direct-routing comparison is not structurally
equivalent because P-ROUTER selects a set of routes rather than one target.

## PB-3 — P-ANF

PB-3 HOLDS and is retained as an exact truth-table <-> ANF transform for eleven
Boolean variables.

Across the frozen fixtures:

- reconstruction failures: 0;
- expected-coefficient failures: 0;
- direct ANF evaluation failures: 0.

The named low-degree fixtures are represented exactly at their expected degree:
constant, one variable, XOR of three variables, one pairwise AND, and the
three-variable majority fixture.

The random controls show the important limitation. Their exact ANFs contain
976 to 1,052 coefficients. Truncation at degree <=4 retains only 272 to 286
coefficients and still produces 724 to 803 wrong truth-table rows out of 2,048
(35.35% to 39.21% error). Therefore the result supports low-degree interaction
extraction when the target actually has low algebraic degree; it does not show
that arbitrary Boolean functions are well approximated by low-degree ANF.

## PB-4 — P-FEISTEL

PB-4 HOLDS and is retained as a reversible nonlinear Pascal composition.

All frozen depths have zero inverse failures, zero deterministic-replay
failures and zero corpus collisions.

| rounds | nonlinearity witness | median avalanche | p95 | XOR ops | AND ops |
|---:|---|---:|---:|---:|---:|
| 2 | basis 0,2 | 393 | 729 | 14,336 | 2,048 |
| 4 | basis 0,2 | 996 | 1,054 | 28,672 | 4,096 |
| 6 | basis 0,4 | 1,025 | 1,062 | 43,008 | 6,144 |

For comparison, identity/permutation/XOR-permutation each have median avalanche
1, while the linear global Pascal control has median 48 and p95 256.

At six rounds the median one-bit avalanche is 1,025 of 2,048 output bits,
approximately half the state. This is strong diffusion under this fixture, but
it is not evidence of lower language-model loss, better calibration, improved
retrieval, or faster inference.

## Decision

The four functional contracts are retained:

- PB-1 P-ZETA: `RETAIN_FUNCTION`;
- PB-2 P-ROUTER: `RETAIN_FUNCTION`;
- PB-3 P-ANF: `RETAIN_FUNCTION`;
- PB-4 P-FEISTEL: `RETAIN_FUNCTION`.

The frozen output therefore emits:

`PB5_GATE,READY_FOR_COMPOSED_PREREGISTRATION`

PB-5 must be separately preregistered before combining these functions. The
PB-1..PB-4 results do not reopen the closed PGF2-3B representation-mixing
branch and do not promote any function into SBG, MOR, Delta-KV, context, or the
25B model path without matched downstream evidence.
