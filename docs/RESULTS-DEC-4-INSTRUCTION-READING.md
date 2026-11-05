# DEC-4 results — Boolean heads that must read the question (2026-11-05)

Pre-registration: `docs/DEC-4-INSTRUCTION-READING-PREREGISTRATION.md` (SML
`a7e1dcd3`, SHA-256 checked). Run: RemoteOps #1253, Actions run 42009751804,
Thor, DEC-1 bench `fec8d3c5…`. Two processes, identical scores and predictions.

## Verdict: **DEC4_PARTIAL** (READS_SEEN yes, READS_NEW no)

BLIND control: exactly 2000/4000 on both sets (valid).

| arm | TEST seen | TEST new |
|---|---|---|
| **SML_BOOL_INSTR** | **0.774** (bar 0.70) | 0.515 (bar 0.60) |
| SML_INT_INSTR | 0.832 | 0.515 |
| BLIND | 0.500 | 0.500 |

Per task, SML_BOOL_INSTR (accuracy, both questions right):

| | seen T1 | seen T3 | new T1 | new T3 |
|---|---|---|---|---|
| accuracy | 0.791 | 0.756 | 0.414 | 0.616 |
| both right | 0.659 | 0.617 | 0.189 | 0.271 |

Per intent on `new`: Continues 0.444, NotContinues 0.384, Opening 0.456,
Middle 0.704, End 0.684. Selected quantized head: 8 epochs, 7 levels
(3 bit planes), keep 1/32, 23 545 non-zero weights; fit 49 s on Thor.

## Reading
- With the wordings it was trained on, the mostly Boolean head **reads the
  question**: it gives opposite answers to opposite questions on the same
  content (T1 both-right 0.66 for the quantized head, 0.84 for the integer
  head). The integer head on seen T1 (0.875) is close to DEC-2's single-question
  T1 (0.886).
- With new wordings it fails. T1 falls below chance (0.414): the new negations
  ("wrong", "false") are unseen words, while shared words such as "after" and
  "B"/"A" pull toward the wrong polarity. T3 keeps some signal (0.62) where a
  key word is shared ("middle", "article", "part").
- So word conjunctions read templates, not meaning. Generalising to new
  wordings needs word meaning: Boolean codes in which "false"/"not"/"wrong" or
  "start"/"beginning"/"opening" share bits. That is the next step (DEC-5).
