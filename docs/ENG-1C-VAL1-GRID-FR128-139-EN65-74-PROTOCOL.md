# ENG-1c/VAL1-GRID — FR128..139 and EN65..74 exploratory sweep

Frozen 2026-10-13 at the user's request, before grid code or scoring.
Parent: be86e5926ebf3f1140810e0853055da067aa2e76, PR544.

## Exact predeclared ranges

FR: every integer threshold 128 through 139 inclusive (12 thresholds).
EN: every integer threshold 65 through 74 inclusive (10 thresholds).
No skipping values, adaptive stopping, extending endpoints or retuning
after observing this sweep.

Keep all fifteen earlier control/candidate rows and their identities.
FR128 is already MIN_COUNT_128, so reuse that row instead of duplicating it;
add FR129..139 (11 new rows, 26 total).
Add EN65..74 (10 new rows, 25 total).
Report every requested threshold and the earlier best FR128/EN64 plus
FR140/EN75 controls. Report best requested threshold and all exact ties;
selection on these exposed validations is exploratory.

## Unchanged semantics

New --explore-threshold-grid mode uses pinned corpus identity for its
requested range. The gate chooses deepest of [32,12,7] with exact TRAIN
context event count >= threshold, otherwise depth4.
All previous six modes retain their arm counts and output identities.
Same full TRAIN-only immutable depth32 Witten–Bell store, TRAIN/VAL SHA256,
64-lane histories and target denominators. No labels in depth selection
and no counts update on VAL. TEST is not scored or created as a file.

## Evidence, analysis and resources

Record all rows: bits/byte, exact256-class argmax accuracy, selected depths
and paired descriptive intervals versus FIXED_4 (2048 draws, seed
0xE1C5_2026_1013_B001, order statistics51/1996).
Positive gain = fixed4 loss minus candidate loss.
All fifteen earlier rows must match the prior raw evidence exactly.
Do not treat post-hoc grid selection or its bootstrap as independent
significance. No model promotion; preserve the existing0.010000-bit/byte
effect-size criterion for printed exploratory verdicts.

Rust1.89.0, 262144-KiB virtual-address limit and3000-second evaluator
deadline; existing disk quotas and SHA256-verified local TRAIN/VAL reuse.
Retain aggregate evidence, manifests and source/binary SHA256 in PR544.
No corpus bytes or learned stores uploaded. No physical storage saving,
serving latency, GPU, ElasticXxx actuation or SML25B promotion.
