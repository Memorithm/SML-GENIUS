# DEC-5 — Boolean word-meaning codes for reading new question wordings (pre-registration)

Frozen 2026-11-05, after DEC-4 (`docs/RESULTS-DEC-4-INSTRUCTION-READING.md`,
`DEC4_PARTIAL`: seen 0.774, new 0.515) and before any DEC-5 number on real data.
The owner approved follow-up steps in advance.

## Question
DEC-4 heads read question words as identities, so a held-out wording with new
words is not understood. If each word gets a Boolean code learned from text,
so that words used in similar contexts share bits, does the head read the
held-out wordings?

## Model (`sml-decision::dec5_meaning`, runner `examples/dec5_meaning_heads.rs`)
- **Word codes (64 bits).** Learned once from the unique DEC-1 TRAIN texts
  (all three tasks, file order); DEV and TEST texts and every question are
  never used. Each context word v has fixed signs r(v) ∈ {±1}^64 (a hash).
  S_w is the sum of r(v) over the words within 2 positions of each occurrence
  of w. Bit k is on iff S_w[k]/n_w exceeds the mean of r[k] over all contexts.
  Words with fewer than 20 contexts get no code.
- **Question code (64 bits).** Sum over the question's coded words of ±1 per
  bit. Bit k is on iff that sum exceeds its mean over the TRAIN questions
  (seen wordings).
- **Features** (2^20 hashed bits): the DEC-4 content bits and kind tag, the
  question-code bits, and each question-code bit ANDed with the kind tag and
  with every content bit.
- Bench, rows, wordings, heads, training, quantization grid, DEV-only
  selection and tie rules: DEC-4's, unchanged.
- Arms:
  - **SML_BOOL_MEANING** (primary): question seen only through its code;
  - SML_BOOL_MEANING_ID (descriptive): code plus the DEC-4 word identities;
  - their integer heads (descriptive);
  - BLIND (control, must score 0.500).

## Criteria (SML_BOOL_MEANING on TEST)
- `READS_SEEN`: accuracy on TEST `seen` ≥ 0.70.
- `READS_NEW`: accuracy on TEST `new` ≥ 0.60.
- **DEC5_GO**: both. **DEC5_SEEN_ONLY**: `READS_SEEN` only.
  **DEC5_NEW_ONLY**: `READS_NEW` only. **DEC5_NO_GO**: neither.
- **DEC5_INVALID** overrides all if BLIND is not exactly 0.500 on both sets.
- **NO_GO_DETERMINISM** overrides all if two processes give different
  `DEC5_CODES`/`DEC5_SCORE`/`DEC5_PRED` lines (timings excluded).
- Reported, not judged: the other arms, per-task and per-intent accuracy,
  both-right rates, the Hamming distances between probe words
  (start/beginning/opening/end/final/closing/middle/central/not/false/wrong/
  continue/continues/follow/next/after) and the code of every wording.

## Run
Thor, on the pinned DEC-1 bench and DEC-2 TSV export, two processes, pinned
to this SML commit (RemoteOps PR).

## Known limits
- The codes come from 23 MB of scientific text and a ±2 word window. Such
  codes tend to put antonyms ("start"/"end") close together, because they
  share contexts. Negation ("not" against "false"/"wrong") is the hardest case.
- The question code is a bag of words: word order is lost.
