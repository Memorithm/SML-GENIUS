# BAF-0 — Native Boolean associative focus, not Transformer attention

**Status:** isolated scalar research oracle; no model/runtime promotion, training claim, speedup claim or quality claim.

## Architectural decision

An SML attention-like **selection of relevant state/memory** need not use numerical query/key/value vectors, cosine similarity, softmax or a token-by-token attention matrix. For this research arm, an ANF predicate directly gates each candidate Boolean memory page using the current event bits and recurrent SBG state bits:

```text
event bits + recurrent SBG state
       |
       v
ANF activation predicates (GF(2), XOR of AND monomials)
       |
       v
deterministic bounded page admission
       |
       v
priority-controlled masked Boolean state rewrite
       |
       v
next recurrent SBG state
```

For page `j` with Boolean input `e` and state `s`:

```text
a_j(e,s) = bias_j XOR XOR_{m in monomials_j}
                     (all(e & m.event_mask) AND all(s & m.state_mask))
```

Here `all(x & mask)` means `(x & mask) == mask`. The zero-mask monomial is constant one. The selected-page list is sorted by **descending declared priority**, with `PageId` as an ascending tie break. Only activated pages are selected, up to a configured fixed cap. A page's write mask claims only still-unclaimed bits. Earlier higher-priority pages win conflicts; the original state is preserved everywhere else.

This is **not** quantized Q/K attention and is not a drop-in replacement for a Transformer attention layer. The word "focus" denotes logic-based conditional memory access, not normalized numerical importance weights.

## Deliverable and limits

- Self-contained model-owned Rust module `sml_core::boolean_associative_focus`; no FLAT-ATTENTION runtime dependency, no CUDA, no FFI, no external library.
- Pure deterministic `step(event, state)`, with explicit selected pages and claimed bits for inspection.
- Validation rejects malformed widths, duplicate page IDs, out-of-width predicate/write bits, write values outside masks and out-of-width runtime inputs.
- Tests include exhaustive small-state monomial semantics against an independent bit-by-bit oracle, non-linear ANF, conflict resolution, admission cap and no-match retention.
- Fixed research width `1..=64` for both event and state. This does not establish scalability to SML-25B.
- The current implementation **does not learn monomial supports, page priorities, page contents or state update masks**. Trainability is future work, not an accomplished result.
- Sequential page scan is up to `O(sum of examined predicate monomials + writes)`; **no O(1), linear-in-context, throughput or memory advantage is claimed**.
- A state rewrite with OR-like bit semantics can differ from attention's weighted sum. Conflict policy and information preservation must be evaluated explicitly.

## Qualification plan (next experimental slices)

1. Freeze an independent symbolic oracle and demonstrate parity on held-out random multi-page states and 64-bit boundaries, including zero masks and all-clause parity.
2. Learn page activation and update rules on **train only**; reserve disjoint validation/test/OOD sequences. No oracle labels for hidden rules.
3. Compare under matched task/data/compute budgets with SML's existing `hybrid_logic_engine` XOR/Hamming route, native MOR + ANF recurrent baseline, unconditional reads, and an appropriately controlled numerical attention model.
4. Report exact recall, delayed recall, interference/conflicts, negative controls, convergence, state capacity, bits of **actual** storage including page metadata, RSS, measured latency and energy when hardware counters are reliable.
5. Prototype SIMD/WGPU and indexed admission **only after** the scalar correctness/quality and cost gates. Keep the final SML model self-sufficient; qualify portable helper primitives separately through SciRust/FLAT/RemoteOps.

## Repository ownership

SML owns the research semantics and eventual model decision. SciRust may host reusable packed monomial evaluation after an explicit reviewed contract, FLAT may prototype portable GPU kernels without changing its qualified dense numerical attention, and RemoteOps controls native measurement. Existing SML-GENIUS and FLAT-ATTENTION roadmaps remain authoritative. Do not merge the private off-main roadmap into `main`.

**Research gate:** complete exact-head `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings` and `cargo test --locked --workspace` before merging; no deployment/default selection without independent evidence.
