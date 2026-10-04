# SML Delta-KV bootstrap

Status: research-only.

## Purpose

SML-GENIUS already has bounded direct associative memory, recurrent state,
mapped semantic storage and hierarchical-context work. Delta-KV studies a
different mechanism: a bounded rewritable associative matrix that can correct an
existing association in place instead of only appending or replacing complete
records.

The external Cadence architecture is used only as a hypothesis source for
rewritable associative memory and repair-oriented computation. SML does not
import Cadence code, equilibrium semantics or performance claims.

The SML adaptation is deliberately native:

- deterministic Rust reference first;
- signed fixed-point arithmetic rather than an unconstrained floating-point core;
- bounded matrix dimensions;
- exact semantic-bit accounting;
- explicit saturation/interference evidence;
- independent comparison against existing SML memory;
- no Transformer-semantic KV-cache requirement.

DKV-0a merged through PR #78 as `6807f978ecbc5b3d6c8f31c98dfd753ab5c208c8`.

DKV-0b merged through PR #79 at `2fa5ca2e2cacaa05056a1d60a79efbe1e8536f68`.

DKV-1 merged through PR #80 at `c3bbca6241a6244afccc828a21faa870f6743a5b`.

DKV-2a merged through PR #81 at `e20310763073c9767de1a6c45cc1a5fa15a4d894`.

DKV-2b merged through PR #83 at `6a26a65d0c953a7e83aedc9f6e785a7df8b88ea8`.

DKV-2c was executed in PR #86 and is preserved in [`SML-DKV-2C-OBSERVATION.md`](SML-DKV-2C-OBSERVATION.md). Dynamic re-keying demonstrated deterministic state-dependent key changes but is **not promoted**: the missing-successor control remained confidently non-ambiguous and the dynamic arm showed no final-state advantage over merged DKV-2b on the frozen target fixtures.

The next context-facing Delta-KV slice must therefore use the qualified DKV-2b frozen-key recurrence. Dynamic re-keying remains research-only until a separate miss/confidence mechanism is preregistered and qualified.

## DKV-0a — fixed-point normalized delta reference

The first implementation is crates/sml-core/src/delta_kv.rs.

Representation:

- key: signed Q12 i16 vector;
- value target: signed Q12 i16 vector;
- matrix: signed Q12 i16 coefficients;
- read output: signed Q12 i32 values to avoid silent accumulation clipping;
- learning rate: Q12 value in 0..=1.

Update:

    M <- M + eta * k^T (target - kM) / ||k||^2

The normalization is an SML-specific stability choice, not a claim about the
external source architecture. With a full-rate update and no storage saturation,
the current key is corrected to its target up to deterministic integer rounding.

Required DKV-0a controls:

- zero/malformed dimensions fail closed;
- zero key norm fails closed;
- one-hot write and rewrite;
- orthogonal-key non-interference;
- overlapping-key interference is preserved as evidence;
- exact matrix and metadata bit accounting;
- saturation count is explicit.

This is reference semantics, not a throughput result.

## DKV-0b — interference and correction panel

The implementation freezes 14 deterministic cases in source before comparative observation. The panel includes:

- orthogonal keys;
- controlled cosine/overlap bands represented in fixed point;
- repeated overwrite of one key;
- conflicting targets;
- partial learning rates;
- saturation stress;
- increasing key/value dimensions.

Report:

- current-key correction error;
- previously stored-key error before/after each write;
- interference matrix;
- saturation count;
- read/update MAC counts;
- semantic bits;
- update count.

No favorable key family may be selected after seeing confirmatory outcomes.

## DKV-1 — matched memory controls

The reference control uses a 704-bit declared ceiling: Delta-KV 16x2 and the
bounded append ledger each consume exactly 704 semantic bits, while the direct
associative memory fits three 32-bit value slots in 676 bits. The same frozen
6-write / 8-read script is replayed against every backend.

Compare Delta-KV at matched declared storage budgets against:

1. BoundedAssociativeMemory with RejectCollision;
2. BoundedAssociativeMemory with ReplaceExisting;
3. append/immutable record lookup where relevant;
4. density/capacity-matched deterministic controls.

Required task families:

- overwrite/update facts;
- stable facts with unrelated writes;
- conflicting facts;
- associative recall;
- repeated correction;
- context supersession with raw provenance retained.

A matrix update is not automatically preferable to a discrete record update.
Promotion requires task evidence.

## DKV-2 — SBG/MOR/recurrent integration

DKV-2a introduces a deterministic integer-only projection from V/M/A/R plus the
already-produced MOR selected-expert set into a bounded Q12 key. It does not run
another router. Optional raw/source provenance is copied into evidence and is
mathematically excluded from the key: changing only provenance must leave the
projected key unchanged.

DKV-2b keeps the DKV-2a query projection frozen across recurrent cycles so the
effect of memory reads is isolated from dynamic re-keying. A Delta-KV read may
rewrite configured V coordinates and mark low-magnitude values ambiguous. Such
ambiguity may block stability halting but never the hard max-depth bound.

Every DKV-2b comparison carries three paths: adaptive Delta-KV recurrence,
matched fixed-depth Delta-KV recurrence, and a no-Delta-KV identity control.
Direct associative memory remains a distinct coexisting mechanism.

Only after DKV-1:

- derive bounded key projections from SBG/MOR state;
- keep raw/source provenance outside the compressed matrix when exact recovery is
  required;
- feed Delta-KV reads into recurrent refinement as one memory source;
- compare fixed recurrent depth, adaptive depth and no-Delta-KV controls;
- preserve direct associative memory as a separate mechanism.

Delta-KV must not become the only store for identifiers, exact code, numbers or
source evidence.

### DKV-2c — dynamic re-keying qualification

DKV-2b deliberately froze the projected query key across recurrent cycles. DKV-2c asks one narrower question: when the SBG state changes after a memory read, does deterministically re-projecting the key from that new state alter subsequent memory evidence in a useful and bounded way?

The comparison must keep MOR selection fixed and must not invoke another router. Raw/source provenance remains outside the key mathematics. Required controls are frozen-key adaptive recurrence, dynamic-rekey adaptive recurrence, matched fixed-depth recurrence, and no-Delta-KV identity recurrence. Key projections, read MACs, recurrent depth, ambiguity and final semantic deltas must be reported separately.

A favorable DKV-2c result is still reference-mechanism evidence, not a long-context, model-quality, latency or scaling claim.

## DKV-3 — context and long-range evaluation

DKV-3a is preregistered in [`SML-DKV-3A-CONTEXT-PREREGISTRATION.md`](SML-DKV-3A-CONTEXT-PREREGISTRATION.md). Execution is blocked until CONTEXT-0/1 merge their authoritative raw-segment/provenance contract and bounded streaming ingest. DKV-3a admits only DKV-2b frozen-key recurrence; DKV-2c dynamic re-keying is excluded.

Integrate with SML-CONTEXT only after exact provenance boundaries exist.

Measure separately:

- raw bytes/tokens ingested;
- active Delta-KV bits;
- direct associative-memory bits;
- mapped semantic/raw bytes;
- recurrent work;
- retrieval/update traffic;
- end-to-end latency.

A small bounded Delta-KV that summarizes history is not evidence of an effective
long context unless tasks requiring the distant information succeed.

## DKV-4 — adaptive compute handoff

DKV-4 has completed its bounded Development implementation sequence. The full
decision and evidence summary is in
[`SML-DKV-4-FINAL-REPORT.md`](SML-DKV-4-FINAL-REPORT.md).

The controller is a compute-control plane, not an answer verifier. Its actions
are `Refine`, `StopCandidate` and `Abstain`; there is no `Accept` action.
Provenance is evaluated before numeric memory signals: Missing/Conflict abstain
before a matrix read, while Unverified evidence can never StopCandidate.

### DKV-4a reference

The first controller combined:

- Delta-KV absolute Q12 correction magnitude;
- exact SBG V/M/A/R state change;
- memory-output ambiguity;
- MOR structural disagreement and precision escalation;
- recurrent depth;
- external provenance state.

On the frozen K64/K256 Development panel it preserved the same written-state
correctness as fixed-depth and stability controls but used 490/491 total cycles,
versus 251/250 for stability-only. The absolute correction signal could remain
large after the Boolean semantic state stopped changing. That rule is therefore
retained as negative evidence and rejected as the default adaptive policy.

### DKV-4c semantic-impact reference

DKV-4c keeps correction magnitude as telemetry but removes it as an independent
reason to continue. Refinement depends on actual semantic change, ambiguity or
MOR escalation/disagreement, all under the same hard maximum depth.

On the unchanged paired panel:

- K64: correctness 125/128, 251 cycles, 32,128 read MACs;
- K256: correctness 128/128, 250 cycles, 128,000 read MACs;
- every DKV-4a written-query correctness result is preserved;
- no retained written row is deeper than DKV-4a;
- recurrent work falls 48.78% at K64 and 49.08% at K256 versus DKV-4a.

However, DKV-4c exactly matches SBG stability-only in aggregate correctness and
cycle count on this fixed one-page, zero-disagreement MOR panel. The additional
MOR signals therefore remain unqualified as a source of task benefit.

### Dimension and unknown-state boundary

The same panel showed 128/128 written correctness at K256 versus 125/128 at K64,
while K256 requires four times as many key-coordinate MAC terms per read/update.
This synthetic difference does not identify an optimal K.

Unknown handling is provenance-driven. Missing provenance produced zero matrix
reads and zero StopCandidate decisions. In the separate Unverified latent
diagnostic, the matrix frequently produced resolved coordinates for unknown
states; numeric resolution is therefore not presence proof or calibrated
confidence.

### Remaining qualification

DKV-4 Development engineering is complete, but scientific/model promotion is
blocked pending:

- a separately preregistered non-trivial MOR disagreement/escalation panel;
- learned or representative semantic states;
- measured wall-clock/RSS/traffic/energy evidence;
- controlling CONTEXT/provenance prerequisites for end-to-end context use;
- independent reruns for any model-level claim.

Forge may later propose controller/key candidates and ElasticXxx may enforce
declared resource budgets, but SML independently requalifies them and retains
memory/control semantics.

## Relationship to FLAT-ATTENTION

FLAT MAA-14 studies adaptive widening of sparse exact attention. SML Delta-KV
studies bounded rewritable associative memory. They may later be compared or
combined by a higher-level runtime, but neither repository imports the other's
semantic ownership.

Dense/attention baselines remain valid external controls. No result from one
programme is automatically evidence for the other.

## Scientific boundary

Delta-KV is an architectural hypothesis plus deterministic reference semantics.

No claim is made yet that it:

- improves language-model quality;
- replaces attention;
- beats the existing SML direct associative memory;
- scales to the target 25B model;
- reduces RAM/latency in a complete model;
- avoids interference for overlapping keys.

Those are experimental questions gated by DKV-0b through DKV-4.
