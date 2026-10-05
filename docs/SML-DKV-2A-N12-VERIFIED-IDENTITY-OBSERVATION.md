# DKV-2a-N12 — verified identity/provenance observation

Status: current-stack Development qualification under
`SML-DKV-2A-N12-VERIFIED-IDENTITY-PREREGISTRATION.md`.

## Execution identity

The scientific execution was re-run after tightening the H6 harness to enforce
the preregistered intentionally-wrong candidate control explicitly.

- protocol PR: #188;
- parent execution PR: #189;
- corrected retention/execution PR: #190;
- corrected executed head: `50081998223e44f671cbfedcc1284ea677be701a`;
- GitHub Actions run: `37360161482`;
- job: `111932481964`;
- Rust 1.89.0;
- two release-mode executions were byte-identical;
- retained output: `research/dkv-2a-n12/CURRENT-REPLAY.csv`;
- manifest: `research/dkv-2a-n12/MANIFEST.json`.

No projector, MOR, DeltaKvMemory, DeltaKvContextMemory or observed-history API
was changed.

## Selection

All ten frozen hypotheses hold.

**N11 two-bank composition + N12 verified write locator = RETAIN as the leading
Development identity/provenance semantics.**

Context/model promotion remains blocked.

## Clean verified continuity

At both K=256 and K=1024, P1 recovers every clean PRIMARY fact under R2, R3 and
R4:

- 128/128 exact+clear per route;
- 128/128 VerifiedPresent per route;
- PRIMARY selected for every row;
- screening preserves the latent candidate exactly.

This is the intended separation between source identity and the current
associative query geometry: provenance is minted from the stored write locator,
while the current PRIMARY query key may vary with secondary-route drift.

## Explicit route overrides

P2 succeeds at both widths:

- R2 -> PRIMARY base A: 128/128;
- R3 -> ORDERED override B: 128/128;
- R4 -> PRIMARY base A: 128/128.

Every row is VerifiedPresent.

P3 primary-switch overrides also succeed:

- R2 -> PRIMARY: 128/128;
- R6 -> ORDERED: 128/128.

Source receipts prove raw identity/provenance only; they do not establish that
A or B is semantically correct.

## Identity negative controls

At K=1024 all frozen P4 controls remain non-VerifiedPresent:

- nonexistent record;
- foreign stream;
- wrong subject;
- incomplete subject candidate set.

A record-level success never upgrades an incomplete subject view.

## Conflict is fail-closed

P5 maps complete contradictory subject history to:

- provenance = Conflict;
- matrix reads = 0;
- screened value = None.

The same result holds at K=256 and K=1024.

## Supersession preserves candidate semantics

P6 maps the explicitly superseded two-version history to VerifiedPresent and
performs exactly one selected-bank read.

The corrected H6 control then independently re-enters the authoritative
VerifiedHistory -> bind_record -> bind_projected_subject path and screens the
preregistered intentionally-wrong candidate `[false,false]`.

Requirements all hold:

- both subject versions are retained;
- subject state is Agreement after explicit supersession;
- the wrong candidate remains `[false,false]`;
- source observations do not repair the latent candidate.

This corrected control was the reason for the post-#189 re-run.

## Corruption and snapshot failures are Unverified

For a declared ORDERED override, every P7 cell is Unverified with zero matrix
reads:

- verification budget failure;
- raw corruption;
- same-count fork;
- stale ledger;
- source substitution.

There is no fallback to PRIMARY.

## Alias boundary

P8 retains the existing source-binding boundary at both widths:

- unwritten subject -> Missing;
- complete conflicting requested subject -> Conflict;
- omitted requested-subject version -> Unverified.

Associative key equality does not mint subject presence.

## Provenance is not latent correctness

P9 flips one deterministic V bit while keeping independent RecordRequest and
SubjectKey fixed.

At K=1024:

- all 128/128 rows remain VerifiedPresent;
- 6/128 latent matrix candidates change.

At K=256:

- all 128/128 remain VerifiedPresent;
- 28/128 latent candidates change.

This is desired: provenance identifies the requested record/source snapshot,
not correctness of the current latent state.

## Read/accounting invariant

Across the retained complete panel:

- VerifiedPresent resolutions: 2,306;
- matrix reads: 2,306;
- PRIMARY reads: 1,794;
- ORDERED reads: 512;
- Missing: 6;
- Conflict: 4;
- Unverified: 16;
- verified source segments: 2,308;
- verified source bytes: 30,572.

Therefore every VerifiedPresent source-required resolution performs exactly one
selected-bank matrix read, while non-VerifiedPresent paths perform none.

## Frozen hypotheses

- H1 clean verified continuity — HOLDS;
- H2 explicit override — HOLDS;
- H3 primary-switch override — HOLDS;
- H4 identity negatives — HOLDS;
- H5 conflict fail-closed — HOLDS;
- H6 supersession boundary / wrong candidate preserved — HOLDS;
- H7 unverified fail-closed — HOLDS;
- H8 alias boundary — HOLDS;
- H9 provenance/correctness separation — HOLDS;
- H10 one-bank verified read invariant — HOLDS.

## Mechanistic conclusion

The N11 contradiction is now resolved at both memory-composition and
identity/provenance levels:

1. PRIMARY carries shared knowledge continuity;
2. ORDERED carries explicit route-specific overrides;
3. exact record/route metadata chooses the bank;
4. stored write identity mints provenance;
5. current projected state produces the one numeric read only after successful
   source/record/subject preflight;
6. Missing, Conflict and Unverified remain zero-read fail-closed outcomes.

The remaining model-core questions are no longer synthetic identity binding.
They are:

- representative recurrent SBG/MOR state distributions;
- unbalanced page/expert populations and desired cross-page sharing;
- physical latency/RSS/traffic/energy of the retained composition;
- learned-model qualification at the staged SML scale gates.

No core API or context/model promotion follows from N12 alone.
