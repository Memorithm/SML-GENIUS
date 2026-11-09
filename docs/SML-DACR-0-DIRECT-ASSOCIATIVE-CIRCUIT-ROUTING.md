# SML-DACR-0 — direct associative circuit routing (research candidate)

**Status: RESEARCH CANDIDATE ONLY — not implemented, not preregistered as a confirmatory experiment, not promoted.**  
**Date:** 2026-11-09. **Owner of model semantics:** SML-GENIUS.

## Problem and boundaries

Investigate whether SML can select a bounded useful fraction of **both** historical memory and the learned circuit atlas without all-history token scoring or all-expert scanning, while preserving long-range exact recall, next-token learning and independently measured quality/cost parity.

This is an architectural *candidate*, not a novelty, complexity, quality, 25B or speed claim. SML-0 prohibits Transformer-style Q/K/V, QK^T, softmax token scoring and disguising a full-history similarity scan as direct routing. External attention systems remain baselines only. Model and Harness remain separate.

Existing authoritative building blocks **must be reused**, not duplicated:
- `sml-store::context_retrieve` (CONTEXT-3): SBG prefix buckets -> MOR top-k -> raw SHA-256 verification; current sublinear results are synthetic fixture-only.
- CONTEXT-0/1 immutable raw segments, provenance and semantic index; CONTEXT-2 working state; CONTEXT-7/B1 protected long-range evaluation.
- `sml-core` SBG V/M/A/R, native MOR hierarchical routing and expert-page IDs, bounded direct associative memory, elastic page activation, frozen-key DKV-2b; do not promote failed DKV-2c dynamic rekeying.
- G1 search-key encoder experiments: a wrong/over-coarse representation can destroy gold recall even when an index is fast; the reported v1 exhaustive gold@1 was 0.032 for its specific real-text fixture.
- A6/ENG full-training n-gram capacity negatives; no claim that longer n-grams generalize semantically, and changing predictive context depth changes model semantics.

## Candidate: three independently qualified access lanes

```text
input + frozen recurrent SBG state (V/M/A/R) + task/epoch metadata
        |
        +--> E: exact/source lane (canonical literal/identifier/tuple keys)
        |      versioned bounded index -> exact-key comparison
        |      -> provenance-bound immutable raw segment verification
        |
        +--> L: latent semantic lane (research: trained discrete SBG-derived codes)
        |      bounded direct buckets / versioned index
        |      -> existing MOR shortlist and limited exact comparisons
        |      -> candidates (NOT source truth)
        |
        +--> P: circuit lane (existing SBG R + native MOR / direct page routing)
               -> bounded selected atlas PageIds
               -> native Elastic Weight residency/admission
        |
      source-verified evidence + derived semantic memory + selected circuits
        -> SML recurrent workspace / qualified DKV-2b frozen-key refinement
        -> existing decision / generation head
        -> commit / explicit unknown / abstain
```

E and L index **context records**. P indexes **learned circuit pages**; a circuit-page hit is not evidence that an old fact exists. Never silently mix these namespaces or generation/stream epochs. No full-history score, Hamming search, or global neural ANN probe is hidden under a hash name.

**E lane:** a deterministic, stream/epoch-scoped exact key is a candidate pointer only. Equal hash/fingerprint never proves equal key: compare canonical key bytes or a versioned collision-safe exact-key representation and independently authenticate the raw source through existing CONTEXT checks. Missing source, stale manifest, corrupt record, collision, contradictory sources, or bucket cap violation -> explicit miss/conflict/error, not a confident answer. Index construction may cost O(N) ingest/storage; query work is bounded only if its structures, bucket/fanout caps and failure semantics are accounted.

**L lane:** optional research inspired by latent-discrete n-gram memory; map SBG observed/known bits to a small fixed number of discrete addresses through a **trained** encoder. Unknown mask bits must not be coerced to false. Freeze hard addressing for deployment. This is only an auxiliary shortlist; access a small declared number of candidates and validate their identity/provenance before source-dependent answers. Trainable codes are not evidence that unseen paraphrases or exact arbitrary facts can be retrieved. Include strong collisions, rare facts, adversarial near misses and OOD paraphrases.

**P lane:** use the existing native MOR authority for final page selection; optional trainable address-to-MOR proposal maps must be evaluated against fixed and exhaustive references, not become a second unqualified router. Selected page counts, active parameter touches, runtime page faults and byte transfers are separate counters. No new CUDA or NVIDIA SDK dependency.

**Correction / uncertainty:** DKV-2b may refine *derived* semantic state. It cannot authenticate facts. A task requiring numbers, names, exact copied spans, code, citations or conflict resolution must reread/verify raw source. A bounded controller may widen from e.g. 1 -> 2 -> 4 *qualified* candidates on uncertainty, subject to frozen per-query budgets. If it still cannot verify, abstain; never widen into an unbounded history scan on the SML candidate path.

## Training hypothesis, not a trained learner

Compare (a) fixed literal addresses, (b) fixed SBG prefix, (c) trained discrete latent address and (d) trained latent address plus native MOR page routing under the **same** model capacity and training budgets. During TRAIN only, bounded soft/straight-through or discrete task-loss learning can be explored; deployment is discrete. The loss must include task/next-token quality, hard budget penalties and independently separated collision/abstention calibration. A teacher Transformer may be used to define an external distillation comparator but cannot supply attention at candidate inference.

No route-label oracle alone counts as task-level learning, and isolated supervised routing correctness cannot establish language-model generalization. Prevent expert starvation and code collapse; report code occupancy, effective atlas utilization, load skew and rare-case recall. No TEST labels influence routing, calibration, feature selection or stopping.

## Evidence and preregistration order

Before writing runtime or experimental training code, freeze a separately versioned `DACR-1` protocol: tasks, static/learned addresses, matched training compute/data/seeds, code/weight SHAs, resource envelopes, DEV/TEST separation, metrics, equivalence margins and stopping rules. Do not modify existing LONGCTX-B1 protocol or reuse its protected TEST observations for tuning.

Mandatory controls:
1. Exact source-index with collision-preserving verification; CONTEXT-3 baseline; raw exhaustive oracle outside candidate.
2. Fixed SBG prefix; learned latent code; learned code + MOR pages; MOR dense/all-pages **control only**.
3. Direct memory alone; DKV-2b only; DKV-2b + provenance-verified reread; no-memory control.
4. Lexical BM25, a pinned open-weight attention model and at least one trained recurrent reference as external baselines.
5. Strong negative controls: absent key, hash collision, stale epoch, missing provenance, corruption, deliberate bucket overflow, contradictory updates, paraphrased same meaning, unrelated similarity, multi-hop and repeated overwrites.

Primary evaluation must report **all**: exact/multi-needle/multi-hop recall, false-source assertions, abstention/calibration, next-byte/token held-out log-loss, task quality, effective source span, model-parameter count, trained bytes, index bytes, active circuit fraction, page/segment touches, physical memory traffic, page faults, peak RSS/VRAM, latency p50/p95/p99 and end-to-end throughput. Training cost is separate from serving cost.

A constant number of table probes is **not** constant effective retrieval, nor proof of constant I/O latency: index size, collisions, page faults, reading selected pages and recurrent cycles matter. In particular, a bounded-work path may fail recall while a full-scan control succeeds. Such a result is a scientific NO_GO, not success through abstention.

Eligibility for promotion requires preregistered non-inferiority on **both** trained learning quality and the quality/cost frontier of a complete integrated model. Preserve and report negatives; no extrapolation from fixtures to 25B.

## Integration contract / reuse opportunities

- **SML-GENIUS:** owns discrete latent address semantics, trainability, source-required answer policy, SBG/MOR/page activation, DKV integration and scientific promotion.
- **SciRust:** independent deterministic bit/index, SIMD-oracle and tensor baseline primitives, only if broadly reusable.
- **FLAT-ATTENTION:** external dense/MAA correctness and measured attention reference; never imported as candidate attention.
- **NNIS:** only optional CPU/WGPU portable model-execution qualification; legacy CUDA path not required.
- **ElasticXxx:** generic source/weight residency and budget plans *after* model-authorized semantics and qualified candidate identities.
- **RemoteOps:** authoritative execution/control plane for future Dell/Thor/Mac native measurements; no host side effects in DACR-0.

**Proposed stage gates:** DACR-0 audit/design (this note) -> DACR-1 preregistered exact-key/collision oracle -> DACR-2 trained latent-code vs fixed control -> DACR-3 model-native MOR/page integration -> DACR-4 fully trained model quality/cost and cross-platform physical evidence. No stage automatically authorizes the next.

## External literature (hypothesis sources, not Memorithm results)

- Cheng et al., *Conditional Memory via Scalable Lookup: A New Axis of Sparsity for Large Language Models*, ACL 2026, DOI: 10.18653/v1/2026.acl-long.226. Static conditional memory / direct lookup and compute-memory allocation.
- Zheng et al., *Lngram: N-gram Conditional Memory in Latent Space*, May 2026, arXiv:2605.24869. Learned discrete latent addresses, preliminary evidence.
- Zeng et al., *Attention with Routed-Memory for Learnable Sparse Control*, ICML 2026, PMLR 306. Hierarchical learning and variable access budget; its attention is **not** imported.
- Hu et al., *Preserving Memory Function in Hybrid Language Models: A Survey of State, Evidence, Access, and Granularity*, EMNLP Findings 2026. State/evidence/access separation.
- Tandon et al., *End-to-End Test-Time Training for Long Context*, arXiv:2512.23675 (2025/26). Continual learning / training-vs-serving cost reference only.
- Internal: `docs/SML-CONTEXT-3-RETRIEVAL.md`, `docs/SML-CONTEXT-BOOTSTRAP.md`, `docs/SML-LONGCTX-B1-PREREGISTRATION.md`, `docs/G1-CONTEXT-3-KEY-ENCODER-V2-PREREGISTRATION.md`, `docs/SML-DKV-3A-CONTEXT-PREREGISTRATION.md`, `docs/SML-0-SPEC.md`.

**Deny promotion** if no independent model-training evidence, no reliable exact-source validation, no positive retrieval against matched baselines or no real physical resource measurements.
