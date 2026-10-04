# DKV-2a-N1 — ambiguity-neutral projection candidate preregistration

Status: frozen before implementation or current-stack execution.

## Input and motivation

Owner-supplied input: `sml-dkv-0003-dkv-4b-4c.patch`.

- bytes: 42,281;
- lines: 829;
- SHA-256: `b9bd7cdf20b0ea9de546b837ddece803051acae725b77909809370e78daf88b6`.

The supplied patch contains two deterministic Rust probes named DKV-4b/4c in
the source input. Those names are historical input labels only: current
SML-GENIUS already uses the DKV-4 namespace for the adaptive-compute programme,
so this follow-up is assigned to the projection layer as **DKV-2a-N1**.

The useful observation is narrower than the supplied headline. Current
`DeltaKvSemanticProjector` maps every Boolean coordinate through bipolar
`false -> -1`, `true -> +1` accumulation. Therefore an ambiguity field that
is identically false across many states is not neutral: it contributes the same
hashed vector to every projected key. The current implementation confirms this
mechanism directly in source: V, M, A and R are all folded with the same bipolar
rule.

The supplied patch reports, on 64-bit synthetic states and Rust 1.97, that
zero-ambiguity states exhibit a mean independent-key cosine near 0.25 and poor
recall under load, while replacing A with random state-dependent bits removes
most of that correlation and substantially improves load recall. These numbers
are **input observations**, not yet current Rust 1.89 evidence and not a reason
to promote random ambiguity.

## Candidate

Keep the qualified DKV-2a projector unchanged by default.

Add one separately selectable research encoding for the ambiguity field:

- V, M and R retain the existing bipolar projection exactly;
- MOR accumulation retains the existing rule exactly;
- A=true contributes the existing positive signed hash contribution;
- A=false contributes no bucket accumulation in the candidate arm;
- the implementation must still read/count all A coordinates so work accounting
  remains explicit;
- no source/provenance data enters the key;
- no key or agreement result becomes a presence/correctness signal.

This candidate is called **active-only ambiguity**. It treats absence of
ambiguity as no ambiguity contribution, not as a negative semantic feature.
This is a hypothesis, not an architectural correction.

The existing `DeltaKvSemanticProjector::new` constructor and its output must
remain byte-for-byte/deterministically equivalent to the current bipolar
behavior.

## Current-stack conformance gate

Before any capacity experiment, implementation tests must establish:

1. legacy constructor == explicit bipolar constructor for key, selected experts,
   provenance clone, counters and transient-key bits;
2. zero-A active-only projection still reads all 4*BITS semantic coordinates;
3. zero-A active-only projection performs exactly 3*BITS semantic bucket
   accumulations plus the existing MOR accumulations;
4. each true A coordinate adds exactly one ambiguity bucket accumulation;
5. zero key, duplicate MOR, provenance and overflow failures remain fail-closed;
6. no floating-point dependency is introduced into the core.

## Frozen Development panel

After conformance, compare **bipolar** versus **active-only ambiguity** on the
same deterministic state/value streams.

Grid:

- Rust 1.89;
- semantic width BITS=64 for continuity with the supplied diagnostic;
- key widths K in {64, 256, 1024};
- write loads W in {1, 8, 16, 32, 64};
- at least 16 deterministic seeds;
- one fixed native MOR page for the geometry-isolation panel;
- full-rate Q12 writes;
- A distributions:
  - ZERO: every ambiguity bit false;
  - SPARSE: independently generated sparse ambiguity with a frozen density;
  - DENSE_CONTROL: balanced/random ambiguity, mechanism control only.

Report:

- exact strict recall of written values;
- sign-only recall;
- mean wrong value coordinates;
- signed key dot products, squared norms and derived cosine in the research
  harness only;
- projection counters;
- matrix read/update work;
- exact logical matrix bits.

The supplied Rust 1.97 result is not used as an acceptance target. Its numbers
may be compared only after the current-stack output is frozen.

## Required aliasing and retrieval controls

A capacity improvement is insufficient. If active-only ambiguity improves the
ZERO/SPARSE load panel, it must then face:

- DKV-3a alias/near-miss classes;
- exact semantic-record / subject binding from current `sml-store`;
- missing/conflict/unverified provenance boundaries;
- matched exact and Hamming-neighbour retrieval controls;
- projection-agreement arms only in a separate factorial comparison.

The active-only key may generalize differently from the qualified key. Lower
key correlation is not automatically lower false acceptance.

## Frozen hypotheses

H1 — compatibility: explicit bipolar and the legacy constructor are identical.

H2 — mechanism: with ZERO ambiguity, active-only removes all ambiguity bucket
accumulations while preserving semantic-coordinate reads.

H3 — geometry: on ZERO ambiguity, active-only reduces mean independent-key
cosine versus bipolar at every K.

H4 — capacity: on ZERO ambiguity at W=16, active-only strict recall is greater
than bipolar at K=256 and K=1024.

H5 — finite capacity: active-only is not assumed to solve overload; all W=32/64
results are retained even when poor.

H6 — sparse realism: any advantage must be remeasured on SPARSE ambiguity; a
gain confined to DENSE_CONTROL does not justify promotion.

H7 — safety: no result changes source authority, semantic-record identity or
the DKV-4 acceptance boundary.

Failed hypotheses remain failed. No threshold, density, grid or key width may be
changed after first execution to recover a desired result.

## Promotion boundary

Even if the candidate improves key geometry and written-fact recall, it remains
research-only until the aliasing/false-acceptance and representative-state gates
also pass at matched resource cost. It cannot bypass current raw SHA-256
verification, subject history, applicability, calibration or external-action
authorization.

No model/context promotion follows from this preregistration.
