# DKV-3a context/provenance integration preregistration

Status: preregistered integration contract. Execution is blocked until SML-CONTEXT-0/1 provide a versioned immutable raw-segment/provenance contract and bounded streaming ingest.

## Motivation

DKV-2a/2b establish a bounded SBG/MOR key projection and frozen-key Delta-KV recurrent memory source.

DKV-2c showed that dynamic re-keying can produce confident output for an unseeded successor key. Therefore DKV-3 MUST NOT treat a Delta-KV read as proof that a source memory record exists.

Delta-KV enters the context programme only as **derived semantic memory**. Exact source authority remains in SML-CONTEXT raw segments and their provenance-linked semantic records.

## Hard prerequisites

DKV-3a implementation MUST NOT begin until current `main` contains:

1. CONTEXT-0:
   - stable stream/segment IDs;
   - exact byte/token/span accounting;
   - immutable raw segment descriptors;
   - semantic-record -> raw-segment provenance;
2. CONTEXT-1:
   - bounded streaming ingest;
   - persisted raw segments;
   - persisted semantic index records;
   - deterministic segment ordering/boundary tests.

The actual versioned `ContextSegmentRef`/equivalent type from those slices becomes authoritative. DKV-3a MUST adapt to that merged type rather than invent a competing segment identity.

## Qualified DKV mechanism

The only Delta-KV recurrent mechanism admitted to DKV-3a is merged DKV-2b:

- one DKV-2a key projected from the admitted current SBG/MOR state;
- key frozen across the bounded recurrent refinement episode;
- direct `BoundedAssociativeMemory` retained independently;
- hard recurrent depth cap retained;
- raw/source provenance outside the Q12 matrix.

DKV-2c dynamic re-keying is explicitly excluded.

## Provenance boundary

A Delta-KV matrix coefficient or read value is never an authoritative source reference.

Every context-linked Delta-KV write MUST emit an immutable sidecar evidence record logically equivalent to:

```text
DeltaKvContextWriteEvidence {
    write_id
    context_generation
    semantic_record_id
    raw_segment_refs[]
    key_digest
    target_digest
    learning_rate_q12
    matrix_shape
    pre_update_counter
    post_update_counter
}
```

The exact field types will reuse the merged CONTEXT-0 identities.

This sidecar proves which admitted sources contributed to a write event. It does **not** claim that a later superposed matrix read can be decomposed back into those sources.

A Delta-KV read is classified as derived semantic evidence. When a task requires exact copying, identifiers, numbers, code, quotations, contradiction adjudication, or source attribution, the runtime MUST independently retrieve/verify the relevant raw segment(s).

## Frozen DKV-3a comparison arms

Once CONTEXT-0/1 are merged, the first DKV-3a experiment MUST compare:

1. `context_no_dkv`
   - SBG/context path without Delta-KV;
2. `context_direct_memory`
   - direct bounded associative memory with explicit record/source identity;
3. `context_dkv_frozen_unverified`
   - frozen-key Delta-KV semantic refinement only;
   - allowed only as a negative/safety control;
   - its output may not satisfy source-required tasks;
4. `context_dkv_frozen_verified`
   - same Delta-KV refinement plus independent provenance-linked raw verification before exact/source-required answers.

All arms share the same raw ledger, semantic records, input order, task fixtures, MOR admission, and recurrent depth limits.

## Frozen task families

The first panel MUST include at least:

- stable fact with unrelated later writes;
- superseding fact/update;
- two conflicting source segments;
- exact numeric identifier;
- exact code/string fragment;
- semantic paraphrase where exact copying is not required;
- missing/unseen fact;
- distractor-heavy context.

Each fixture declares whether raw verification is mandatory.

No task label may be inferred from Delta-KV confidence/magnitude.

## Required evidence

Report separately:

- task exactness/decision correctness;
- false confident unverified answers;
- abstentions/unknowns where supported;
- raw-verification-required fixtures answered without verification (hard failure count);
- raw segments shortlisted;
- raw segments actually reread;
- exact source bytes reread;
- Delta-KV semantic bits;
- direct-memory semantic bits;
- DKV reads, read MACs, updates and saturation;
- SBG recurrent cycles;
- mapped/raw bytes and process RAM when the CONTEXT runtime exposes authoritative counters;
- latency only when measured end-to-end on a declared physical runner.

Logical MAC counts are not latency evidence.

## Promotion gate

The DKV-3a verified arm may advance only if:

1. zero source-required fixture is accepted without raw verification;
2. zero missing/unseen fixture is converted into a confident sourced fact solely from Delta-KV;
3. raw/source provenance remains traceable to immutable CONTEXT segment identities;
4. Delta-KV does not degrade declared task correctness versus the stronger of no-DKV and direct-memory controls;
5. any claimed reduction in raw rereads or recurrent work is reported together with task quality and measured resource counters.

A failure preserves DKV-2b as an isolated semantic-memory mechanism but blocks context promotion.

## Relationship to CCOS-style provenance

The semantic rule is compatible with the ecosystem provenance boundary:

- raw segment = observed/source evidence;
- deterministic semantic record = derived evidence linked to source;
- Delta-KV matrix/read = derived compressed evidence;
- hypothetical/recurrent branch = non-observed until independently verified.

SML-GENIUS remains the owner of its memory semantics; no runtime dependency on CCOS-Enterprise is introduced.

## Non-claims

This preregistration does not establish any effective context length, RAG replacement, model-quality improvement, RAM reduction, latency improvement, or production readiness.
