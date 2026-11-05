# DEC-3 — SML language-model features for the Boolean decision heads (pre-registration)

Frozen 2026-11-05, after DEC-2 (`docs/RESULTS-DEC-2-BOOLEAN-DECISION-HEADS.md`,
`DEC2_GO`) and before any DEC-3 number on real data. The owner approved
follow-up steps in advance.

## Question
Do SML language-model features improve the DEC-2 heads? The reference is
DEC-2 SML_BOOL on TEST: T1 0.886, T2 0.902, T3 0.739.

## Model (`sml-decision::dec3_lm`, runner `examples/dec3_lm_heads.rs`)
- **SML P1 byte model.** `sml-core::gb11_p1_stream` (Boolean-addressed count
  banks) with byte orders {0, 1, 2, 3, 4, 6}, average mixer, adaptive update
  and 2^24 table bytes. Nothing is searched.
- **Training.** One prequential pass over the unique TRAIN texts of the bench
  (in file order). The model is then frozen. DEV and TEST never update it.
- **New bits** (64-bit extension block). DEC-2 never sets them, and its
  outputs are unchanged, as checked on the synthetic bench.
  - `bpc` thermometer of each scored text (T3 passage, T1 B, T2 each option):
    bit k is on iff bpc > 1 + k/8.
  - `boundary gain` thermometer for pairs: the bits saved on the first 16
    bytes of B when the model has read A and a space, against a space alone.
    Bit k is on iff gain > (k − 8)/2. This sees the words that cross the A/B
    cut, which the DEC-2 bits do not.
- **Unchanged from DEC-2:** heads, training, quantization grid, DEV-only
  selection and tie rules.
- Arms: **SML_BOOL_LM** (primary, quantized) and SML_INT_LM (descriptive).
  TEST is read once per arm.

## Criteria (SML_BOOL_LM on TEST, per task, against DEC-2 SML_BOOL)
- `BEAT` if accuracy ≥ DEC-2 + 0.015; `BELOW` if accuracy ≤ DEC-2 − 0.015;
  otherwise `PAR`.
- **DEC3_GO**: at least 1 `BEAT` and no `BELOW`.
- **DEC3_MIXED**: at least 1 `BEAT` and at least 1 `BELOW`.
- **DEC3_NO_GO**: no `BEAT`.
- **NO_GO_DETERMINISM** overrides all if two processes differ in
  `DEC3_SCORE`/`DEC3_PRED`.
- Reported, not judged: SML_INT_LM, the P1 TRAIN bpc and weights SHA-256,
  feature time and fit time.

## Run
Thor, on the DEC-1 bench already pinned for DEC-2 (`fec8d3c5…`), with the
same TSV export, two processes, pinned to this SML commit (RemoteOps PR).

## Known limits
- The P1 model is small (orders ≤ 6, about 25 MB of TRAIN text) and trained
  only on bench text. It is the SML model family, but not the S5h/S6 model.
- A gain on T1/T2 mainly shows that boundary continuity helps. T3 gets only
  the bpc code.
