# ENG-1c/VAL1-LANG-FR140-EN75 — exploratory language-specific thresholds

Frozen 2026-10-13 at the user's explicit request before new code/scoring.
Parent: f80a988e98552812d5597ee3cd6626f88ff30d82, PR544.
User-selected thresholds: FR=140 exact TRAIN occurrences; EN=75.

## Frozen candidate and comparison

Keep all fourteen earlier fixed-depth/count-gated arms. Add one explicitly
language-specific candidate LANG_COUNT_FR140_EN75: use threshold140 in the
pinned FR evaluation, threshold75 in the pinned EN evaluation.
It is selected from the declared corpus identity, not predicted language
or a VAL label. Choose deepest in [32,12,7] with TRAIN count >= threshold,
otherwise depth4. No adaptation or fitting on VAL.

Same frozen TRAIN/VAL hashes, depth32 immutable Witten–Bell store, histories,
64-lane target denominators and previous resource quotas. Score FR140 and
EN75; report each against fixed4 and its earlier best (FR128, EN64).
All original fourteen rows must reproduce exactly. TEST remains unscored
and is not materialized as a file.

This is a further post-hoc refinement on exposed Dumas/Austen VAL. No
independent significance, general language advantage or promotion claim.

## Evidence and unchanged criteria

New opt-in --explore-language-counts-140-75 adds a fifteenth arm and separate
ENG1C_VAL1_LANG140_75 output prefix. All five earlier modes keep their arm
counts and identities. Include explicit effective TRAIN threshold in the
new mode's metadata.

Report bits/byte, exact256-class accuracy, all selected depths and descriptive
paired64-lane bootstrap gain versus fixed4:2048 draws, unchanged seed
0xE1C5_2026_1013_B001 and order statistics51/1996.
Positive gain = fixed4 loss minus candidate loss. Keep the existing
0.010000-bit/byte effect-size condition for exploratory verdicts.

Rust1.89.0, 262144-KiB virtual-address limit, 3000-second process deadline
and existing storage quotas. Only SHA256-verified local TRAIN/VAL reuse.
No dataset text/stores uploaded. Aggregate evidence, source/binary SHA256
and report retained in PR544. No storage, GPU, runtime or SML25B promotion.
