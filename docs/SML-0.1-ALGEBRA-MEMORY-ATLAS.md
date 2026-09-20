# SML-0.1 algebra, memory and circuit-atlas contract

Status: executable reference semantics. This document does not establish language-model quality, hardware speedup or architectural novelty.

## Purpose

SML-0.1 closes four reference-level gaps needed before training experiments:

1. deterministic GF(2) binding/address transforms;
2. canonical algebraic-normal-form (Zhegalkin) relation composition;
3. bounded associative memory with explicit collision semantics;
4. directly addressable circuit pages with exact semantic and physical accounting.

The design follows the non-attention boundary already frozen by SML-0 and the requirements exposed by Memorithm TDI-21/TDI-8: no history scan, explicit hits/misses/collisions/replacements, and counters that can prove pairwise token scoring stayed absent.

## GF(2)

`Gf2Vector` stores coefficients bit-packed. `Gf2LinearMap` stores output rows and evaluates matrix-vector products as parity dot products.

The implementation may use machine-word parallelism, but `gf2_ops` records semantic scalar operations so experiments can compare algorithms without pretending one machine XOR instruction equals one mathematical operation.

No GF(2) operator performs token-pair ranking.

## ANF / Zhegalkin

An ANF polynomial is represented as:

    c XOR m_0 XOR m_1 XOR ... XOR m_k

where every monomial is a square-free product of strictly increasing variable indices.

Canonical construction rejects:

- empty monomials (the constant has its own field);
- repeated or descending variables;
- duplicate/non-sorted monomials;
- variables outside the declared input width.

This prevents multiple encodings of the same polynomial from contaminating parameter and search accounting.

## Bounded associative memory

`BoundedAssociativeMemory` has fixed capacity and exactly one resident entry per direct address. A deterministic 64-bit mixer maps a key to one slot in O(1) time.

A read returns one of:

- `Hit`;
- `EmptyMiss`;
- `KeyMismatch`.

A write returns one of:

- `Inserted`;
- `Updated`;
- `CollisionRejected`;
- `Replaced`.

Replacement is an explicit policy. No read or write scans historical tokens or searches other slots.

The canonical semantic-memory bit count is deliberately separate from Rust heap size. Per slot it includes:

    occupancy:   1 bit
    key:        64 bits
    write epoch:64 bits
    value:       V bits

Global metadata includes capacity, value width, replacement policy and next-write epoch. This gives experiments an auditable memory budget before any optimized representation exists.

## Circuit Atlas pages

`AtlasPageManifest` records:

- dense `PageId`;
- learned low-bit parameter budget;
- explicit non-parameter metadata bits;
- physical alignment.

For a page:

    semantic_bits = parameter_bits + metadata_bits
    physical_bytes = align_up(ceil(semantic_bits / 8), alignment)

`CircuitAtlas` requires IDs 0..N-1 in order, so activation is a vector index rather than a scan or hash-map lookup.

Activation records exact:

- page activations;
- learned parameter touches;
- semantic storage bits touched.

These counters are distinct from recurrent-cycle totals to prevent accidental double counting.

## Non-attention audit gate

`validate_non_attention_candidate` fails closed if either:

- `pairwise_comparisons != 0`;
- `attention_score_ops != 0`.

Direct key equality inside one addressed memory slot is counted separately as `memory_key_comparisons`; it is not a token-pair search.

## Next gate

SML-0.2 must bind recurrent cycles to immutable trace records containing:

- token index;
- recurrent depth;
- selected page IDs;
- active learned parameters;
- active semantic storage bits;
- workspace transition distance;
- contradiction state;
- candidate-audit counters.

Only after those traces exist should SML-1 task generators and trainability experiments be treated as evidence-producing surfaces.
