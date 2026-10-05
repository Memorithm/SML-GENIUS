# DKV-2a-N6 — terminal-write crosstalk observation

Status: current-stack Development diagnostic under
`SML-DKV-2A-N6-CROSSTALK-PREREGISTRATION.md`.

## Execution identity

- protocol merged before execution in PR #174;
- protocol merge: `5fad0dd25bf47056e3a759540a652200efdfa4d7`;
- executed code head: `42d6ffe075fd0f5879d508666b8f612fd314582b`;
- GitHub Actions run: `37280066984`;
- job: `111665824121`;
- runner: `tarek-scirust-arm64-01`;
- Rust 1.89.0;
- two release-mode executions were byte-identical;
- retained output: `research/dkv-2a-n6/CURRENT-CROSSTALK.csv`;
- manifest: `research/dkv-2a-n6/MANIFEST.json`.

No projector or memory implementation changed. Every arm uses current
`ActiveOnly` plus the N3B outer PageId(5)-derived seed.

## SELF control

Every terminal self-write is recovered exactly and clearly when stored alone:

- all K in {64,256,1024};
- all terminal chain lengths L0/L1/L2/L3;
- 100% exact+clear recall.

The failure is therefore produced by interference from other writes, not by an
inability of the isolated write to encode/retrieve itself.

## Same-chain writes are highly correlated

At K=1024, terminal-key geometry is:

| terminal class | same-chain mean |same cosine| | cross-chain mean |cosine| |
|---|---:|---:|
| L1 | 0.680492 | 0.067296 |
| L2 | 0.675859 | 0.068820 |
| L3 | 0.683404 | 0.054983 |

No exact projected-key equality occurs. The problem is high correlation, not
literal key aliasing.

The same pattern persists at smaller widths. For example at K=256 the
same-chain absolute means are 0.673, 0.668 and 0.673 for L1/L2/L3, while
cross-chain absolute means remain about 0.067-0.086.

Widening K therefore makes unrelated keys somewhat cleaner but does not remove
the approximately 0.67 same-chain correlation induced by shared M/R.

## Count-matched causal control

At K=1024:

| L | SELF | SELF + same chain | SELF + matched cross-chain | FULL |
|---:|---:|---:|---:|---:|
| 0 | 100% | 100% | 100% | 100% |
| 1 | 100% | 100% | 100% | 100% |
| 2 | 100% | 87.5% | 100% | 81.25% |
| 3 | 100% | 50.0% | 100% | 50.0% |

Each matched cross-chain arm uses exactly L non-self writes, the same number as
the corresponding same-chain arm, with four deterministic replicates per
terminal query.

All matched cross-chain observations at all widths remain 100% exact+clear.

This rules out write count alone as an explanation for the low-load L2/L3
failure on this fixture.

## FULL failures are reproduced by same-chain interference

At K=1024:

- FULL L2/L3 terminal failures: 11;
- those same terminal queries also failing under SELF_SAME: 10;
- reproduction: 90.9%.

At K=64 and K=256, 9 of the same 11 FULL failures are reproduced by SELF_SAME.

Cross-chain/global interference can therefore contribute an additional small
residual, particularly at L2, but same-chain writes are sufficient to reproduce
most of the retained capability failure.

## Width does not solve the correlated interference

SELF_SAME L3 exact+clear recall:

- K64: 9/16 = 56.25%;
- K256: 8/16 = 50.00%;
- K1024: 8/16 = 50.00%.

SELF_SAME L2:

- K64: 14/16 = 87.50%;
- K256: 15/16 = 93.75%;
- K1024: 14/16 = 87.50%.

The non-monotonicity is consistent with N4/N5: key width is not the missing
control variable once correlated states share M/R.

## FULL control reproduces N5 exactly

At K=1024 FULL terminal exact+clear counts are:

- L0: 16/16;
- L1: 16/16;
- L2: 13/16;
- L3: 8/16.

The N6 fixture therefore reproduces the N5/N4 capability ceiling exactly before
interpreting its subset arms.

## Frozen hypotheses

All N6 hypotheses hold:

- H1 SELF perfect — HOLDS;
- H2 same-chain geometry separation — HOLDS;
- H3 count-matched same-chain damage — HOLDS;
- H4 >=80% FULL failure reproduction by SELF_SAME — HOLDS;
- H5 widening K does not eliminate L3 same-chain interference — HOLDS;
- H6 FULL control reproduces N5 — HOLDS.

## Mechanistic conclusion

The dominant remaining low-load failure is now localized one level further:

**correlated writes from the same chain interfere because their projected keys
remain strongly aligned when M/R are shared and V changes.**

This explains why:

- PageSeeded solved inter-page isolation but not within-page chains;
- larger K did not remove L2/L3 failures;
- the owner-supplied value-weight sweep produced only small gains.

The next candidate should therefore alter **within-page binding geometry**, not
controller thresholds or raw key width.

A useful next Development hypothesis is to condition the projection domain on
the current V identity (or another explicit transition/source identity) so that
states sharing M/R do not inherit the same projection family. Such a candidate
must be tested as an outer-domain experiment first, because it can trade away
near-state generalization and become brittle to V errors.

Before any core API, the existing multi-expert, unbalanced-page and DKV-3a
record/subject/provenance gates remain mandatory.

No context/model promotion follows.
