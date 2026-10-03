# C aliasing integration: adapted observation and API contract

The new 3094-line upload is identified in SML-DKV-C-INTEGRATION-PLAN.md.
Its state generator was ported; the deprecated unchecked core adapter was NOT
imported. This is an adapted current-API diagnostic, NOT execution of the
complete original Rust1.97 harness or validation of its claimed chronology.
Native fixed-page5 MOR executes; one page does not establish useful competition
or prove that this is worst-case aliasing. Existing A/A2/B reports are untouched.

## Retained adapted evidence

The immutable manifest identifies code, protocol, Rust1.89 and output hash.
Two explicit executions were byte-identical. There are 3072 query rows with
binding alternatives in each row, not 3072 independent real-world tasks.
The original FNV candidate counts and current full-vector candidates are both
reported. Raw bytes are genuinely reread and SHA-verified by the current API.
Source work is shared instrumentation and is not a free per-arm computation.
No matrix query is performed: these are source-attribution outcomes, not
Delta-KV latent retrieval accuracy or improvement from recurrence.

```
SUMMARY_HEADER,K,contrast,kind,queries,legacy_key,legacy_record,verified_key_candidates,record_bound,exact_index_bound,index_only,rejected_query,hash_vector_disagreement,wrong_subject_candidates
SUMMARY,16,false,independent,96,0,0,0,0,0,0,0,0,0
SUMMARY,16,false,near_1,96,69,0,69,0,0,0,0,0,69
SUMMARY,16,false,near_16,96,2,0,2,0,0,0,0,0,2
SUMMARY,16,false,near_2,96,46,0,46,0,0,0,0,0,46
SUMMARY,16,false,near_32,96,0,0,0,0,0,0,0,0,0
SUMMARY,16,false,near_4,96,26,0,26,0,0,0,0,0,26
SUMMARY,16,false,near_8,96,3,0,3,0,0,0,0,0,3
SUMMARY,16,false,written,96,96,96,96,96,96,96,0,0,0
SUMMARY,16,true,independent,96,0,0,0,0,0,0,0,0,0
SUMMARY,16,true,near_1,96,63,0,63,0,0,0,0,0,63
SUMMARY,16,true,near_16,96,1,0,1,0,0,0,0,0,1
SUMMARY,16,true,near_2,96,39,0,39,0,0,0,0,0,39
SUMMARY,16,true,near_32,96,0,0,0,0,0,0,0,0,0
SUMMARY,16,true,near_4,96,18,0,18,0,0,0,0,0,18
SUMMARY,16,true,near_8,96,8,0,8,0,0,0,0,0,8
SUMMARY,16,true,written,96,96,96,96,96,96,96,0,0,0
SUMMARY,64,false,independent,96,0,0,0,0,0,0,0,0,0
SUMMARY,64,false,near_1,96,39,0,39,0,0,0,0,0,39
SUMMARY,64,false,near_16,96,0,0,0,0,0,0,0,0,0
SUMMARY,64,false,near_2,96,17,0,17,0,0,0,0,0,17
SUMMARY,64,false,near_32,96,0,0,0,0,0,0,0,0,0
SUMMARY,64,false,near_4,96,1,0,1,0,0,0,0,0,1
SUMMARY,64,false,near_8,96,0,0,0,0,0,0,0,0,0
SUMMARY,64,false,written,96,96,96,96,96,96,96,0,0,0
SUMMARY,64,true,independent,96,0,0,0,0,0,0,0,0,0
SUMMARY,64,true,near_1,96,57,0,57,0,0,0,0,0,57
SUMMARY,64,true,near_16,96,0,0,0,0,0,0,0,0,0
SUMMARY,64,true,near_2,96,42,0,42,0,0,0,0,0,42
SUMMARY,64,true,near_32,96,0,0,0,0,0,0,0,0,0
SUMMARY,64,true,near_4,96,14,0,14,0,0,0,0,0,14
SUMMARY,64,true,near_8,96,1,0,1,0,0,0,0,0,1
SUMMARY,64,true,written,96,96,96,96,96,96,96,0,0,0
PROMOTION,BLOCKED,source_attribution_is_not_model_quality
```

## Useful integration, not weaker acceptance

VerifiedContext::bind_record(RecordRequest, RecordBindingLimits) provides a
private, nonempty borrowed view for an explicitly requested record VERSION.
The stream and full-key candidate set must match, the write must exist, and
all referenced sources have already passed the parent receipt verification.
The view allocates no buffers and performs no new raw I/O; counts disclose the
full parent candidate set inspected. Limits cover those inspected elements,
not only the successful filtered subset. Parent verification/storage costs
remain. A record claim is not authenticated by constructing RecordRequest.

The view has no Accept or screen method. A source-bound individual version
cannot bypass a subject's other versions: complete VerifiedHistory and
BoundProjectedSubject::screen are still required for subject-level screening.
Tests explicitly retain conflicts and reject incomplete subject candidates
even when bind_record succeeds. Do not replace that path with the upload's
single-record filter. Agreement still never repairs a wrong latent result.

## Identity and negative controls

C's FNV surrogate encodes V/M/R and ignores A. A separate outside-panel test
changes each A bit and gets the same surrogate but a different full-state
identity. This is NOT a found collision over the surrogate's declared 192-bit
input, and does not prove that every change of A denotes a different subject.
The complete-state fixture index is an exact diagnostic control, not a learned
semantic identity service. It stores the admitted states and uses comparisons;
its cost is extra and it determines membership independently of the matrix.
A semantic record identifier must come from an appropriate independently
qualified index. A hash or a projected key cannot silently take that role.
Independent random queries are not called orthogonal in the adapted output.
No probabilistic worst-case bound or benefit of MOR diversity is inferred.

## Boundaries and next work

Original input claims of source correctness are not model accuracy. Record
selection and full subject history solve different questions. Source identity
is neither truth nor entailment nor an external action grant. No trained or
linguistic states, durability, physical RAM or timing are qualified here.
The next research task should improve information preservation on independently
specified learned/representative tasks while keeping these exact boundaries.
No unqualified cross-repository propagation: CCOS may reuse the identity
contract after destination review, and TDI/SciRust the diagnostic methodology.
The current task changes no numeric core, dependency or protected policy.

**Context/model promotion remains BLOCKED.**
