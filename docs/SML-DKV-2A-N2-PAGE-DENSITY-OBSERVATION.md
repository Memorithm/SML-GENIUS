# DKV-2a-N2 — page discrimination and ambiguity-density observation

Status: current-stack Development replay under
`SML-DKV-2A-N2-PAGE-DENSITY-REPLAY.md`.

## Execution identity

Owner-supplied input:

- `sml-dkv-0005-dkv-4e.patch`;
- 42,511 bytes;
- 761 lines;
- SHA-256 `6d8d60b570f7a8fbefa0089b01eb4633625f08bbd23aa18aaef4e71fb5f5d71f`.

Current-stack replay:

- frozen protocol commit: `e0609cef8c40de1b37bea6d517cf10efa82f90ee`;
- executed code head: `d772d5483377149ae5b5f21618ee4a2b859f35dc`;
- GitHub Actions run: `37236717769`;
- job: `111537172347`;
- runner: `tarek-scirust-arm64-01`;
- Rust: 1.89.0;
- two release-mode executions were byte-identical;
- machine-readable output: `research/dkv-2a-n2/CURRENT-REPLAY.csv`;
- manifest: `research/dkv-2a-n2/MANIFEST.json`.

The source patch already contained Rust 1.97 outcomes. This replay therefore
tests reproducibility on the current library/API; it is not an independent blind
replication.

## Contracts

All **51,232** current-library contract checks passed.

- legacy constructor == explicit Bipolar complete evidence;
- repeated ActiveOnly projection is deterministic;
- all-ones A produces the same key in Bipolar and ActiveOnly;
- every SemanticState<64> projection reads 256 semantic coordinates;
- one-page Bipolar uses 257 bucket accumulations;
- one-page ActiveOnly uses 193 + popcount(A) accumulations;
- no zero projected key occurred in the frozen zero-A contract panel.

No local copy of the projection hash/bucket/sign implementation participated in
the replay.

## E1 — PageId barely changes retrieval

Strict recall, shown as **own page / wrong page**:

| K | arm | W=8 | W=16 | W=32 |
|---:|---|---:|---:|---:|
| 256 | Bipolar | 57.8 / 57.0% | 27.3 / 27.0% | 11.7 / 12.3% |
| 256 | ActiveOnly | 100 / 100% | 94.1 / 93.0% | 61.1 / 60.2% |
| 1024 | Bipolar | 54.7 / 53.9% | 25.0 / 25.4% | 11.5 / 11.3% |
| 1024 | ActiveOnly | 100 / 100% | 98.4 / 98.4% | 78.7 / 78.3% |

At K=1024/W=8, a wrong PageId retrieves the stored value as successfully as
the original PageId in ActiveOnly: 128/128 in both cases. The default projector
shows the same qualitative weakness.

This is not an ActiveOnly regression. It exposes a separate DKV-2a key-design
property shared by both modes.

With one selected expert, the current projector injects that PageId into one
hashed accumulator. Replacing one page by another can therefore directly change
at most the old and new page buckets before sign quantization. The E1 result
shows that this sparse page perturbation is not an effective memory namespace
under the present weight and key geometry.

## E2 — two values under two pages are not isolated

Same state, two distinct pages, independent values:

| K | arm | W'=4 | W'=8 |
|---:|---|---:|---:|
| 256 | Bipolar | 37.5% | 17.6% |
| 256 | ActiveOnly | 50.0% | 47.3% |
| 1024 | Bipolar | 34.4% | 16.0% |
| 1024 | ActiveOnly | 50.0% | 49.6% |

ActiveOnly reaches approximately one-half recall, consistent with two
page-tagged values interfering rather than becoming cleanly isolated memories.
Bipolar is worse because its zero-A common-vector interference remains layered
on top of the page conflict.

A PageId must therefore not be treated as a strong independent associative key
component at the current DKV-2a settings.

## E3 — density crossover reproduced

K=1024, W=16:

| active A bits | Bipolar strict | ActiveOnly strict | Bipolar cosine | ActiveOnly cosine |
|---:|---:|---:|---:|---:|
| 0 | 25.0% | 98.0% | 0.246236 | -0.000259 |
| 1 | 27.7% | 99.2% | 0.226990 | -0.004987 |
| 2 | 32.8% | 98.8% | 0.218753 | 0.002794 |
| 4 | 40.6% | 98.8% | 0.194218 | 0.009449 |
| 8 | 71.1% | 99.2% | 0.147353 | 0.014558 |
| 16 | 97.3% | 98.8% | 0.069296 | 0.025136 |
| 32 | 99.6% | 95.7% | 0.002289 | 0.069094 |
| 64 | 22.7% | 22.7% | 0.238654 | 0.238654 |

The current Rust 1.89 result reproduces the supplied Rust 1.97 table at the
reported precision.

ActiveOnly is not worse on the tested synthetic grid through 16 active A bits,
then loses at 32. At 64 all ambiguity bits are constant true and both encodings
become identical; both are poor. This supports the narrower mechanism claim:
**state-independent field mass is harmful, while a genuinely state-varying dense
field can be useful information.**

It does not justify a runtime rule such as "switch at popcount 16". Real SBG
states have not been measured, and changing encoding by instantaneous density
could itself change key continuity across recurrent/write/read states.

## Imported hypotheses

All five supplied hypotheses reproduce on Rust 1.89:

- H1 page-diversity no-regression — HOLDS;
- H2 PageId weak discrimination — HOLDS;
- H3 page conflict unresolved — HOLDS;
- H4 no E2 ActiveOnly regression — HOLDS;
- H5 ambiguity-density crossover — HOLDS.

## Decision

**ActiveOnly remains a Development candidate, not the default.**

N2 sharpens the next work into two independent problems:

1. measure the ambiguity-density distribution and temporal stability of
   representative SBG recurrent states before choosing an A encoding;
2. qualify a stronger PageId/MOR discriminator for page-sensitive Delta-KV
   memory, because the current single-bucket page injection is not an isolating
   namespace.

The second problem applies equally to Bipolar and ActiveOnly. It should be
evaluated with E1/E2 as primary controls plus the existing DKV alias/provenance
gates. Increasing confidence or accepting answers is not part of this
mechanism.

No context/model promotion follows.
