# PGF2-3B — K=2048 block-local locality-rescue observation

Status: retained negative component result under
`SML-PGF2-3B-BLOCK-LOCAL-PREREGISTRATION.md`.

## Execution identity

- protocol PR: #204;
- protocol merge: `aaee37d5aa38c056df5d7a1bd22fd0414f772c49`;
- implementation PR: #205;
- executed/resynchronized head:
  `85db9739ee8eebe846f490e72b2189ab5a92f3df`;
- implementation merge: `44d16762a4923ab6370622ac86f7e4579581710c`;
- retained GitHub Actions run: `37424429490`;
- job: `112140758553`;
- runner: `tarek-scirust-arm64-01`;
- Rust 1.89.0, target aarch64-unknown-linux-gnu;
- retained output: `research/pgf2-3b/CURRENT-REPLAY.csv`;
- retained SHA-256:
  `75c471f5a80e0048105bdaa80bcb70a13b1980b8468c68facce87c695aefb5f6`;
- manifest: `research/pgf2-3b/MANIFEST.json`.

The same 295-line output was already produced by green run `37422055607`
before the branch was synchronized with the completed N14 work. Its SHA-256 is
identical. Each CI run also executes the release harness twice and requires
byte-identical stdout.

## Exactness and cost

All block-local transforms remain exactly self-inverse and retain zero
full-state collision excess on the frozen controls. H1 holds.

Frozen structural costs are reproduced exactly:

| Arm | logical XOR updates | packed-word updates | state bytes | scratch |
|---|---:|---:|---:|---:|
| PGF2_B64 | 6,144 | 192 | 256 | 0 |
| PGF2_B128 | 7,168 | 208 | 256 | 0 |
| PGF2_B256 | 8,192 | 224 | 256 | 0 |
| PGF2_B512 | 9,216 | 240 | 256 | 0 |
| PGF2_GLOBAL | 11,264 | 272 | 256 | 0 |

Every block-local arm therefore passes the frozen structural-cost reduction
gate H8.

## Single-bit amplification

Block locality bounds the previously global amplification:

| Arm | median output flips | p95 | maximum |
|---|---:|---:|---:|
| PGF2_GLOBAL | 48 | 256 | 2,048 |
| PGF2_B64 | 8 | 32 | 64 |
| PGF2_B128 | 12 | 64 | 128 |
| PGF2_B256 | 16 | 64 | 256 |
| PGF2_B512 | 24 | 128 | 512 |

At least one block-local arm therefore satisfies the bounded-amplification
hypothesis H7. This is a real improvement in locality, but it is not sufficient
for representation selection.

## Structured-local geometry

The required geometry rescue does not occur.

On STRUCTURED_LOCAL the global PGF2 nearest-wrong medians are:

- class: 12 bits;
- route: 6 bits.

Block-local medians are even smaller:

| Arm | class median | route median |
|---|---:|---:|
| PGF2_B64 | 2 | 4 |
| PGF2_B128 | 2 | 2 |
| PGF2_B256 | 3 | 2 |
| PGF2_B512 | 4 | 3 |

The H2 requirement was >=90% of the best matched Hamming-isometric control
median for both class and route. No width comes close. H2 is REFUTED.

Because the same H2-selected width is required for H3 and H4, the block-local
family also fails the preregistered low-noise and moderate-noise joint
selection path.

## What locality does improve

At h=64 on STRUCTURED_LOCAL, PGF2_GLOBAL achieves:

- class retrieval: 42.38%;
- route accuracy: 57.03%.

PGF2_B64 improves those cells to:

- class retrieval: 52.93%;
- route accuracy: 81.25%.

Thus block locality does address part of the global error-amplification
mechanism, and aggregate H5 is recorded as HOLDS.

However, the same B64 candidate still fails H2, H3 and H4. Improvement over a
failed reference is not enough to beat the matched controls.

## Random-control sanity

BALANCED_RANDOM remains well behaved. H6 holds.

For example, PGF2_B64 preserves 100% class and route accuracy through h=64 on
the frozen random panel, while its clean full-state collision excess remains
zero. This confirms that the implementation is functioning; it does not rescue
the structured semantic objective.

## Frozen candidate gates

The preregistered per-candidate results are:

- PGF2_B64: H2 REFUTED, H3 REFUTED, H4 REFUTED, H5 HOLDS,
  H7 HOLDS, H8 HOLDS -> ALL REFUTED;
- PGF2_B128: H2/H3/H4/H5 REFUTED, H7/H8 HOLDS -> ALL REFUTED;
- PGF2_B256: H2/H3/H4/H5 REFUTED, H7/H8 HOLDS -> ALL REFUTED;
- PGF2_B512: H2/H3/H4/H5/H7 REFUTED, H8 HOLDS -> ALL REFUTED.

Aggregate hypotheses:

- H1 exactness — HOLDS;
- H2 structured geometry rescue — REFUTED;
- H3 low-noise parity — REFUTED;
- H4 moderate-noise viability — REFUTED;
- H5 material improvement over global PGF2 — HOLDS;
- H6 random-control sanity — HOLDS;
- H7 bounded amplification — HOLDS;
- H8 cost reduction — HOLDS.

## Decision

The frozen stop rule fires:

`PGF2 representation mixing = CLOSE`

`arm = NONE`

`PGF2-4 MOR/Delta-KV handoff = BLOCKED`

No global or block-local Pascal transform enters the default SBG
representation path from this research branch.

This closes the current Pascal-as-general-SBG-representation-mixer objective.
A future Pascal experiment requires a different prospectively declared
objective, for example a task explicitly aligned with subset/parity semantics.
It cannot reopen PGF2-4 by changing block size, coordinate ordering or
orientation after this observation.

No MOR, Delta-KV, context or model promotion follows.
