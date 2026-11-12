# BPB-3-A8 — 256-weight shared dyadic base: held-out result, 2026-11-12

**Status: research-only.** Rust correctness **PASS**. Fully costed 108B/256 reconstruction and paired-dot improvement versus A6-delta2 **PASS** on the frozen four new source strata. Competitive parity or superiority to official native Q3_K **FAIL**. No change to production inference.

## Frozen protocol and source provenance

- Prospective [A8 protocol](BPB3_A8_SUPERBLOCK_PROTOCOL_20261112.md) committed **before source evaluation** at `837006c9e6ccf0101e8aa3fc5c137f7718076016`.
- Original pretrained public Apache-2.0 [SmolLM2-135M](https://huggingface.co/HuggingFaceTB/SmolLM2-135M) BF16 weights, pinned HF revision `93efa2f097d58c2a74874c7e644dbc9b0cee75a2`, exact verified original safetensors SHA256 `80521b40281d6ce74e35c9282c22539e75aa0ac8578892b2a59955ef78d55da1`.
- Fresh, preregistered trained matrices: `model.layers.28.self_attn.o_proj.weight`, `model.layers.28.self_attn.q_proj.weight`, `model.layers.29.mlp.up_proj.weight`, `model.layers.29.mlp.down_proj.weight`. Earlier A3 layers2/3, A4 6/7, A5 10/11, A6 18/19, A7 24/25 TEST sources were **not reused**. All are layers of **one model**, not a new architecture.
- SHA-seeded deterministic selection from each original matrix: 24 nonoverlapping DEV and 24 disjoint TEST 256-weight contiguous rows, 6,144 TEST coefficients per tensor. SHA provenance and exact indexes in the source manifest. The original stored weight bytes were **not uploaded to artifacts**.
- All arms operate on the same FP32-widened original source and **16 identical xorshift64* bipolar masks per row**, yielding 384 signed dot samples per matrix, **1,536 paired 256-weight dots per codec**. Native controls are the original untouched ggml Q3_K/Q4_K C encoders at commit `f7591a6a01f9273de03331732bdc0b39721168be` with independent C/Rust decoder cross-check.
- [GitHub A8 CI run 43282901056](https://github.com/Memorithm/SML-GENIUS/actions/runs/43282901056), two hosted CPU jobs **success**; Rust 1.89 **6 tests passed, 0 failed**. Tests check 256-weight canonical wire, negative zero and invalid bases/modes/exponents, independent signed POPCOUNT/dense integer dot, exact checked i128 dyadic sum, local DP minimum for all-mode1 and outer global DP against brute force.
- [Raw result CSV, SHA manifest and admission evidence, artifact 13973481979](https://github.com/Memorithm/SML-GENIUS/actions/runs/43282901056/artifacts/13973481979). `a8-all-results.csv` SHA256 `2ecb531d74ed9942c239ef7c2b0b3bcb9c75f18af25ea815edbaff84e18e4a16`. `a8-source/selected/manifest.json` SHA256 `c52d165e80d920a53c91e93f87ac1038a3aa3840003b5359e5d6f24f64d8f72b`.

## Source algorithm and actual storage

A8 packs **one** dyadic base for a *256-coefficient* superblock, **16 two-bit local signed exponent offsets**, 16 independent magnitude plane modes, 256 sign bits and 16 groups of 16-bit magnitude planes. Exact cost:
- base: 1 byte
- local exponent offsets: 4 bytes (16 × 2 bits)
- 16 precision mode codes: 4 bytes (16 × 2 bits)
- 256 sign bits: 32 bytes
- unsigned magnitude planes: 2 × sum(mode[g]) bytes

**Total = 41 + 2 Σmode[g] bytes per 256 original weights**, min 73 and max 137 per superblock. Global exact DP over 24 independently readable superblocks enforces **24×108 = 2,592 bytes per original tensor split**, counting every stored byte; per-row allocation can vary. All four measured TEST tensors used exactly 2,592 bytes per24 rows, 108B/256. By comparison, the A6 2-bit scale scheme uses one common base per64 coefficients, **four** bases per256, costing 8 scale bytes per256; A8 uses **5 scale bytes per256**, saving 3 bytes for extra magnitude precision at the same global 108B/256 budget. Sharing one base across 16 groups is an additional potential precision restriction.

## All four frozen TEST matrices

| Source tensor | A8 weight RMSE | A6 delta2 weight RMSE | Official Q3_K weight RMSE | A8 matched dot RMSE | A6 matched dot RMSE |
|---|---:|---:|---:|---:|---:|
| attention_output_layer28 | **0.041332443381** | 0.044277685400 | **0.033080156901** | **0.672143376660** | 0.740402402805 |
| attention_q_layer28 | **0.025526693348** | 0.027251937587 | **0.021706201708** | **0.392904236967** | 0.430356764365 |
| mlp_up_layer29 | **0.042366297446** | 0.045490084404 | **0.033776463530** | **0.707829193010** | 0.770896594797 |
| mlp_down_layer29 | **0.036357380906** | 0.038566066800 | **0.031542545551** | **0.600544470927** | 0.635119599214 |

All four source TEST strata retained as preregistered. The A8 improvement over A6-delta2 is positive for all four, but official Q3_K is still more accurate for all four. These comparisons are paired at each original source coefficient and input mask, not comparing different model checkpoints.

## Pooled TEST, weighted by original number of samples

Pooled RMSE is `sqrt(sum(n_i × matrix_RMSE_i²) / sum(n_i))`, not an arithmetic average. 24,576 original TEST weights and 1,536 signed bipolar dot samples per codec.

| Codec | Real B/256 | Pooled weight RMSE | Pooled signed-dot RMSE |
|---|---:|---:|---:|
| Tagged A1 fixed | 108 | 0.047126459234 | 0.754956148983 |
| A1 adaptive | 108 | 0.044453189810 | 0.730091056493 |
| A6 delta4 | 108 | 0.043375294964 | 0.696801285958 |
| **A6 delta2** | **108** | **0.039559672692** | **0.657848981129** |
| A6 mixed | 108 | 0.039559672692 | 0.657848981129 |
| **A8 256-weight shared base** | **108** | **0.037002517667** | **0.605769404611** |
| Native Q3_K | 110 | **0.030418898902** | **0.485638390610** |
| Native Q4_K | 144 | 0.014475260727 | 0.228833425151 |

A8 improves against matched-budget A6-delta2 by **6.46% source weight RMSE** and **7.92% bipolar-dot RMSE**. A8's weight RMSE remains **21.64% higher** than the stock Q3_K's at only 2B/256 less storage, while its dot RMSE is **24.74% higher**. This is a **new component-level improvement**, not competitive numeric-quality parity with native K-quantizers.

## Scientific and engineering decision

Retain **A8 as the new best measured BPB-3 research variant** for the frozen source cohort, subject to additional independent-model validation. A6 remains the previously reproducible fallback; A4/A5/A7 negative results are explicitly retained, not cherry-picked away.

The next development gate is a **whole-model** evaluation with native A8 weight storage and signed Boolean dot kernels, pinned tokenizer and held-out text perplexity, generated-token fidelity, CPU decode latency, throughput, RSS and actual memory traffic (read-only isolated physical workloads through Memorithm/RemoteOps only if admitted). Comparing source RMSE alone does not establish task-level quality, bandwidth advantage, energy reduction or compliance with Posit Standard. No physical Dell/Thor/Mac workload used.
