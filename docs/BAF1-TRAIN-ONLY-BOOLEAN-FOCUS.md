# BAF-1 — Task-loss-driven ANF memory-focus learning

Status: opt-in research-only scalar trainer. No production routing activation.

## What is learned

BAF-0 selects native Boolean memory pages through ANF predicates over raw event and recurrent state bits, with no query/key/value projections, similarity vectors, token-pair attention, softmax or matrix multiplication. BAF-1 adds a **train-only supervised learner** that changes coefficients in one page's activation ANF by minimizing **observed final next-state Hamming error**. It does not receive hidden route labels, teacher gate identities, or desired attention maps.

For each current event `e`, current state `s`, and observed final target next state `s'`, the training loss is:

```text
L_train(A) = sum_train popcount(BAF0(A,e,s).next_state XOR s')
```

The feature bank is deterministically enumerated from input bits: all degree-1 monomials and, optionally, all degree-2 pairs. Each round toggles the ANF constant or one monomial coefficient (XOR / GF(2)), evaluates entire original BAF-0 page execution, then admits the first strict improvement in fixed enumeration order. Stopping and feature bank bounds are explicit. Max total event+state input dimensions: 16 for this baseline; model-state and stored pages remain constrained by BAF-0.

An independent **validation** split only vetoes the final frozen train winner if validation Hamming loss worsens from the unchanged baseline. It does not select features during training. No final **TEST/OOD** set enters this API: final quality must be evaluated separately and prospectively.

## Non-claims and limitations

- Only **one page's activation** is fitted; write mask/value, priorities, other page activations and admission cap stay fixed. No joint page learning or generalized language training.
- This is a deterministic greedy coordinate-flip reference, **not** proof of optimal monomials, gradient-based training, convergence, recoverability, or superiority to Hamming routing.
- Requires observable next-state targets. A masked write that leaves the observed state unchanged is not identifiable from that transition alone.
- Train and validation have disjoint case IDs **and** disjoint exact (event,state) input pairs; malformed widths, duplicate IDs, overlap, unsupported degree or oversized basis fail closed.
- On validation regression, the unchanged baseline model is returned, alongside honest candidate errors and `admitted=false`.
- Some tasks need higher-degree ANFs, page-content learning or temporal credit assignment; a degree-1 negative-control test deliberately cannot express a degree-2 conjunction.
- No timing, RSS, energy, GPU, 25B capacity, quality or runtime acceleration claims. Each must use matched independent and real-device evaluation.
- The final SML model remains self-sufficient and is not dependent on FLAT-ATTENTION, SciRust or a vendor software SDK.

## Reproduction

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo test --locked -p sml-core boolean_associative_train
```

Merge only on green exact-head full CI. Future research should compare BAF-1 against existing `hybrid_logic_engine` Hamming routing and native MOR+ANF controls with matched train/DEV/TEST splits, matched training compute, memory and latency accounting.
