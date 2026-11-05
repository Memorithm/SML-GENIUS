# DEC-1 — typed-decision bench, Laya zero-shot reference (pre-registration)

Frozen 2026-11-05 before any build or run. The owner asked on 2026-11-05 for
SML-GENIUS, a mostly Boolean model, to understand instructions and answer at
the level of a ~500M model such as Laya, and chose the "typed decisions" bench
as the first comparison ("Banc de décisions typées").

## Why typed decisions
Laya (Convai Innovations, Apache-2.0, `convaiinnovations/laya` 421M English,
`laya-multilingual` 322M) is not a text generator. It is a bidirectional
encoder that answers constrained questions about a text: `noul` (yes/no),
`choice` (one of several options) and `score` (an ordered rubric). It cannot
be measured in bits per character, so SML and Laya are compared on such
questions. A constrained answer is a classification, which a Boolean model
can target directly.

DEC-1 builds the bench and measures the reference arms only. **No SML model is
trained or scored in DEC-1.** The SML decision head is DEC-2, pre-registered
separately before it reads TEST.

## Data: `dec1-pes2o7-v1`
- Source: `peS2o-0007.json.gz` from `common-pile/peS2o_filtered` at revision
  `29774751…`, SHA-256 `453b3115…` (the S5h pin). No SML model trains on 0007
  (S6 TRAIN is 0001/0008/0009).
- The 2400 articles of the G1 corpus `g1-pes2o7-v1` are recomputed with the G1
  rules and skipped, so DEC-1 shares no article with G1.
- Admission: the S5h screens (shape, a CC-BY/CC0 claim covered by the committed
  operator review, no email, no duplicate id or text) and at least 8 spans.
- Split by SHA-256 of the peS2o id: TEST 1/16, DEV 1/16, TRAIN the rest, capped
  at 1000 / 1000 / 12000 articles (TRAIN at least 4000). The builder refuses
  if TEST or DEV is short.
- Each article is cut into word-aligned spans of at most 256 characters and
  gives one example per task, labelled by the construction itself:
  - **T1 `noul`**: "Passage A: … / Passage B: …". Does B directly continue A
    in the same article? Yes half the time (B is the next span). Otherwise B
    is a middle span of another article in the same split.
  - **T2 `choice`**: which of four options (A–D) directly continues the
    passage? One option is the next span, and the other three are middle spans
    of three other articles in the same split. The position of the true option
    is uniform.
  - **T3 `score`**: is the passage the opening of its article (span 0), the
    middle (span n/2) or the end (span n−2)? Each level is uniform.
- All draws are SHA-256 of the seed `dec1-pes2o7-v1` and a tag, so every host
  builds the same bytes. The manifest records the counts, the label histogram
  per split and task, and the SHA-256 of `bench.jsonl`.
- Builder: RemoteOps `ops/sml/dec1_bench/build_dec1_bench.py` (stdlib, tested).

## Arms (all scored on TEST once)
- **B0 majority**: the most frequent TRAIN label (chance is 0.50 / 0.25 / 0.33).
- **B1 lexical**, fitted on TRAIN:
  - T1: word-set Jaccard(A, B) at or above a threshold chosen on TRAIN;
  - T2: the option with the highest Jaccard with the passage;
  - T3: multinomial naive Bayes on lowercase word counts.
- **Laya zero-shot**: `laya_en` (`convaiinnovations/laya` @ `55cf4c4e`) and
  `laya_ml` (`laya-multilingual` @ `e4e9ddf2`). Upstream runtime
  `NandhaKishorM/laya` @ `5a6dc4b8`, CPU, float32, one example per call.
  - Laya reads the instructions and options, and never sees a label.
  - Predictions: `noul` ≥ 0.5 means yes; `choice` is the returned label;
    `score` is the expected level rounded to the nearest level.
- Metric: accuracy per task; for T3, also the mean absolute level error.

## Host
Thor (`[self-hosted, Linux, ARM64, remoteops, thor]`). It fetches peS2o-0007
only if absent, at the same pinned revision and SHA-256. The Mac mini runner
has been unassigned since 2026-11-04, and the Dell is busy with S6. The bench
text and per-example predictions stay on Thor. Only counts, hashes and scores
go to the job log.

## What DEC-1 reports (no GO/NO_GO)
- The manifest, the build counts and `DEC1_SCORE` rows for B0, B1, `laya_en`
  and `laya_ml` on T1–T3, plus Laya timings.
- Bench sanity flags, fixed now:
  - `TASK_TOO_EASY` if B1 ≥ 0.95 on a task;
  - `TASK_AT_CHANCE` if every arm is within 0.02 of chance.

  A task flagged `TASK_TOO_EASY` is dropped from DEC-2.
- The reference for DEC-2 on each remaining task is max(B1, best Laya arm) on
  TEST.

## Fairness, stated before the run
- Laya answers zero-shot from the instructions. B1 and any later SML head
  learn each task from TRAIN labels. A future SML win therefore shows that it
  learned these three tasks, not that it reads instructions. Following
  instructions zero-shot needs its own later bench.
- Laya may have read peS2o text in pre-training. The labels here are new
  (built by construction), so no answer key can have leaked.
- Dependency versions besides the pinned laya commit and checkpoints are
  recorded (`pip freeze` SHA-256), not pinned.
