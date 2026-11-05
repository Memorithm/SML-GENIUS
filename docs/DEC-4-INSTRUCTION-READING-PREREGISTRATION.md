# DEC-4 — Boolean heads that must read the question (pre-registration)

Frozen 2026-11-05, after DEC-3 (`docs/RESULTS-DEC-3-LM-FEATURES.md`,
`DEC3_NO_GO`) and before any DEC-4 number on real data. The owner approved
follow-up steps in advance. This is the first step toward the owner's goal of
an SML model that understands instructions: the answer now depends on the
question, not only on the content.

## Question
Can the DEC-2 Boolean heads read a yes/no question whose wording changes per
row, and give opposite answers to opposite questions about the same content?
Can they do so with wordings never seen in training?

## Bench (derived from DEC-1 `fec8d3c5…`, built by `sml-decision::dec4_instr`)
- T1 (continuation) and T3 (position) examples only. T2 is left out because it
  has no natural yes/no negation.
- Every example is asked **twice**, with opposite answers:
  - T1: "B continues A" (`Continues`) and "B does not continue A" (`NotContinues`);
  - T3: the true level (`Opening`/`Middle`/`End`) and one other level, chosen
    by a hash of the example id.
- Wordings are fixed in `dec4_instr::seen` (2–3 per intent, used in TRAIN, DEV
  and TEST `seen`) and `dec4_instr::held_out` (1–2 per intent, used only in
  TEST `new`). A row's wording is picked by a hash of split, id and intent.
- Rows: TRAIN 48 000, DEV 4000, TEST `seen` 4000, TEST `new` 4000.
- A head that ignores the question gives the same answer to both rows of an
  example, so it scores exactly 0.500 on every split.

## Model
- Features, hashed into 2^20 bits: the DEC-2 content bits (T1 pair bits, T3
  text bits), a content-kind tag, the question's word bits, and the AND of each
  question word with each content bit and with the kind tag.
- Heads, training, quantization grid, DEV-only selection and tie rules are
  DEC-2's, unchanged, run over the larger space (`select_on_dev_dim`). DEC-2
  outputs are unchanged by this generalization (same prediction digests on the
  synthetic bench).
- Arms: **SML_BOOL_INSTR** (primary, quantized), SML_INT_INSTR (descriptive)
  and BLIND (no question bits; a control that must score 0.500).
- TEST is read once per arm and per wording set.

## Criteria (SML_BOOL_INSTR on TEST)
- `READS_SEEN`: accuracy on TEST `seen` ≥ 0.70.
- `READS_NEW`: accuracy on TEST `new` ≥ 0.60.
- **DEC4_GO**: `READS_SEEN` and `READS_NEW`.
- **DEC4_PARTIAL**: `READS_SEEN` only.
- **DEC4_NO_GO**: no `READS_SEEN`.
- **DEC4_INVALID** overrides all if BLIND is not exactly 0.500 on both sets.
- **NO_GO_DETERMINISM** overrides all if two processes give different
  `DEC4_SCORE`/`DEC4_PRED` lines.
- Reported, not judged: SML_INT_INSTR, accuracy per task and per intent, the
  fraction of examples with both questions right, non-zero weights, fit time.

## Run
Thor, on the pinned DEC-1 bench, with the DEC-2 TSV export, two processes,
pinned to this SML commit (RemoteOps PR).

## Known limits
- The heads only learn conjunctions of words. A held-out wording that shares
  no informative word with the seen ones (for example "false" or "wrong" for
  the negation) cannot be read; `READS_NEW` measures how far word overlap
  carries. Real paraphrase reading needs word meaning, a later step.
- The questions are templates in English, about two task families. This is
  not open instruction following, and the heads do not generate text.
