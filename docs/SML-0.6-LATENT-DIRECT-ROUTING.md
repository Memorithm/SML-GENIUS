# SML-0.6 latent direct routing

Status: executable reference trainability experiment. It is not evidence of model quality, natural-language generalization, throughput, scaling, or architectural novelty.

## Objective

SML-0.6 makes the existing direct page-routing table trainable without changing the hard inference contract.

For an address width `A` and page cardinality `P`:

- route entries: `2^A`;
- semantic route width: `ceil(log2(P))` bits;
- learned Boolean route parameters: `2^A * ceil(log2(P))`;
- continuous latent scalars during training: one per eventual route bit.

`RouteLearningBudget` rejects address width, page count, and learned-bit totals before allocating the latent table. The default reference ceiling is 12 address bits, 4096 pages, and 49,152 learned Boolean route bits.

## Training contract

`LatentDirectRouter::train_complete` requires exactly one caller-owned target `PageId` for every address. Targets outside the declared page cardinality fail closed.

Each eventual hard route bit has one training-only logit. Binary cross-entropy is optimized deterministically against the complete route oracle. The hard bit is `logit > 0`.

This experiment deliberately uses a complete supervised oracle. It does not yet learn routing indirectly from downstream task loss and does not demonstrate routing generalization to unseen addresses.

## Hard freeze and runtime boundary

`freeze()` converts every learned address to a discrete `PageId` and constructs the existing `DirectRouteTable`. Inference then performs one low-address extraction and one direct table lookup; it does not scan token history, rank candidates, compute all-pairs scores, or invoke attention semantics.

The reference smoke verifies all four addresses of its oracle and passes the execution counters through the non-attention audit gate.

The learned-bit count is a semantic parameter count, not the physical memory footprint of the current Rust reference table. `DirectRouteTable` currently stores a `Vec<PageId>` and `PageId` wraps a `u32`; no bit-packed route-table implementation is claimed by SML-0.6.

A future packed runtime must report its own payload, metadata, alignment, RAM traffic, and decode-performance measurements separately.

## Evidence produced by this slice

The executable tests establish that:

- a bounded direct router can learn a complete synthetic route oracle deterministically;
- structural budgets are enforced before latent allocation;
- malformed oracle lengths and out-of-range pages are rejected;
- exact training work counters are reproducible;
- hard freeze preserves the learned mapping;
- frozen routing keeps pairwise-comparison and attention-score counters at zero.

This closes the SML-0 learned-direct-routing reference primitive only. The SML-1 gate still requires task-level experiments showing whether learned routing and memory improve held-out task performance against declared ablations.
