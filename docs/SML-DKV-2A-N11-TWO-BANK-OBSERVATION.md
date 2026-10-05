# DKV-2a-N11 — two-bank PRIMARY / ORDERED resolver observation

Status: current-stack Development experiment under
`SML-DKV-2A-N11-TWO-BANK-PREREGISTRATION.md`.

## Execution identity

- protocol merged before execution in PR #186;
- protocol merge: `c9b91294c68f7cdec5e7103e526127a58f0601a1`;
- executed scientific head: `72b2296f37241f85a42233468957f3ecd7293459`;
- GitHub Actions run: `37338234850`;
- job: `111858364930`;
- runner: `tarek-scirust-arm64-01`;
- Rust 1.89.0;
- two release-mode executions were byte-identical;
- retained output: `research/dkv-2a-n11/CURRENT-REPLAY.csv`;
- manifest: `research/dkv-2a-n11/MANIFEST.json`.

No core projector, DeltaKvMemory or MOR implementation changed.

## Result

N11 separates the two incompatible N9/N10 route semantics into independent
banks:

- **PRIMARY**: q=3 PRIMARY_ANCHORED key for transparent secondary-route
  continuity;
- **ORDERED**: q=3 ORDERED_ROUTE key for explicit route-specific override.

An exact bounded `(semantic_record_id, ordered_route)` directory chooses the
bank before retrieval. Retrieval strength, cosine, agreement and successful
associative reads never participate in bank selection.

All ten frozen hypotheses hold.

## P1 — secondary continuity is restored without speculative override

At K=1024, facts written only to PRIMARY under R2 are recovered exactly+clear
under all three query routes:

| Query | Recall | Selected bank |
|---|---:|---|
| R2 | 256/256 = 100% | PRIMARY 256/256 |
| R3 | 256/256 = 100% | PRIMARY 256/256 |
| R4 | 256/256 = 100% | PRIMARY 256/256 |

The counterfactual ORDERED read under R3/R4 is 0/256 in both cases. The
resolver therefore succeeds because the explicit directory preserves the shared
PRIMARY semantic path; it does not infer continuity from a weak ORDERED read.

At K=256 the resolved recall remains 99.2–99.6%.

H2 holds.

## P2 — explicit secondary overrides coexist with continuity

For the same semantic record/state:

- base A is written to PRIMARY/R2;
- independent override B is written to ORDERED/R3;
- only `(record_id,R3)` is bound in the directory.

At K=1024:

| Query | Expected | Recall | Bank |
|---|---|---:|---|
| R2 | base A | 256/256 | PRIMARY |
| R3 | override B | 256/256 | ORDERED |
| R4 | base A | 256/256 | PRIMARY |

Thus the N10 conflict/continuity tradeoff disappears once the two semantic
roles are represented separately rather than interpolated inside one key.

H3 holds.

## P3 — primary-switch override remains isolated

At K=1024, R2 base retrieval and R6 route-specific override retrieval are both
256/256 exact+clear. The resolver selects PRIMARY for R2 and ORDERED for R6.

H4 holds.

## P4 — metadata guard and remaining identity boundary

For each K, 768 negative metadata probes were executed.

At K=1024:

- wrong record_id under R3: no false override;
- correct record_id under unbound R4: no false override;
- total false override selections for non-bound pairs: **0**;
- false acceptances caused by non-bound pairs: **0**.

The deliberately different-state control is more informative:

- the exact bound `(record_id,R3)` still selects ORDERED when paired with a
  different semantic state;
- this happens in 256/256 such probes by construction;
- **wrong-state false acceptances remain 0/256** on the frozen panel.

This is not sufficient as a production identity guarantee. It shows that N11's
directory semantics are exact with respect to record+route, while semantic
state/subject binding still belongs to DKV-3a. The next qualification must bind
the override directory to the real semantic record/subject/provenance contract
rather than rely on the synthetic record id alone.

H5 holds under the preregistered non-bound-pair definition.

## P5 — q=3 recurrent capability is preserved

With an empty override directory and fixed R2, N11 resolves only PRIMARY reads
and reproduces the retained multi-expert q=3 capability.

K=1024:

| Load | F6 | ORACLE |
|---:|---:|---:|
| G=4 | 64/64 = 100% | 64/64 = 100% |
| G=16 | 238/256 = 92.97% | 238/256 = 92.97% |

K=1024/G4 teacher-forced advancing recall:

- h0: 48/48;
- h1: 32/32;
- h2: 16/16.

K=1024/G16:

- h0: 177/192 = 92.19%;
- h1: 119/128 = 92.97%;
- h2: 64/64 = 100%.

H6 holds.

## P6 — route-specific recurrent transition override

For selected L2/L3 advancing transitions at G=4:

- R2 base target: 32/32 exact+clear;
- R3 override target: 32/32 exact+clear.

The resolver selects PRIMARY for every base query and ORDERED for every override
query.

H7 holds.

## P7 — local V robustness is retained

At K=1024, one-bit V perturbations under fixed R2 and an empty override
directory give:

- mean |cosine| = 0.244710;
- exact+clear retrieval = 28/96 = 29.17%.

This remains above the frozen 20% robustness gate and is consistent with the
selected N8 q=3 behavior.

H8 holds.

At K=256 the same panel gives 19/96 = 19.79%; N11 did not preregister a K256
robustness gate.

## P8 — one-bank-read resolution

Across the complete retained panel:

- directory probes: 10,752;
- resolved DeltaKvMemory reads: 10,752;
- PRIMARY reads: 9,152;
- ORDERED reads: 1,600;
- explicitly labelled counterfactual diagnostics: 1,024 reads.

Therefore every actual resolved query performs exactly one bank read after the
directory decision.

The counterfactual reads are diagnostic controls and are excluded from resolver
cost.

H9 holds.

Bank selection is structurally a pure exact-directory-membership decision.
There is no magnitude/agreement/fallback path.

H10 holds.

## Frozen hypotheses

- H1 endpoint compatibility — HOLDS;
- H2 secondary continuity — HOLDS;
- H3 secondary override — HOLDS;
- H4 primary-switch override — HOLDS;
- H5 metadata guard — HOLDS;
- H6 recurrent capability — HOLDS;
- H7 route-transition override — HOLDS;
- H8 Hamming-1 robustness — HOLDS;
- H9 one-bank-read invariant — HOLDS;
- H10 explicit-directory-only selection — HOLDS.

Selection:

**two-bank resolver = RETAIN as Development memory-composition semantics.**

## Mechanistic conclusion

N11 resolves the N9/N10 contradiction by refusing to encode two different
semantic roles into one associative address.

The retained Development interpretation is:

1. PRIMARY is the shared knowledge path inside a primary MOR domain;
2. ORDERED is an explicit route-specific override path;
3. exact identity metadata decides whether an override exists;
4. associative strength never decides source presence;
5. a query reads only the selected bank.

The remaining question is no longer key interpolation. It is authoritative
identity binding.

The next model-core gate should therefore replay this two-bank resolver against
the existing DKV-3a exact semantic-record/subject/provenance machinery,
including Missing/Conflict/Unverified boundaries and wrong-state/foreign-subject
controls.

No core API, context or model promotion follows from N11 alone.
