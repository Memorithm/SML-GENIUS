# DKV-4c — semantic-impact controller observation

Status: Development observation under the frozen protocol in
`SML-DKV-4C-SEMANTIC-IMPACT-PREREGISTRATION.md`.

Protocol commit: `286b4b9d847cc08bc1a445a0dcb7b3e623437fce`.
Executed code: `0efd2287965c6a8039b07dfe2b8603f5e5c3d879`.
Retained-evidence head: `6bdcbdce81fd3af1fdb1e31d8dbad6627baaffa9`.
RemoteOps run: `37198054863`.
Host: `debian`, x86_64. Rust: 1.89.0.
Rows: 2,048. Two explicit runs were byte-identical.
Output SHA-256: `a57076feb598c6923981d3e79c871278093ea5ce4eea4ba506224e476e27cf5f`.

The retained evidence is in `research/dkv-4c/`. No threshold, seed, task,
write order or classification rule was changed after observation. DKV-4a and
the prior DKV-4b evidence remain unchanged.

## 1. Written-state result

There are 128 written-state queries at each K.

| metric | K=64 | K=256 |
|---|---:|---:|
| correct, unchanged DKV-4a | 125/128 | 128/128 |
| correct, DKV-4c semantic-impact | 125/128 | 128/128 |
| correct, fixed depth 1 | 125/128 | 128/128 |
| correct, fixed depth 4 | 125/128 | 128/128 |
| correct, SBG stability-only | 125/128 | 128/128 |
| DKV-4a StopCandidate | 11/128 | 10/128 |
| DKV-4c StopCandidate | 127/128 | 128/128 |
| DKV-4a Abstain | 117/128 | 118/128 |
| DKV-4c Abstain | 1/128 | 0/128 |
| DKV-4a recurrent cycles | 490 | 491 |
| DKV-4c recurrent cycles | 251 | 250 |
| SBG stability-only cycles | 251 | 250 |
| DKV-4a read MACs | 62,720 | 251,392 |
| DKV-4c read MACs | 32,128 | 128,000 |

DKV-4c preserves every DKV-4a written-query correctness outcome in the retained
256 paired written rows. It is never deeper than DKV-4a on any of those rows.

Relative to DKV-4a, DKV-4c removes 239/490 recurrent cycles at K=64 (48.78%)
and 241/491 at K=256 (49.08%). Read-MAC reductions are the same proportions
because each recurrent cycle performs one fixed-width DKV-2b read for a given K.

The remaining K=64 abstention is seed 7, fact 5. That row is correct under all
listed arms, and both DKV-4c and the pre-existing stability-only control reach
depth 4. Therefore this residual abstention is not introduced by removing the
absolute-correction signal.

## 2. DKV-4c collapses to the simpler stability control on this panel

At both key dimensions, DKV-4c has exactly the same aggregate written
correctness and recurrent-cycle total as the existing SBG stability-only arm:
125/128 and 251 cycles at K=64; 128/128 and 250 cycles at K=256.

That equality is important negative evidence. The panel fixes MOR to one page
with zero declared disagreement, so it does not exercise a useful MOR-escalation
advantage. The semantic-impact controller therefore demonstrates that the
absolute Q12 residual was unnecessary on this fixture, but it does **not**
demonstrate superiority over the simpler stability-only policy.

The extra DKV-4c value remains architectural at this stage: provenance is a
first-class preflight gate and MOR escalation/disagreement are typed inputs that
can request refinement in future panels. This experiment does not qualify those
extra paths as beneficial.

## 3. Unknown-state boundary is preserved

For every one of the 896 Missing-provenance queries per K, DKV-4c executes zero
matrix reads and produces zero StopCandidate decisions.

For the paired Unverified diagnostic, DKV-4c produces zero StopCandidate
decisions. Because Unverified provenance is deliberately fail-closed, each
diagnostic query consumes the full four-cycle budget before abstaining:
512 aggregate cycles in each 128-query unknown family.

This is a safety boundary, not an intrinsic unknown-state detector. The prior
DKV-4b latent-resolution observation remains controlling for the matrix itself:
Delta-KV numeric ambiguity did not identify unknown states reliably.

## 4. Frozen hypotheses

- **H1 holds.** DKV-4c uses no more recurrent cycles than DKV-4a at either
  dimension, and no retained written row is deeper.
- **H2 holds.** DKV-4c preserves DKV-4a final written-state correctness on all
  256 paired written rows.
- **H3 holds.** Missing provenance executes zero matrix reads.
- **H4 holds.** Unverified provenance never produces StopCandidate.
- **H5 holds as a pre-execution conformance test.** On the frozen positive-sign
  Q12 fixture where the Boolean bit is already correct but analog residual
  remains above 128, unchanged DKV-4a exhausts its budget while DKV-4c stops
  after the first stable semantic cycle. H5 is a unit-level mechanism check,
  not an independent task-quality result.

## 5. K=64 versus K=256

The dimension result from DKV-4b is unchanged: K=256 is correct on 128/128
written fixtures versus 125/128 at K=64, but each DKV read and update has four
times as many key-coordinate MAC terms. DKV-4c does not compensate for that
per-cycle dimension cost; it simply avoids the unnecessary extra cycles that
DKV-4a introduced at both K.

No optimal K follows from this small synthetic panel.

## Decision

The absolute-correction refinement rule in DKV-4a remains rejected for adaptive
compute. DKV-4c is the cleaner controller candidate because it preserves the
fail-closed provenance contract and removes the measured over-refinement.

However, **DKV-4c is not promoted as superior to SBG stability-only**. On this
panel their written correctness and cycle counts are identical. Before any
stronger adaptive-compute claim, DKV-4c needs a separately frozen panel with
non-trivial MOR disagreement/escalation and representative learned states where
an additional refinement signal can demonstrate attributable benefit.

DKV-4 completion therefore establishes a bounded control-plane reference and
negative/positive development evidence; it does not establish model-quality,
latency, long-context or architecture-superiority claims.

**Scientific/model promotion remains BLOCKED.**
