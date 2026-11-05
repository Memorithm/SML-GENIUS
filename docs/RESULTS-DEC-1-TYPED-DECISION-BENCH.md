# DEC-1 results — typed-decision bench, reference arms (2026-11-05)

Pre-registration: `docs/DEC-1-TYPED-DECISION-BENCH-PREREGISTRATION.md` (commit
`5b75c685`, SHA-256 checked in the job). Run: RemoteOps #1249, Actions run
41999033696, Thor, controller `51b0de91`.

## Bench `dec1-pes2o7-v1`
- Source peS2o-0007 SHA-256 `453b3115…` verified; 2400 G1 articles recomputed
  and skipped (id-list SHA-256 `ae896355…`).
- Build counts: rows 20061, claim 424, email 89, short 2, dup 0, split_full 3146.
- Articles: TEST 1000, DEV 1000, TRAIN 12000. One example per task per article.
- TEST labels: T1 False 502 / True 498; T2 A 244 / B 247 / C 274 / D 235;
  T3 0 316 / 1 351 / 2 333.

## TEST scores (accuracy; T3 also mean absolute level error)

| arm | T1 `noul` continuation | T2 `choice` 1 of 4 | T3 `score` position (MAE) |
|---|---|---|---|
| B0 majority | 0.498 | 0.274 | 0.316 (1.017) |
| B1 lexical (TRAIN-fitted) | **0.813** | **0.835** | **0.744** (0.302) |
| Laya EN 421M zero-shot | 0.631 | 0.457 | 0.359 (0.643) |
| Laya ML 322M zero-shot | 0.537 | 0.342 | 0.344 (0.660) |

Laya CPU float32, 14 threads: EN p50 532 / 900 / 231 ms; ML p50 181 / 314 /
137 ms (T1 / T2 / T3). Runtime laya 0.4.24 @ `5a6dc4b8`, torch 2.15.0+cpu,
transformers 5.22.1, pip freeze SHA-256 `269fd63c…`.

## Pre-registered flags
- `TASK_TOO_EASY` (B1 ≥ 0.95): none. `TASK_AT_CHANCE`: none.
- DEC-2 reference per task = max(B1, best Laya) on TEST = **B1**:
  T1 0.813, T2 0.835, T3 0.744.

## Reading
- Laya zero-shot is above chance on all three tasks (EN: +0.13, +0.18,
  +0.04) but well below the lexical baseline that learns from TRAIN labels.
  This matches upstream's own note that the base checkpoints are weak
  zero-shot on unfamiliar typed decisions.
- The tasks are largely solvable by word overlap and word counts. Beating B1
  means learning these tasks better than a bag of words, not reading
  instructions. Zero-shot instruction following needs its own later bench.
