# Owner-supplied DKV-4n patch — current-stack input review

Input reviewed 2026-10-05:

- uploaded file: `sml-dkv-0009-dkv-4n.diff`;
- SHA-256: `40c51c62b34cf609912f378f51df3cddc357dce1796b42270e217a82e5bd77a6`;
- 577 lines;
- 5 paths changed;
- supplied delta: 523 insertions and 1 deletion.

This note preserves the useful negative result without applying the historical
projector API to the current repository state.

## Supplied experiment

DKV-4n follows the supplied DKV-4m result. Instead of dropping M/R entirely, it
adds an opt-in `with_context_divisor(D)` that keeps approximately one in D
meaning-mask and routing coordinates, with D in {1,2,4,8,16}. The supplied
harness compares those settings with the V-only endpoint at K in {256,1024},
loads G in {4,16}, and 16 deterministic seeds.

The preregistered question is whether a partial context density can preserve
both:

1. low same-chain key correlation / multi-hop recall; and
2. separation of twins with identical V but different M/R.

## Supplied K=1024 result

At G=4:

| D | same-chain cosine | L>=2 fully strict | twins strict |
|---:|---:|---:|---:|
| 1 | 0.677 | 0/32 | 96.1% |
| 2 | 0.509 | 4/32 | 63.3% |
| 4 | 0.347 | 10/32 | 50.0% |
| 8 | 0.209 | 23/32 | 48.4% |
| 16 | 0.135 | 26/32 | 47.7% |
| V only | 0.033 | 22/32 | 48.4% |

At G=16, no density setting recovers the higher-load multi-hop gate: the best
L>=2 fully-strict result reported is 5/128 at D=16, while twin strict is 26.4%.

The supplied preregistered outcomes are:

- H1, same-chain cosine non-increasing with D: HOLDS;
- H2, some D reaches >=85% chain and >=85% twin performance at G=4: REFUTED;
- H3, some D reaches >=70% for both at G=16: REFUTED;
- H4, twin strict non-increasing with D: HOLDS.

The negative result is the useful part: within this hashed sign-quantized
single-address family, thinning M/R moves smoothly between two failure modes
rather than revealing a joint operating point.

## Current-stack interpretation

Do not import `with_context_divisor` into the current core.

The current repository already addresses the same semantic conflict at a
stronger architectural level:

- N6 localized same-chain crosstalk from shared contextual geometry;
- N7/N8 measured the value-domain/continuity trade-off and retained q=3 at fixed
  final width rather than deleting semantic fields;
- N9/N10 showed that continuity and route-conflict isolation do not share a
  satisfactory single-key interpolation;
- N11 therefore split the roles into PRIMARY and ORDERED banks;
- N12 bound that resolver to exact source/record/subject verification;
- N13B retained the composition under balanced, empirical-skew and heavy-skew
  page populations, with primary-page isolation by default and explicit
  replicated sharing when cross-primary continuity is intended.

That two-bank/write-rule structure is already the "outside the address"
alternative suggested by the supplied DKV-4n observation.

## Why the API is not ported

1. The supplied success criteria H2/H3 are refuted.
2. The proposed density sweep changes the number of M/R bucket accumulations
   and does not provide matched physical work or resource measurements.
3. The supplied observation explicitly labels the suspected high-load capacity
   explanation as unmeasured.
4. The current q=3/two-bank candidate has later, stronger source-identity,
   route-semantics and page-skew qualification.
5. The current declared gate is physical latency/RSS/traffic/energy
   qualification of the retained composition, not another monolithic-key sweep.

## Retained regression idea

The twin control remains useful conceptually: equal V with different M/R must
not silently become one semantic identity. Any future optimized two-bank/core
implementation should preserve that distinction, but a new scientific result
requires a prospective current-stack protocol rather than replaying this old
projector API.

## Non-claims

This review does not independently reproduce the supplied Rust 1.97 run or
authenticate its preregistration chronology. It compares the supplied diff with
the retained current-stack evidence and records the integration decision.
