# DACR-1 — frozen exact-key/source-address reference protocol

**Research scope:** source-verified direct addressing only. **Protocol frozen before implementation and tests.** No claim about language quality, learned latent addressing, circuit routing, constant physical I/O, 25B scaling, or end-to-end attention replacement.

## Ownership and immutable boundaries

This additive research slice lives under `sml-store::dacr_exact`. Existing CONTEXT-0/1/2/3, MOR, DKV-2b and B1 evaluation contracts are unchanged. Source identities and raw bytes belong to CONTEXT-0/1c; the exact-key index is an **untrusted hint**, not a new source authority. No persistent artifact format, GPU dependency, remote host mutation, or default decoder path is introduced.

- Source identity: `ContextStreamId` and `ContextSegmentId` from CONTEXT-0.
- Index generation: explicit caller-owned nonzero epoch. Neither stream IDs nor epochs may be silently switched.
- Binding: exact **nonempty** key bytes, source segment ID, and immutable `ContextContentHash`; only canonical byte equality counts as an exact-key match.
- Address: deterministic stable FNV-1a 64-bit over key bytes, masked to `bucket_bits` in [1,16]. It is *not cryptographic* and must never be used as source authentication.
- Bounded index admission: maximum entry count, key bytes per entry, entries per address bucket, provenance artifact length, raw artifact size. Unknown and malformed limits are rejected before mutation.
- Collision: addresses are hints; full key bytes must match. Distinct keys with same address coexist up to a frozen cap. A bucket overflow rejects insertion **without altering existing entries**.
- Same key bound to different source segments: `Ambiguous`, not a silent chosen winner, even if the payload happens to agree. Exact duplicate binding rejected. Explicit supersession policy is out of scope.
- Verification: before returning a hit, verify manifest stream/segment, hash and semantic-ID linkage against the existing CONTEXT-1c manifest; load the immutable raw segment using `raw_context::load_raw_segment`; verify segment identity, source hash, semantic IDs, and literal key occurrence in returned bytes. SHA-256 is content integrity, **not access authorization or a digital signature**.
- Missing/corrupt/stale artifacts, mismatched source or absent literal occurrence -> error (not Hit). Missing key -> Miss. Missing/corrupt provenance -> error. No scan of all indexed keys on the query path.

## Primary fixture protocol (not confirmatory quality evidence)

Use tiny deterministic, local temp-directory fixtures; no private corpora. Each raw file is built through the existing immutable raw CONTEXT artifact builder, and every provenance file through the CONTEXT-1c serializer. Unit/integration tests use exact declared key bytes and the same original files; no normalization or language-specific tokenizer.

Tests must include:
1. One indexed source: exact key hit plus verified raw bytes.
2. Unseen key: explicit miss, no raw read.
3. Two *different* keys intentionally sharing an address: only byte-exact source returned; compare counter shows work and remains bounded.
4. Duplicate same binding: reject without mutation.
5. Two segments for one exact key: ambiguity (no arbitrary source selection).
6. Per-bucket cap overflow and total-cap overflow: preflight rejection; old bindings still retrievable.
7. Wrong stream, wrong epoch, and wrong binding segment: fail closed.
8. Missing provenance, truncated/corrupt provenance and missing raw: explicit errors.
9. Raw content/hash tampering and binding hash mismatches: explicit errors.
10. Indexed key absent from the otherwise valid source bytes: error.
11. Nonzero limits, key-length cap, zero key, and any malformed source identity rejected.
12. Query determinism and per-query work accounting: bucket entries visited and full-key comparisons, raw reread count/bytes; never inferred from elapsed time.

Use `cargo test --locked -p sml-store dacr_exact` for the slice, plus `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace`; CI must pass on the **exact final SHA**. Any failure stays open. An independent exhaustive scan may be added later as a **control only**, not as part of the candidate query path.

## Evidence and promotion rules

This is a *structural correctness* gate, not a trained-model/quality study. A passing test proves only exact-key lookup on the frozen synthetic fixtures. Report explicit limits: index is constructed using O(N) materialization; BTreeMap key lookup, FNV hashing, full-key byte comparisons, cold raw reread, filesystem metadata and page faults all cost resources. The maximum bucket work is bounded by the declared cap, but no general constant-time or throughput guarantee follows. For source-exact tasks, the raw-source bytes remain the authority and must be reread; verified retrieval cannot be inferred from a hash alone.

**Next gates, separate work:** DACR-2 learned discrete latent-key candidate and matched fixed-routing controls, trained/DEV/TEST separation; DACR-3 native MOR circuit-page activation; DACR-4 fully integrated learned model and measured quality/resource parity. Do not change the frozen LONGCTX-B1 thresholds or access its protected TEST to tune DACR.
