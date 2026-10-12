# A6-E1 — Native results: candidate-scoped input novelty on real French text

**12 October 2026. Fully executed first preregistered natural-data experiment. NEGATIVE model-utility result (STOP), successful engineering and scientific checks.**

Precommitted experiment design: [A6-E1-REAL-BYTE-SUPPORT-RISK-PREREGISTRATION.md](A6-E1-REAL-BYTE-SUPPORT-RISK-PREREGISTRATION.md), commit `938055047b49340bb634a8b55076f87afde82a0f` BEFORE any A6-E1 code. First successful real-data run: [GitHub-hosted job 114898905052, run 38275221470](https://github.com/Memorithm/SML-GENIUS/actions/runs/38275221470/job/114898905052), source SHA `46af6e5049cb48e2caeffb5e87c2969cca530dc8`, Rust 1.89.0 on Ubuntu24.04 x86_64, 4 logical CPUs visible to runner. The pinned corpus was fetched from Project Gutenberg #800 via existing `scripts/fetch-char-corpus.sh`: `433036` bytes, SHA256 `b51f36fb2baf0847fd3632fbd6ff939b8858abc1d728b5f86c94d99e64766a74` **confirmed by CI**. Download/hashing errors would have failed rather than falling back to fabricated data.

## 1. Precise real-world task and split

Binary event only: will the next UTF-8 byte be ASCII space `0x20`? The learner reads the **previous two bytes** (16 Boolean bits). The native ngram baselines use their original count tables, trained only on the same TRAIN split, with history lengths 2 and 4 bytes. All use contiguous 80/10/10 train/validation/test splits and 64 independent lane chunks, with a two-byte warmup per chunk. Measured TRAIN count `346240`; separate VAL `43136` and heldout TEST `43136`. The frozen real test split was scored only after model, support, probability posterior, distance index and VAL-selected policy were frozen. This is an auxiliary real-language binary task and **not** the full next-byte modeling task measured in A5. Its bits/event MUST NOT be compared numerically to the previous 2.019 bits/byte result.

Four Boolean candidates were **learned from TRAIN** using nested greedy leave-one-out KT conditional scores over all 16 eligible past input bits. Their independently compiled `WideAnfGate` supports and active monomial counts:

| Fan-in k | Learned physical bits | Active GF(2) ANF monomials | TRAIN leave-one-out bits/event |
|---|---|---:|---:|
| 2 | [2,14] | **0** | 0.538236 |
| 3 | [2,4,14] | **0** | 0.515777 |
| 4 | [0,2,4,14] | **0** | 0.494942 |
| 5 | [0,2,4,6,14] | **16** | 0.457768 |

Each native Boolean majority table is checked against all `2^k` inputs after Möbius ANF inversion, independent of unseen TEST values. The first three candidates collapse to the **constant-zero function**; only k5 has nontrivial coefficients. Four native gates with heterogeneous fan-ins share the real global 16-bit source through the opt-in `PassiveCommittee::new_with_source_width`; historical `PassiveCommittee::new` remains unchanged.

The posterior over hard gate outputs produces near-deterministic 0/1 forecasts. The preregistered numerical `10^-6` clipping merely makes log-loss finite and **does not calibrate the posterior**; catastrophic probabilistic loss below is therefore a genuine weakness, not a reporting error.

## 2. Strict matched TEST comparison — STOP

| Method, on exactly 43 136 heldout binary events | Binary log-loss (bits/event) | Accepted predictions | Wrong classifications where measured |
|---|---:|---:|---:|
| Ngram4 Witten–Bell, always predicts | **0.21751656** | 43 136 | 3 236 |
| Ngram2 Witten–Bell, always predicts | 0.30636244 | 43 136 | 4 590 |
| TRAIN majority event base rate | 0.58551026 | 43 136 | 6 060 |
| ANF native four-gate posterior, unguarded | **2.52933653** | 43 136 | 5 474 |
| **VAL-selected ANF support/Hamming + ngram4 fallback** | **2.52734408** | **43 091 ANF; 45 fallback** | 5 474 on full fallback decisions; 5 469 errors among accepted ANF decisions |

The frozen grid comprised `confidence in {0.50,0.90}`, `strength in {2,3}`, `max_new_patterns in {0,1}`, `max_Hamming16 in {0,1,2,4}` = **32 policies**; each was independently reported for both VAL and TEST. The VAL-selected best policy was **index 0**: confidence≥0.50, strength-2 scoped support with zero missing-pattern tolerance and Hamming≤0. On TEST, it covers **43 091/43 136 = 99.895679%**, refusing only 45 requests. Its selective error is **5 469/43 091 ≈12.691745%**, with a full-fallback loss of **2.52734408 bits/event** — dramatically worse than ngram4.

**All 32 TEST policies** have coverage between **0.99895679 and 1.00000000**, and full-fallback loss between **2.52734408 and 2.52934442 bits/event**. Thus neither strict scoped-pattern coverage nor full-input Hamming changes the model's real generalization risk substantially. Across TEST every real candidate-scoped pair/triple pattern was covered by the TRAIN inputs; the standalone scoped pair/triple controls accept 43 136 predictions, produce 5 474 wrong classifications and equal the unguarded **2.52933653 bits/event** loss. An exact Hamming distance of zero rejects only 45 inputs and yields 2.52734408, and radius one rejects four inputs and yields 2.52934442.

The preregistered binary-task criterion was a full-fallback TEST advantage of at least **0.02 bits/event** against ngram4 **with coverage >=80% and valid resource caps**. Coverage and resource checks succeed, but the actual loss is nowhere close. **A6-E1 scientific verdict: STOP / not promotable.** This is a valid negative scientific result and must not be turned into a positive by choosing TEST thresholds or counting abstentions as correct.

## 3. Rank and resource evidence, with rigorous limits

Selected union of the **real candidate gate input positions** has s=5 within global N=16, so the scoped scans track just **C(5,2)=10 pair tuples** and **C(5,3)=10 triple tuples** (each 2^t patterns). Their logical retained payload is **360 bytes each** on the measured 64-bit ABI, including stored support and tuple records but excluding Vec headers/capacities and allocator. This is **support-scoped cost**, not the global C(16,t) scan of the earlier experimental guard, and it is not an assertion that teacher has no unknown coordinates.

The exact global 16-bit Hamming novelty guard uses a precomputed 2^16 Boolean hypercube minimum-distance table: **131072 bytes of logical array payload** (one 65,536-element observed mask plus one 65,536-element distance table). Construction is bounded multi-source BFS; prediction lookup is constant-time **only for this explicitly 16-bit domain**. No broad claim of constant-time Hamming distance over millions of coordinates.

GF(2) TRAIN design rank for the selected candidate union is **6/6** at assumed degree≤1 and **16/16** at degree≤2. Full rank means that *if* the true noiseless function uses only this support and belongs to the specified degree-bounded grammar and has exactly consistent TRAIN labels, its coefficient vector would be identifiable. Neither actual real-world membership nor label exactness is proved, as the candidate loss makes clear. The separate mandatory negative test exercises four even-parity inputs `000,011,101,110` and proves complete pair-pattern coverage with **rank-deficient degree-1** evaluations: `f=0` and `g=a XOR b XOR c` remain indistinguishable despite t=2 coverage.

The first successful dedicated run passed unit tests, cargo fmt/check and Clippy -D warnings, independently compiled true ANF gates, fetched the real SHA-locked corpus, and printed 4 `A6_E1_LEARNED`, 1 `A6_E1_FROZEN`, 32 each of `VAL` and `TEST` risk curves, five standalone controls per split and an exact terminal `A6_E1_COMPLETE,...decision=STOP,oracle=PASS`. Initial CI attempts failed on format, an integer type in a test, Clippy's needless-range-loop/useless-vec/bool-assert/too-many-arguments diagnostics; only formatting, type annotations, test efficiency and immutable scoring-argument grouping were corrected. The pretrained split, seeds, truth labels, model selection policy and 32-grid thresholds were never changed after the successful first real TEST result.

## 4. Scientific decision and future direction

This experiment refutes the **utility of this particular TRAIN-learned hard-ANF Bayesian-style posterior** on the binary next-space task; it does not refute all ANF learning, all sparse memory or a future autoregressive SML model. Crucially, it demonstrates that adding more input-support guards was not the binding problem: the learned bank and its probability calibration were.

**Next model-oriented research is contingent on a separate preregistered TRAIN/VAL experiment to:**
- replace the hard 0/1 posterior with a properly normalized **train-only count-calibrated ANF readout**, with explicit unseen-address backoff and probability scoring;
- compare that to the existing ngram2/4 on the same real bytes, then broaden to full 256-symbol next-byte log-loss rather than perpetually optimize the auxiliary space label;
- test conditional candidate-support discovery, sufficient GF(2) design rank, unknown-grammar fallback and correlations without claiming support coverage certifies a hidden teacher.

Do not promote A6-E1 to SML-2 or a 25B research-quality statement, and do not extend guards to degree4/5 merely because a weaker model remains overconfident. SciRust/FLAT/NNIS/RemoteOps kernels only become a justified next step after the candidate model beats a credible reference on an unchanged model-level task.
