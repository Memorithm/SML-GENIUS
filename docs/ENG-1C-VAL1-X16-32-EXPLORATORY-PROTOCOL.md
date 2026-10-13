# ENG-1c/VAL1-X16-32 — exploratory occurrence thresholds

Frozen on 2026-10-13 at the user's explicit request, before implementing or
scoring the new arms. Parent source: a9a8a8f4e30656c0810c4c8297b65ea6bd49c1b4.

This is a post-hoc exploratory extension of already exposed Dumas/Austen
VAL1 data. It is NOT independent confirmation and does not replace the
original seven-arm preregistration or its negative result.

## Frozen comparison

- Keep the exact FR/EN TRAIN and VAL identities, 64-lane layout, denominators,
  immutable depth32 paged Witten–Bell store and existing resource quotas.
- Retain controls FIXED_4/7/12/32 and MIN_COUNT_2/4/8, add MIN_COUNT_16/32.
- Choose the deepest available TRAIN context in [32,12,7] whose exact
  event count is >= the threshold; otherwise choose depth4.
- No VAL label enters the depth selector. No counts update on VAL.
- Report all nine arms, bits/byte, accuracy, selections at depths4/7/12/32,
  and descriptive paired lane bootstrap intervals versus FIXED_4.
- Bootstrap: 2048 draws, seed 0xE1C5_2026_1013_B001, order statistics 51/1996.
- Positive improvement is FIXED_4 bits/byte minus candidate bits/byte.
- Any comparison/interval on these exposed data is descriptive.
  No predictive promotion, TEST scoring or independent significance claim.
- Default evaluator retains the original seven arms and output identities;
  the explicit --explore-counts-16-32 flag opts into separate exploratory
  output identities and verdicts.
- Pin Rust 1.89, record source/binary hashes and aggregate raw output.
  Process VmHWM excludes filesystem cache; all arms retain depth32 storage.
- No TEST file is created or passed to the evaluator. Original source
  extraction checks the full Gutenberg source hash without TEST scoring.

## Decision

Report each threshold against FIXED_4 and MIN_COUNT_8 in both languages.
A better exploratory loss may motivate a separately frozen fresh-data
study; it cannot authorize model/runtime promotion. Negative outcomes
must be retained without threshold retuning.

## Execution

```bash
cargo build --locked --release -p sml-core --bin sml-eng1c-val1
bash scripts/eng1c/fetch-val1-splits.sh FR /tmp/FR-TRAIN /tmp/FR-VAL
target/release/sml-eng1c-val1 --explore-counts-16-32 FR \
  /tmp/FR-TRAIN /tmp/FR-VAL /tmp/FR-store-new /tmp/FR-sort-new
```

Repeat for EN with fresh paths; use the original 262144 KiB virtual-memory
limit and 3000-second evaluator deadline. Store only aggregate evidence in
the repository, never corpus bytes or trained stores.
