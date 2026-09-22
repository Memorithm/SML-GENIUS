# SML-D1b: observed SBG to typed decisions

Implementation: `crates/sml-decision/src/semantic.rs`.
Tests: `crates/sml-decision/tests/semantic.rs`.

## Meaning, availability and uncertainty are different

An SBG state has V (semantic value), M (relevance), A (ambiguity) and R (routing).
None of them alone records whether its source supplied a bit. The bridge takes
four explicit borrowed availability masks through ObservedFields. They can share
one mask when availability is identical, and require no field cloning.

For ContextValue(i), the feature is known iff:

    observed_V[i] AND observed_M[i] AND observed_A[i] AND M[i] AND NOT A[i]

A known feature then carries V[i], including false. Otherwise it carries an
unknown marker with a canonical zero payload, not the Boolean answer false.
Meaning/Ambiguity/Routing metadata use only their own observation masks. A known
ambiguity bit may truthfully report ambiguity without asserting a reliable V.
These masks are caller assertions, not provenance authentication or calibration.
Logical inference may still produce a determinate answer from partial inputs
when the existing D0 truth-table semantics justify it (e.g. false AND unknown).

## Frozen projection and batch execution

A projection binds 1..64 selectors to an explicit schema ID and nonzero version.
Duplicate selectors and invalid coordinates are rejected. Different kinds may
select the same coordinate. The program must match the projected input count and
schema. Every input must match schema/version; a mismatch rejects the full batch
before field reads. Versions are caller-managed identifiers, not content hashes.

Only declared coordinates are read. ContextValue costs at most six logical bit
reads (three availability plus three state); other selectors cost at most two.
All heads reuse the same projected record and gate planes. Batch plans separately
bound projection buffers/reads and decision results/scratch/work. Counters reflect
logical accesses, not CPU instructions, physical I/O, peak allocator usage or RSS.
The combined element-payload bound excludes caller fields/masks/programs and
allocator overhead. Returned results may be retained by callers outside the bound.

There is no text decoder, floating probability, new dependency or unsafe code.
All declared D0 gates still execute: selective MOR execution remains future work.

## Learning and scientific boundary

fit_expert uses the same projection for training and validation, rejects unknown
selected features, checks sample/projection limits, and delegates table selection
to existing integer-only D1a. Training chooses among sixteen functions; validation
can reject but cannot select. Overlapping/duplicate case IDs are rejected; distinct
IDs do not establish deduplicated or independent source observations.

The accepted object is a finite table proposal, not a live weight commit. Tests
cover all sixteen functions, development rejection and separate test IDs. These
fixtures identify four Boolean input patterns; they do not show generalization
to unseen concepts or natural-language questions. Projection coordinates are
hand-declared metadata. Broader D1 must learn its semantic features and experts.

The store test decodes actual SBG v1 bytes before projection. It deliberately
requires external availability masks: neither CRC nor a successful decode proves
that the decoded semantic statements were observed or correct. This is not yet
mmap/MOR/Elastic/recurrent end-to-end integration and does not complete SBG-0.14.

## Next gates

1. Learned feature selection/experts with matched fixed-coordinate controls.
2. Sparse expert execution and bounded streamed publication of decisions.
3. Independent calibration, risk/coverage and shift evaluation.
4. End-to-end latency/quality measures including encoding, storage and outputs.

The format keeps core, decision and store responsibilities separate. No external
Memorithm repository is modified by D1b; research transfers still require the
destination-side requalification documented in SML-D-REUSE.md.
