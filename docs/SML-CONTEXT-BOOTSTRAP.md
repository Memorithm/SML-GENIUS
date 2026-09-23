# SML-CONTEXT: hierarchical context horizon programme

SML-GENIUS must support long-context reasoning without requiring the complete
history to remain resident in RAM or to be rescored densely for every new
decision/token.

This programme deliberately separates **raw context length** from **effective
retrievable context**. A large on-disk archive is not, by itself, evidence of a
large usable model context.

## Architectural contract

SML context is a hierarchy:

```text
incoming bytes/tokens
      |
      v
C0  bounded ingest buffer
      |
      +--> immutable Raw Context Ledger
      |
      v
C1  recurrent SBG working state (V/M/A/R)
      |
      +--> C2 bounded associative/episodic memory
      |
      +--> C3 mapped SBG semantic index
      |
      +--> C4 mapped raw segment store
      |
      v
MOR shortlist -> selected context pages/segments -> expert/memory refinement
```

The active working set stays bounded. Older context may remain accessible through
mapped/indexed storage rather than being copied into the live working state.

### Dual representation is mandatory

Long context must preserve both:

1. **raw immutable segments** for exact copying, quotations, code, numbers,
   identifiers and other information that cannot safely be reconstructed from a
   semantic summary;
2. **SBG semantic index records** for low-cost relevance/routing over those
   segments.

Every semantic record must retain provenance back to one or more raw segment IDs.
SBG summaries never silently replace source bytes.

## Context is not one number

Every benchmark/report must distinguish:

- raw input bytes ingested;
- tokenizer tokens ingested under the frozen tokenizer;
- recent raw tokens/bytes resident in the C0 ingest window;
- active SBG bits/state count;
- associative-memory entries;
- mapped semantic-index records;
- mapped raw segments;
- bytes resident in process RAM;
- mapped virtual bytes;
- disk bytes;
- page faults where measurable;
- context pages/segments shortlisted;
- context pages/segments actually decoded/read;
- recurrent cycles used;
- exact source bytes reread;
- latency p50/p95/p99 and throughput.

Do not claim an N-token context window merely because N tokens were written to
disk. A context length is admitted only when a frozen task demonstrably requires
information separated by that span and the model retrieves/uses it at the
declared quality and resource budget.

## Segment identity and provenance

Each raw context segment must eventually carry a stable descriptor such as:

```text
ContextSegmentRef {
    stream_id
    segment_id
    byte_start / byte_len
    token_start / token_len
    order_index
    content_hash
    semantic_record_ids[]
}
```

The exact serialized contract is an implementation task. Hashing/provenance must
not be conflated with CRC corruption detection.

## Context routing

Long-context search should be hierarchical rather than a complete scan.

Reference direction:

```text
query/current SBG state
  -> cheap SBG/MOR candidate shortlist
  -> bounded top-k segment/page set
  -> exact semantic/context comparison
  -> optional raw-segment reread
  -> associative/recurrent refinement
```

This is compatible with the decision programme: a large candidate or document
set can be reduced before the final decision expert executes.

The shortlisting idea is independently useful from Laya, but SML must implement
it with native SBG/MOR/Boolean/low-bit mechanisms rather than importing Laya's
encoder or Python runtime.

## Context retention policy

Retention/eviction experiments may use explicit evidence such as:

- recency;
- semantic novelty;
- unresolved ambiguity;
- routing relevance;
- repeated access;
- task/user pinning;
- source importance;
- exact-copy requirement.

No learned eviction policy may silently discard the only raw source for an
unresolved fact. Lossy semantic compaction must retain provenance and must be
evaluated against a no-compaction control.

## Evidence-gated slices

### CONTEXT-0 — accounting and immutable segment contract

- exact byte/token/span accounting;
- stable stream/segment IDs;
- immutable raw segment descriptors;
- provenance from semantic records to raw segments;
- no claim of long-context capability yet.

### CONTEXT-1 — streaming ingest

- bounded C0 ingest buffer;
- ingest arbitrarily long streams without retaining the complete raw input in
  process RAM;
- incrementally produce SBG context records;
- persist raw segments and semantic index records independently;
- exact order and boundary tests.

### CONTEXT-2 — recurrent working context

- recurrently update bounded SBG working state from incoming segments;
- compare fixed-state, expanding-state and bounded recurrent-state controls;
- measure information retained/lost rather than assuming compression helps.

### CONTEXT-3 — hierarchical retrieval

- SBG/MOR top-k context shortlist;
- bounded associative lookup;
- exact raw-segment reread when required;
- compare against exhaustive semantic scan on small fixtures;
- prove retrieval work grows sublinearly on declared datasets before scale claims.

### CONTEXT-4 — retention and compaction

- hot/warm/cold context tiers;
- explicit promotion/eviction accounting;
- optional lossy semantic compaction with preserved raw provenance;
- matched no-compaction controls;
- no silent deletion of pinned/exact-source context.

### CONTEXT-5 — recurrent long-range refinement

- allow selected historical segments/memory records to re-enter recurrent
  reasoning;
- adaptive halting;
- compare fixed depth, adaptive depth and no-history refinement at matched
  budgets.

### CONTEXT-6 — long-range learning

- curriculum with increasing dependency spans;
- train retrieval/routing/retention from task loss/evidence rather than only
  hand-authored policies;
- separate train/development/protected long-context holdouts;
- include exact-copy, reference resolution, contradiction, multi-segment
  composition and decision tasks.

### CONTEXT-7 — scale ladder

Candidate evaluation ladder, not a promise:

```text
4k -> 16k -> 64k -> 256k -> 1M tokenizer-token input spans
```

Progress only when the previous span satisfies quality, latency, memory and
traffic gates. Larger spans may be tested later, but no "infinite context" claim
is permitted.

## Required benchmark families

At minimum:

- exact needle retrieval with controlled depth and distractors;
- multiple needles / set membership;
- exact copy of identifiers/numbers/code fragments;
- long-range coreference/reference;
- contradiction across distant segments;
- chronology/order questions;
- multi-hop composition across separated segments;
- repeated fact with superseding updates;
- decision over large structured record streams;
- adversarial distractor density;
- context poisoning/irrelevant-segment robustness.

Each benchmark must include trivial recency and exhaustive-scan controls where
appropriate.

## Relationship to SML-D

The decision programme should exploit context without forcing generation:

```text
large state/context
 -> context shortlist
 -> MOR capability route
 -> Elastic working set
 -> decision expert
 -> DIRECT / REFINE / ABSTAIN
```

SML-D2 calibration must include context-depth and retrieval-failure strata.
Confidence is not allowed to hide an incapable context route.

## Relationship to mapped stores and Elastic Weights

SBG-0.13 provides the mapped semantic-store foundation. CONTEXT must add a raw
segment ledger/store and provenance links; the semantic store alone is not enough
for exact context recovery.

Elastic Weights control model/expert pages. SML-CONTEXT controls input/history
pages. The two page classes must remain separately accounted even if a future
runtime shares cache/prefetch infrastructure.

## Scientific boundary

This programme is an architectural target. SML-GENIUS has not yet demonstrated
4k, 16k, 64k, 256k or 1M-token effective context under this contract.

A mapped archive is not a context-window result. A successful needle lookup is
not general long-context reasoning. A compressed semantic state is not proof that
the omitted raw details are recoverable.

Promotion requires measured task quality together with exact RAM, mapped/disk
storage, retrieval traffic, page faults where available, recurrent work and
latency evidence.
