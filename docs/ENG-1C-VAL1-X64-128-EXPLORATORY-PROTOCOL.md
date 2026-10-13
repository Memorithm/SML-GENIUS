# ENG-1c/VAL1-X64-128 — exploratory occurrence thresholds

Frozen 2026-10-13 at the user's request, before implementing or scoring
threshold64 and threshold128. Parent: cb089ce6644f76f2d7c8a8544f2b94a381336a93.

This extends the exposed Dumas/Austen VAL1 exploration. It does not provide
independent confirmation or change either earlier seven-arm or nine-arm
experiment. Earlier reports and their raw evidence remain intact.

## Frozen arms and semantics

Retain FIXED_4/7/12/32 and MIN_COUNT_2/4/8/16/32, then add
MIN_COUNT_64 and MIN_COUNT_128. Each gate chooses the deepest of [32,12,7]
whose exact TRAIN event total is >= its threshold, otherwise depth4.

Same frozen TRAIN/VAL hashes, depths, immutable depth32 Witten–Bell store,
64-lane reset histories, target denominators and resource quotas.
No VAL label enters the selector and no counts update on VAL.
TEST is neither scored nor created as a file.

## Reporting

Report all eleven arms' bits/byte, accuracy, selected depths, and paired
descriptive intervals against FIXED_4. Bootstrap: 2048 draws,
seed 0xE1C5_2026_1013_B001, order statistics 51 and 1996.
Positive gain = FIXED_4 loss minus adaptive loss.
Report both new thresholds against 16 and 32 without further retuning.
The intervals on already exposed data are descriptive, not independent
significance tests. No promotion or physical storage-saving claim.

## Compatibility and reproducibility

Default command keeps seven arms and original identities.
--explore-counts-16-32 keeps nine arms and previous exploratory identities.
New opt-in --explore-counts-64-128 selects eleven arms with separate
ENG1C_VAL1_X64_128 output identity. Retain hashes and aggregate evidence
in the existing PR #544, never corpus bytes or learned stores.

Use Rust1.89.0, 262144-KiB virtual-address limit, 3000-second deadline,
and the existing pinned FR/EN TRAIN/VAL extraction. Existing hash-verified
local splits may be reused. Process VmHWM excludes filesystem cache.

```bash
cargo build --locked --release -p sml-core --bin sml-eng1c-val1
target/release/sml-eng1c-val1 --explore-counts-64-128 FR \
  /tmp/FR-TRAIN /tmp/FR-VAL /tmp/FR-store-new /tmp/FR-sort-new
```

Repeat for EN with fresh store/scratch paths. Freeze these two thresholds
before code and retain negative outcomes.
