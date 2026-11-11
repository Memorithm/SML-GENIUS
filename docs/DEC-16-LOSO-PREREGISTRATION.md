# DEC-16 pre-registration: leave-one-set-out router evaluation (2026-11-10)

## Why
DEC-15 (`docs/RESULTS-DEC-15-POLARITY.md`) showed that changing the reserved wording set moves the same router by about 0.10. For example, ROUTER14_BOOL scored 0.728 on S14 and 0.627 on S15. That is larger than the effects being tested.

DEC-16 measures the router families on all four spent reserved sets: S12 (14 wordings), S13 (50), S14 (50) and S15 (50), 164 wordings in total. Each fold is held out in turn. No new wording is read.

## Design (`sml_decision::dec16_loso`, runner `examples/dec16_loso_heads.rs`)
- **Base training.** The DEC-12 training wordings plus the DEC-13 scope grammar, minus every reserved set.
- **Fold k.** The router trains on the base plus the other three sets. TEST is then asked with the wordings of set k. `tests/dec16_loso.rs` checks that set k is absent from the fold-k training for every intent.
- **Router families.** The DEC-12 procedure applies to all three: 16 epochs, quantized to ±7 (BOOL), with the integer router reported as INT.
  - **R12:** DEC-12 features.
  - **R14:** R12 + DEC-14 anchors.
  - **R15:** R12 + DEC-15 anchors (antonym polarity, replicated block).
- **Content heads.** DEC-2 exactly, with BOOL heads for BOOL routers and INT heads for INT routers.
- **ORACLE and CONST** are reported for each fold.

## Contamination (declared)
- The DEC-14 seeds were chosen after reading the DEC-13 misses, so S13 is not clean for R14 or R15.
- The DEC-15 seeds were chosen after reading the DEC-14 misses, so S14 is not clean for R15.
- The clean folds are therefore:
  - for R14: S14 and S15, both written after the DEC-14 seeds;
  - for R15: S15 only.
  - R12 has no seeds, so every fold is clean for it.

## Primary claim (pooled asked-TEST accuracy, BOOL, over the clean R14 folds S14 + S15)
Let gain = R14_BOOL − R12_BOOL.
- **DEC16_ANCHOR_CONFIRMED** if gain ≥ 0.05.
- **DEC16_ANCHOR_WEAK** if 0.02 ≤ gain < 0.05.
- **DEC16_ANCHOR_NOT_CONFIRMED** if gain < 0.02.

## Validity (all must hold, else DEC16_INVALID)
1. The two processes give identical rows.
2. CONST scores exactly half on every fold.
3. ORACLE scores 3375/4000 on every fold.
4. The DEC-2 content heads reproduce 886/1000 and 739/1000.

## Reported, not gated
- Pooled accuracy over all four folds for each family and variant.
- Per-fold accuracy and the spread between folds.
- Per-wording router accuracy, pooled over all 164 wordings and over the clean folds, with Wilson 95% intervals.
- The route of each wording.
