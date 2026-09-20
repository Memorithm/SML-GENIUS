# SML-0.5 Forge topology search and destination requalification

Status: executable cross-repository development contract. No model-quality, scaling, hardware-performance or novelty claim is made.

## Qualified Forge source

SML-0.5 is paired with the Forge contract:

- repository: `Memorithm/Forge`;
- contract: `sml-topology/v1`;
- Forge PR: #42;
- Forge qualified exact head: `b8774b508ea2163c7aeafd7aed4dffc293c06291`;
- Forge squash merge: `6ea4844311a5246bfe57fc3795abe3579b643cf0`;
- Forge CI: general CI run 217 passed on the exact head;
- Forge dedicated SML topology workflow run 2 passed on the exact head, including the isolated campaign/holdout smoke.

SML does not depend on the Forge crate at runtime.

## Authority boundary

Forge proposes and evaluates bounded topology candidates. It does not authorize SML promotion.

A candidate crosses into SML only as the canonical textual representation:

    sml-topology/v1|g0=LEFT,RIGHT,TABLE|...|out=SIGNAL

SML parses that string independently and rejects:

- wrong contract version;
- unknown fields;
- non-contiguous gate indices;
- non-canonical integers;
- invalid truth-table codes;
- missing/duplicate output;
- wrong gate count;
- invalid, forward or cyclic references;
- invalid output references.

## Full-oracle requalification

Forge deliberately keeps a disjoint final holdout during search. After Forge selection, SML does not trust the reported score as promotion evidence.

`ForgeTopologyCandidateV1::requalify` executes every row of the caller-owned Boolean oracle again inside SML.

For a candidate to qualify for later promotion:

    incorrect_rows == 0

The destination also recomputes from its own implementation:

- reachable gate count;
- learned Boolean truth-table bits;
- wiring metadata bits.

This catches semantic or accounting drift between Forge and SML.

## Reference cross-repository fixture

The first shared fixture is:

    g0 = XOR(input0, input1)
    g1 = AND(g0, input2)
    g2 = unreachable FALSE gate
    output = g1

Canonical Forge representation:

    sml-topology/v1|g0=0,1,6|g1=3,2,8|g2=0,0,0|out=4

SML must independently reproduce:

- 8/8 truth-table rows correct;
- 2 reachable gates;
- 8 learned Boolean bits;
- 15 wiring metadata bits.

The unreachable third gate must not inflate the effective learned capacity.

## Promotion rule

A Forge candidate is only a search result until destination requalification passes on the pinned SML source revision.

A later promotion workflow must retain:

- Forge contract version;
- Forge merge revision;
- candidate canonical representation and hash;
- SML source revision;
- complete destination oracle identity;
- destination requalification report;
- any later runtime or recurrent-depth evidence.

SML-0.5 establishes the boundary, not a production promotion mechanism.
