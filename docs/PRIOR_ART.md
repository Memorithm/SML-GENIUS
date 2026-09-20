# Prior-art anchors for SML research

SML-GENIUS does not claim novelty merely because it combines recurrence, low-bit weights, adaptive computation, direct routing or Boolean logic. Each of those areas has substantial prior work. This file records initial anchors that the programme must compare against and expand before any novelty statement.

## Adaptive / recurrent computation

- Alex Graves, "Adaptive Computation Time for Recurrent Neural Networks", arXiv:1603.08983 (2016).
- Mostafa Dehghani et al., "Universal Transformers", arXiv:1807.03819 (2018).
- Bo Peng et al., "RWKV-7 Goose with Expressive Dynamic State Evolution", arXiv:2503.14456 (2025).
- Tri Dao and Albert Gu, "Transformers are SSMs: Generalized Models and Efficient Algorithms Through Structured State Space Duality", arXiv:2405.21060 (2024).

## Boolean / low-bit learning

- Felix Petersen et al., "Deep Differentiable Logic Gate Networks", arXiv:2210.08277 (2022).
- Shuming Ma et al., "BitNet b1.58 2B4T Technical Report", arXiv:2504.12285 (2025). BitNet remains a Transformer and is a low-bit baseline, not the SML architecture.

## Storage-aware inference

- Keivan Alizadeh et al., "LLM in a Flash: Efficient Large Language Model Inference with Limited Memory", arXiv:2312.11514 (2023).
- Xun Chen et al., "PowerInfer-2: Fast Large Language Model Inference on a Smartphone", arXiv:2406.06282 (2024).

## Memorithm-specific predecessors

SML must explicitly reuse or compare against, rather than silently duplicate:

- TDI-21 Boolean relational architecture programme;
- TDI-8 recurrent associative architecture / ASSR programme;
- TDI-9.3 Boolean policy synthesis;
- SciRust tensor representation and exact StorageBits accounting;
- MorphoDiff differentiation experiments;
- ElasticXxx resource/residency planning;
- Forge execution-authoritative algorithm search;
- FLAT-ATTENTION multi-algebra research as a baseline/research source, not as SML attention.

This list is a starting point, not a complete novelty search.
