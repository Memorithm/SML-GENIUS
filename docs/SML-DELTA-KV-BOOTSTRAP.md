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

Freeze a deterministic panel before comparative observation:

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

Only after DKV-1:

- derive bounded key projections from SBG/MOR state;
- keep raw/source provenance outside the compressed matrix when exact recovery is
  required;
- feed Delta-KV reads into recurrent refinement as one memory source;
- compare fixed recurrent depth, adaptive depth and no-Delta-KV controls;
- preserve direct associative memory as a separate mechanism.

Delta-KV must not become the only store for identifiers, exact code, numbers or
source evidence.

## DKV-3 — context and long-range evaluation

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

A future SML repair controller may use:

- Delta-KV correction magnitude;
- SBG state change;
- MOR disagreement;
- decision margin/ambiguity;
- recurrent stability.

These signals may request another recurrent refinement cycle. They must be
compared against fixed-depth and simpler deterministic controls.

Forge may search candidate key projections, dimensions, learning rates and
topologies, but SML independently requalifies every candidate. ElasticXxx may
enforce resource budgets without owning memory semantics.

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
