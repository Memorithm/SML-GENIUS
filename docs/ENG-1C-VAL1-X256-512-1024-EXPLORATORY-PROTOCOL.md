# ENG-1c/VAL1-X256-512-1024 — exploratory threshold escalation

Frozen 2026-10-13 at the user's explicit request, before new code or scoring.
Parent: b05a726719f2cf7fd78a1838c28d3a96f316b91a, PR544.

## Predeclared sequence

1. Add MIN_COUNT_256 and MIN_COUNT_512 to all eleven existing arms, score
   the thirteen-arm mode in FR and EN.
2. Then score MIN_COUNT_1024 with all thirteen earlier arms retained
   (fourteen-arm mode) in FR and EN. No change in thresholds after stage1.
3. Compare all thirteen common predictive rows between the two stages.
   Retain separate aggregate evidence, source/binary hashes and manifests.

All three thresholds are frozen here before either stage. This continues
exploration on the previously exposed Dumas/Austen VAL; it is NOT independent
confirmation. Do not replace earlier protocols, outputs, or reports.

## Unchanged semantics and data

Same exact TRAIN/VAL SHA256, immutable depth32 Witten–Bell count store,
64-lane histories and target denominators. Choose deepest of [32,12,7]
with exact TRAIN count >= threshold, otherwise depth4.
No VAL label enters selection, counts never update on VAL.
TEST is not scored or created as a file.

Modes: original seven-arm default unchanged; --explore-counts-16-32 keeps9;
--explore-counts-64-128 keeps11; --explore-counts-256-512 adds two arms (13);
--explore-counts-1024 adds the final arm (14). Each mode retains a distinct
output prefix and the proper original experimental identity.

## Analysis and resource constraints

All arms: bits/byte, exact256-class argmax accuracy, depth-selection counts,
descriptive paired64-lane bootstrap versus FIXED_4. Use2048 draws,
seed0xE1C5_2026_1013_B001, order statistics51/1996.
Positive gain = fixed4 loss minus adaptive loss.
Report all results; no winner retuning or model promotion on this exposed VAL.
Keep the previous0.010000-bit/byte effect-size threshold for the printed
exploratory verdict. Intervals do not establish independent significance.

Rust1.89.0, 262144-KiB virtual-address limit, 3000-second evaluator deadline,
existing disk quotas. Reuse only SHA256-verified local TRAIN/VAL splits.
All arms retain the depth32 store; higher thresholds do not prove storage,
RAM, energy or production latency savings. No GPU or ElasticXxx actuation.

Store only aggregate evidence in PR544, never corpus bytes or learned stores.
