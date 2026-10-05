# DKV-2a-N3A — hypothesis-derived ambiguity observation

Status: Development replay under
`SML-DKV-2A-N3A-HYPOTHESIS-AMBIGUITY-REPLAY.md`.

## Execution identity

Owner-supplied input:

- `sml-dkv-0006-dkv-4f-4g-4h.diff`;
- 69,014 bytes;
- 1,363 lines;
- SHA-256 `8b201dbde1a5655fc049536c1457f5f6ed3a352eb0001c7d38ffe803344a9664`.

Current-stack replay:

- protocol merged before implementation in PR #168;
- protocol merge: `4023eb817d873d431b4f8593fc45a800d6641f7d`;
- executed code head: `eb0938627c5f02eb4844b43b202911a76fea66a5`;
- GitHub Actions run: `37268594839`;
- job: `111630572185`;
- runner: `tarek-scirust-arm64-01`;
- Rust 1.89.0;
- two release-mode executions were byte-identical;
- retained output: `research/dkv-2a-n3/N3A.csv`.

The source diff already contained Rust 1.97 outcomes. This is a current-library
replay, not an independent blind replication.

## Result

The current `derive_ambiguity` implementation reproduces the supplied
conditional crossover.

At K=1024:

| H,m | p | mean A bits | Bipolar | ActiveOnly |
|---|---:|---:|---:|---:|
| 3,1 | 1% | 1.88 | 31.25% | 97.66% |
| 3,1 | 5% | 9.39 | 79.30% | 99.22% |
| 3,1 | 10% | 17.26 | 97.27% | 98.83% |
| 3,1 | 30% | 40.05 | 98.83% | 80.08% |
| 5,1 | 1% | 2.98 | 38.28% | 99.22% |
| 5,1 | 5% | 14.08 | 94.53% | 99.61% |
| 5,1 | 10% | 26.16 | 99.22% | 98.05% |
| 5,1 | 20% | 43.11 | 100% | 82.03% |
| 5,2 | 1% | 0.05 | 23.44% | 98.83% |
| 5,2 | 10% | 5.16 | 50.39% | 97.27% |
| 5,2 | 20% | 16.30 | 97.27% | 98.44% |
| 5,2 | 40% | 36.91 | 100% | 89.45% |

The largest tested noise level whose **mean** A weight remains <=16 is:

- H=3,m=1: 5%;
- H=5,m=1: 5%;
- H=5,m=2: 10%.

All 20 current cells with mean A weight <=16 satisfy the frozen ActiveOnly
no-regression margin. All 10 cells with mean A weight >=40 favor Bipolar by
more than the frozen dense-regime margin.

## Frozen hypotheses

- H1 monotone A weight — HOLDS;
- H2 ActiveOnly valid-range margin — HOLDS;
- H3 dense-range Bipolar advantage — HOLDS;
- H4 low-noise separation — HOLDS;
- H5 no ActiveOnly recall cliff inside the frozen valid range — HOLDS.

## Important boundary

This does **not** measure representative recurrent SBG states.

The independent bit-flip model is only a map from one assumed hypothesis-noise
process into the repository's real ambiguity producer. It demonstrates that
the N1/N2 crossover survives `derive_ambiguity`; it does not demonstrate that
actual SBG hypotheses have the same disagreement structure.

Mean A weight alone is also not enough to promote a mode. For example, the
H=5,m=1,p=5% K=1024 cell has mean A weight 14.08 but 20.3% of states exceed 16
active ambiguity bits, while ActiveOnly still succeeds on this synthetic panel.
Representative measurement should therefore retain at least the mean, upper
tail/share above the candidate boundary, and temporal transitions of A across
recurrent steps.

## Decision

ActiveOnly remains an opt-in Development candidate.

N3A narrows the unresolved question from arbitrary synthetic A density to a
measurable runtime quantity: the distribution and temporal stability of
`ambiguity.count_ones()` produced by representative SBG hypothesis sets.

No context/model promotion follows.
