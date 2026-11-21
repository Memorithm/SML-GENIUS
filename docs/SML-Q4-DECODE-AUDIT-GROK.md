# SML Q4 decoder audit and top-k conformance oracle (Grok)

Scope: `QuantizedSparseAtlas` Q4 next-byte decoding as merged in #1011/#1012
(main `6482c237`), both `ByteFactorization` modes. All evidence is
**synthetic** (seeded random Q4 atlases); no DEV/TEST data and no trained
weights were read. Real S2a weights stay on the Dell; nothing here
re-measures the S2a DEV values (f32 3.865978150 / Q4 4.007063135 bpc).

## Deliverables

- `crates/sml-core/src/q4_decode_reference.rs`: independent oracle. It
  re-implements routing, slots, nibble decoding, logistic and both
  factorization paths from the written spec (closed-form tree nodes,
  breadth-first tree mass, stable log-sigmoid, canonical summation), with
  no call into the production decoder. It provides the full 256-way
  distribution (linear and log2), exact top-k (`exact_top_k`, ties by
  ascending byte), an alternative exact best-first top-k for the MSB tree
  (`msb_tree_top_k_best_first`), and the conformance checker
  (`check_topk_conformant`, `check_topk_impl`, `assert_topk_conformant`).
- `crates/sml-core/tests/q4_decode_reference.rs`: differential tests, one
  `f<N>_` test per finding below.

## Findings

| # | Severity | Finding | Evidence | Fix |
|---|---|---|---|---|
| F1 | Pass | Both factorizations are normalised: sum = 1 within 1e-12 (linear and 2^log2). MsbTree is the exact chain rule over the 255 internal nodes `1..=255`, with no off-by-one. | `byte_factorization.rs:33-38`; `f1_*`, `f2_decision_paths_*`, unit `closed_form_tree_nodes_*` | none |
| F2 | Pass | Production head probabilities equal the oracle within 1e-15, including LRU caches of 1, 2 and 32 pages with evictions. All 256 byte scores agree within 1e-7 bits. Routing is injective for head < 256 and stays in range, including contexts (0,0) and (255,255). | `sparse_gradient_atlas.rs:120-129,858-881`; `f2_*` | none |
| F3 | Pass | MAP equals the oracle argmax (880 cases, away from ties). IndependentBits per-bit greedy equals the joint MAP by construction. MsbTree greedy descent differs from MAP, so the 256-path enumeration (or an exact search) is needed. | `:891-925`; `f3_*` | none |
| F4 | Pass | Q4 layout: the even slot is the low nibble; levels are two's complement with step 0.5 and no zero point; rounding is half away from zero. Export and load agree. | `:745-755,:867-878`; `f6_*` | none |
| F5 | Medium | Asymmetric clipping. The master is clamped to [-4, 4] (`:406,:447`), but the Q4 grid is [-4, +3.5]. A weight in (3.75, 4] becomes 3.5, an error up to 0.5 (twice the 0.25 bound elsewhere), one-sided and only for positive saturated weights. It may contribute to the +0.141 bpc f32→Q4 gap; **not measured**. | `:752-753`; `f6_*` | Proposal only. It needs a versioned Q4 format and changes the S2a Q4 DEV value, so it is left to GPT-6 for decision. |
| F6 | Medium | One global step of 0.5 and no per-page or per-block scale: every learned weight with \|w\| < 0.25 becomes 0. This is a likely main contributor to the gap; **not measured**. | `:752-753`; `f6_*` (0.2 → 0) | Same as F5 (per-page scale or symmetric grid plus a header). Needs a Dell weight histogram first. |
| F7 | Medium (perf) | MsbTree MAP evaluates 255 heads, about 250 distinct pages per byte. Against the CLI's 32-page LRU (`bin/sml-q4-byte-generate.rs:76-80`) this cyclic scan is the LRU worst case: repeating the same context reloads at least `distinct - 32` pages. | `:891-900`; `f7_*` | Alternative: exact best-first search. On synthetic atlases, top-1 read 8-24 heads (peaked), 8-20 (uniform) and 32-78 (sparse), against 255. It is not in the engine and needs a Dell measurement on real weights. |
| F8 | Low | The headerless Q4 file silently accepts the wrong factorization; only the file length is checked. | `:800-824`; `f8_*` | Header or manifest. This overlaps with the Q4Checkpoint loader work; no change here. |
| F9 | Low | Greedy MAP generation on a 2-byte state is a deterministic map over 65,536 states, so it is always eventually periodic (synthetic period: 95). Top-k or sampling is required for generation. | bin `:85-96`; `f9_*` | Ripple top-k plus a sampler |
| F10 | Info | The first two bytes of each document are never scored (n = len - 2; documents under 3 bytes give n = 0). This is identical for f32 and Q4, so the gap comparison is fair. | `:927-939`; `f10_*` | none |
| F11 | Pass / Info | Invalid heads and pages are rejected (`:827,:859,:370`). Q4 has no `probability_on_page`, so the trained MOR bridge (`sparse_mor_anf_recurrent.rs`) cannot run on Q4. | `f11_*` | API gap, for GPT-6 to decide |
| F12 | Low (bug) | The lowest-byte tie rule is violated on mathematical ties. Q4 logits are multiples of 1/8, so equal factors σ(z) and 1-σ(-z) are computed as different f64 values and summed in path order. In 8 of 474 exact MAP ties (1.7%) over 8,000 synthetic 1-page MSB atlases, the decoder returned a higher byte. Equality of the signed-logit multisets is asserted. | `:902-915`; `f12_*` | Separate PR: symmetric log-sigmoid terms and a canonical sum. `evaluate_document` is untouched, so the DEV 4.007063135 value is unchanged. |
| F13 | Info | `log2(1-p)` cancels at saturation, an error of about 1.2e-8 bits per byte. This is harmless for bpc, but cross-implementation conformance needs tol ≥ 1e-8 (`DEFAULT_TOL_BITS = 1e-7`). | `:907-908`; `f2_*` | Fixed in the F12 PR for `predict_next_byte` only |
| F14 | Pass | Saturation: the ±16 logistic clamp is reachable (raw \|z\| up to 17), stays finite and normalised, and the minimum byte probability is ≥ 2^-185. | `:152-154`; `f5_*` | none |

## Top-k conformance contract (for Ripple and any other implementation)

An implementation is a closure `impl_fn(a, b, k) -> Result<Vec<(u8, f64)>, E>`:

1. It returns exactly `min(k, 256)` entries; `k = 0` returns an empty list.
2. A score is the **log2 probability** of the byte under the frozen Q4 atlas.
3. Scores are within `tol` (default `1e-7` bits) of the oracle.
4. Entry `i` has the oracle score of the oracle's rank `i` within `tol`.
5. Ties (`TieRule::LowestByte`): bytes with exactly equal model probability
   come in ascending byte order, including across the k-th boundary.

```rust
use sml_core::q4_decode_reference::{assert_topk_conformant, ReferenceQ4};
let oracle = ReferenceQ4::from_bytes(std::fs::read(&q4_path)?)?;
assert_topk_conformant(
    |a, b, k| ripple_top_k(&mut atlas, a, b, k), // (byte, log2 p) list
    &oracle, ByteFactorization::MsbTree, (a, b), 16,
);
```

`assert_topk_conformant` checks `k ∈ {0, 1, 2, 5, k, 255, 256, 300}`. Use
the random-atlas families in `tests/q4_decode_reference.rs` (uniform,
sparse, saturated ±, peaked) and both factorizations.
