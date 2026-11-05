# DEC-2 — Boolean decision heads on the DEC-1 bench (pre-registration)

Frozen 2026-11-05, after DEC-1 (`docs/RESULTS-DEC-1-TYPED-DECISION-BENCH.md`)
and before any DEC-2 number on real data. The owner approved the follow-up
steps in advance on 2026-11-05.

## Question
Can a mostly Boolean SML head learn the three DEC-1 typed decisions better
than the DEC-1 reference, which was **B1 lexical** on all three tasks: T1 0.813,
T2 0.835, T3 0.744 on TEST? Laya zero-shot scored 0.631 / 0.457 / 0.359.

## Model (`sml-decision::dec2_text`, runner `examples/dec2_text_heads.rs`)
- **Text bits.** Lowercase ASCII-alphanumeric words. Each unigram and bigram
  is hashed (FNV-1a) into 2^16 bits, giving a sorted set.
- **Pair bits (T1, T2).** The bits both texts share, plus two thermometer
  codes of 32 bits each:
  - overlap count: bit k is on iff |A∩B| > k;
  - Jaccard: bit k is on iff 64·|A∩B| > (k+1)·|A∪B|.
- **Heads**, mirroring Laya's types:
  - `Binary` (T1): score ≥ 0 means yes;
  - `Choice` (T2): one shared scorer over the four (passage, option) pairs, the
    argmax wins, and ties go to the first option;
  - `Ordinal(3)` (T3): argmax over three level rows.
- **Score.** Σ over active bits of an integer weight, plus an integer bias.
  With weights of at most 7 levels, this is a sum over ≤ 3 bit planes of
  2^p·(popcount(x & w+_p) − popcount(x & w−_p)).
- **Training.** Integer averaged perceptron on TRAIN, with a fixed SplitMix64
  example order per epoch.
- **Quantization.** Keep the largest 1/`keep_div` of the non-zero weights. A
  kept weight v becomes sign(v)·max(1, round(L·|v|/max|v|)). The bias is then
  refit on TRAIN with 4 bias-only passes.

## Selection (DEV only) and arms
- Grid: epochs {1, 2, 4, 8} × L {1, 3, 7} × keep_div {32, 16, 8, 4, 2, 1}.
- **SML_BOOL (primary)** is the quantized head with the best DEV accuracy.
  Ties go to fewer bit planes, then fewer non-zero weights, then fewer epochs.
- **SML_INT (descriptive)** is the averaged integer head with the best DEV
  accuracy over epochs.
- TEST is read once, for the two selected heads only. Every DEV grid row is
  logged.

## Criteria (SML_BOOL on TEST, per task, against B1)
- `BEAT` if accuracy ≥ B1 + 0.02; `BELOW` if accuracy ≤ B1 − 0.02; otherwise
  `PAR`.
- **DEC2_GO**: `BEAT` on at least 2 tasks and no `BELOW`.
- **DEC2_PARTIAL**: `BEAT` on at least 1 task.
- **DEC2_NO_GO**: neither.
- **NO_GO_DETERMINISM** overrides all if two processes give different
  `DEC2_PRED` digests.
- Reported, not judged: SML_INT, T3 mean absolute error, non-zero weights, bit
  planes, fit time, µs per decision, and the gap to Laya.

## Run
Thor, on the DEC-1 bench already built there
(`~/sml-corpora/dec1-bench/dec1-pes2o7-v1-51b0de91…`; manifest SHA-256 checked
against the DEC-1 log). RemoteOps exports `bench.jsonl` to TSV
(`ops/sml/dec1_bench/export_dec2_tsv.py`). The runner then runs twice,
pinned to this SML commit.

## Known limits
- The features are lexical, like B1's. A `BEAT` shows that weights learned
  over shared words and bigrams beat raw overlap and naive Bayes. It shows
  neither language understanding nor reading of instructions.
- The heads use no SML language-model state yet. That is a later step (DEC-3),
  if DEC-2 is GO or PARTIAL.
