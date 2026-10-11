# A5 step 3: incremental evaluation and a first byte-level task

Diagnostic results only. This continues `docs/RESULTS-A5-TOY.md` (step 1) and
`docs/RESULTS-A5-STEP2.md` (step 2). Raw outputs of the runs below were kept
outside the repository; every number here is copied from those outputs.

## Part 1: incremental evaluation of search proposals

### What it does

Module: `crates/sml-core/src/anf_local_incremental.rs`.

- **Cache.** The evaluator keeps the trajectory of the current network: for
  every lane block and step, the bit-sliced words `[s_{t+1}, x_t]`. It also
  keeps the per-address count table of every readout.
- **Gate proposal.** A proposal on state gate `g` is evaluated by
  re-simulating, at each step `t`:
  - gate `g` itself;
  - every gate that reads a state bit whose words differ from the cache at
    step `t-1`.
  A recomputed gate whose words equal the cache stops the propagation. The
  work is therefore the actual downstream cone of the change over time.
  Frozen gates (history layer) are never re-simulated unless they read a
  changed bit.
- **Readout counts.** Counts are updated only at steps where a readout input
  actually changed: `-1` at the old address and `+1` at the new address, per
  changed lane. The loss is updated term by term.
- **Readout rewiring.** A rewiring proposal recounts only that readout from the
  cached trajectory, with no state re-simulation.
- **Commit and discard.** `commit` patches the cache; `discard` undoes the
  count updates.
- **Exact comparison.** Losses are integers: errors, or leave-one-out log-loss
  in fixed-point micro-bits. Incremental and full evaluation are therefore
  compared bit-exactly.
- **Multi-bit networks.** `LocalAnfNetwork` gained `input_bits`, so it can take
  `W` input bits per step. `W = 1` is unchanged.
- **Integration.** The step-1/2 learner (`anf_local_learn::search`) uses the
  incremental evaluator by default (`LocalAnfLearnConfig::incremental`).
  - Proposals now record an undo instead of cloning the whole network.
  - The RNG stream is unchanged, so the search trajectory is identical to step 2.

### Equivalence (tests)

- **`incremental_matches_full_resimulation_*`.**
  - Setup: random networks with 1, 3 or 8 input bits; 2 lane blocks, one of
    them partially valid; 3 readouts, including chained target-bit inputs;
    both loss kinds.
  - 300 random proposals: gate coefficient flips and readout rewirings,
    randomly committed or discarded.
  - Every proposal loss equals `full_loss` on the modified network.
  - Every 25 proposals, the committed cache equals a fresh full simulation bit
    for bit, and so do all count tables.
- **`incremental_search_is_identical_to_full_resimulation_search`.** The step-1
  learner gives an identical report, network included, with
  `incremental = true` and `false`.
- **`incremental_char_search_matches_full_resimulation`.** The same check for
  the byte model.
- **Toy regression.** `sml-learn-toy 5 32 6 local 16` was re-run. Every
  accuracy and exact-seed count of the step-2 table is identical. Only the
  times changed: the full step-2 learner took 0.01-0.51 s per seed instead of
  0.02-1.16 s, and the whole run took 86 s instead of 239 s of wall time.

### Throughput

Command: `cargo run --release -p sml-core --bin sml-anf-throughput -- <corpus>`.
Single thread, shared box.

**Settings:**
- **"toy":** delayed recall d=8 train split, random local wiring, fan-in 6,
  error loss. All gates are mutable.
- **"byte":** 1023 steps x 64 lanes of the corpus, 8 input bits. 32 frozen
  history gates (L=4) plus `N-32` free gates of fan-in 6. 8 chained readouts
  and leave-one-out loss. Only the free gates are mutated.

**Method:**
- A proposal flips 1-2 coefficients of a random mutable gate. It is kept if the
  loss does not increase.
- Full and incremental evaluation run the same proposal sequence and reach the
  same final loss ("same final loss" column).
- The "long run" columns continue the incremental search for up to 20,000
  proposals or 10 s. They show how the cost evolves as accepted flips
  accumulate.

| Setting | N | steps x lanes | proposals | full (proposals/s) | incremental (proposals/s) | speedup | gate evals per proposal (full / incr.) | same final loss | long run: proposals | long run: incr. proposals/s | long run: gate evals per proposal | long-run speedup |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| toy d=8 | 32 | 64 x 64 | 400 | 4321.1 | 11126.7 | 2.6x | 2048 / 640 | yes | 20000 | 6328.6 | 1131 | 1.5x |
| toy d=8 | 256 | 64 x 64 | 400 | 557.2 | 56544.1 | 101.5x | 16384 / 160 | yes | 20000 | 2309.6 | 3041 | 4.1x |
| toy d=8 | 1024 | 64 x 64 | 400 | 142.0 | 102920.5 | 725.0x | 65536 / 91 | yes | 11935 | 1193.3 | 5765 | 8.4x |
| toy d=8 | 4096 | 64 x 64 | 400 | 35.8 | 69360.5 | 1935.7x | 262144 / 91 | yes | 16446 | 1644.2 | 4227 | 45.9x |
| byte | 64 | 1023 x 64 | 400 | 147.9 | 814.1 | 5.5x | 65472 / 10519 | yes | 4911 | 491.0 | 17922 | 3.3x |
| byte | 256 | 1023 x 64 | 400 | 36.6 | 2500.1 | 68.3x | 261888 / 3681 | yes | 1146 | 114.4 | 71522 | 3.1x |
| byte | 1024 | 1023 x 64 | 181 | 9.0 | 5757.0 | 636.8x | 1047552 / 1253 | yes | 2572 | 256.0 | 28011 | 28.3x |
| byte | 4096 | 1023 x 64 | 43 | 2.1 | 3173.1 | 1502.2x | 4190208 / 1023 | yes | 8125 | 801.3 | 7233 | 379.4x |

### Interpretation

- **Equivalence is exact.** Same losses, same cache, same search trajectories.
- **Large wins at the start of a search.** For the first 400 proposals from a
  fresh network, incremental evaluation is:
  - 2.6x faster at N=32;
  - 100x at N=256;
  - about 700x at N=1024;
  - about 1900x at N=4096 (toy), 1500x on the byte setting.
  Most proposals change gate `g`'s words on few lanes and steps, and the
  difference dies out after a few steps.
- **The cone grows as the search goes on.** Accepted flips, including neutral
  ones, make the dynamics more sensitive, and the downstream cone of later
  proposals grows. On the toy at N=1024:
  - the cone goes from 91 to 5765 gate evaluations per proposal;
  - the speedup over the full re-simulation drops from 725x to 8x.
  The long-run speedups are 1.5x / 4x / 8x / 46x on the toy, and
  3x / 3x / 28x / 379x on the byte setting.
  - The full cost is fixed (`N x steps` gate evaluations).
  - The incremental cost depends on how far a change propagates. With
    self-loops (`s_g` in every gate) and dense random wiring, a persistent
    change can reach most of the network over 1000 steps.
- **N >= 1024 runs are now affordable.**
  - In the byte task with N=1024, one full evaluation on the train split costs
    about 0.6 s (1032 gates x 5411 steps).
  - A search epoch of 64 proposals took about 0.6 s in total, validation
    included (see Part 2).
  - Without the incremental evaluator this would be about 40 s per epoch.
- **Limit.** Long searches on chaotic networks lose most of the gain. Bounding
  the cone, through sparser wiring or gates without self-loop, or evaluating on
  sub-windows, is future work.

## Part 2: first byte-level task

### Corpus and splits

- **Corpus.** Jules Verne, *Le tour du monde en quatre-vingts jours* (1873),
  French.
  - Source: Project Gutenberg eBook #800. Public domain (author died in 1905).
  - Only the book body is kept; the Project Gutenberg header, footer and
    license text are stripped.
  - 433,036 bytes (UTF-8; accented letters are 2 bytes).
- **Not committed.** `scripts/fetch-char-corpus.sh` downloads it, normalises
  line endings and checks a pinned SHA-256 (`b51f36fb…74`). The output goes to
  `data/char/`, which is git-ignored.
- **Splits.** Contiguous 80/10/10 %: train 346,428 / val 43,304 / test
  43,304 bytes.
- **Token layout, shared by all models.** Each split is cut into 64
  contiguous chunks, one per lane. The first byte of a chunk is not predicted
  and the context resets at chunk starts. That gives 346,304 / 43,200 / 43,200
  scored tokens.

### How the ANF model predicts bytes

Module: `crates/sml-core/src/anf_char.rs`.

- **Input.** `x_t` enters as 8 bit-planes, at source positions `N..N+8`.
- **State.** `N = F + 8L` bits.
  - **History layer, `8L` gates.** Gate `(j, p)` has target bit `p` of
    `x_{t+1-j}`, for `j = 1..L`. It is fitted greedily and exactly
    (step-2 procedure): forward selection of sources among the input bits and
    the earlier history gates, then the majority truth table converted to ANF.
    It reaches 0 train errors on every run, which makes it an exact byte delay
    line (tested). `h_1` duplicates `x_t`, so the effective context is `L`
    bytes.
  - **Free gates, `F` of them.** Fan-in 6, random wiring over state and input
    bits, step-1 initialisation.
- **Output: 8 chained readouts, most significant bit first.**
  - Readout `b` predicts bit `b` of `x_{t+1}` from bits `0..b-1` of the same
    byte (teacher-forced) plus up to `C` context source bits.
  - Its table is fitted exactly from train counts.
  - `P(x_{t+1}) = prod_b (c_v + 1/2) / (n + 1)` (KT estimate per address). This
    is a proper distribution over the 256 bytes (tested).
  - Bits per byte = mean `-log2 P(next byte)`.
  - Accuracy uses the exact argmax over the 256 bytes.
  - The binary ANF readout is the majority table. The counts are an extra
    calibration table, reported as "count entries".
- **Training.**
  1. Fit the history layer.
  2. Choose readout context bits by greedy forward selection on the train
     *leave-one-out* log-loss, which penalises overfitting without touching
     validation. Every source position is a candidate. Selection stops when the
     loss no longer improves, or at `C = 12` context bits / total fan-in 16
     (20 in one variant).
  3. Run the step-1 local search on the free gates and the readout wiring:
     loss = train leave-one-out log-loss, incremental evaluation, 64 proposals
     per epoch, early stopping on validation bits per byte (patience 4,
     at most 20 epochs).

### Baselines (same splits, same tokens)

- **Unigram and n-grams with context 1-4 bytes.** Interpolated Witten-Bell,
  backing off to uniform over 256 bytes.
- **Elman RNN, byte-level, f32.**
  - H=64, one-hot input (embedding), softmax output.
  - Truncated BPTT over 32 steps on the 64 lanes, Adam (lr 3e-3), gradient
    clipping.
  - 15 epochs, 2 seeds; the best-validation epoch is kept.
  - It is still improving at epoch 15: validation bits per byte go from 2.58 to
    2.55 over the last 3 epochs.

### Results

Command: `cargo run --release -p sml-core --bin sml-learn-char -- data/char/verne-tour-du-monde.txt 3 15 2`.

- Means ± std over seeds. n-grams are deterministic.
- Times are per seed, single thread.
- Inference tokens/s is the scoring pass on test: the probability of the
  actual byte **and** the exact argmax over 256 bytes.
- The "parameters" column is not directly comparable across families:
  - n-grams: stored (context, byte) counts;
  - RNN: f32 weights;
  - ANF: coefficient bits of the state gates plus the readout truth tables
    (`2^k` bits each), plus the number of non-empty count entries.

Corpus: 433036 bytes; splits train/val/test 346428/43304/43304 bytes; scored tokens train/val/test 346304/43200/43200 (64 contiguous chunks, context reset per chunk)

| Model | seeds | val bits/byte | test bits/byte | test accuracy (%) | parameters | bits read per token | train time (s) | inference tokens/s |
|---|---|---|---|---|---|---|---|---|
| unigram (Witten-Bell to uniform) | - | 4.658 | 4.702 | 14.1 | 103 stored counts | 0 byte(s) of context | 0.01 | 4133788 |
| n-gram, context 1 byte(s), Witten-Bell | - | 3.478 | 3.498 | 28.4 | 1730 stored counts | 1 byte(s) of context | 0.02 | 3169029 |
| n-gram, context 2 byte(s), Witten-Bell | - | 2.751 | 2.732 | 41.2 | 11254 stored counts | 2 byte(s) of context | 0.03 | 2624039 |
| n-gram, context 3 byte(s), Witten-Bell | - | 2.300 | 2.238 | 51.5 | 41869 stored counts | 3 byte(s) of context | 0.04 | 2091705 |
| n-gram, context 4 byte(s), Witten-Bell | - | 2.089 | 2.019 | 57.8 | 108031 stored counts | 4 byte(s) of context | 0.08 | 1591586 |
| Elman RNN H=64 (f32, BPTT 32 steps, Adam, 15 epochs) | 2 | 2.556 ± 0.009 | 2.535 ± 0.011 | 48.2 ± 0.2 | 37184 f32 | 20480 f32 MACs | 69.7 | 121663 |
| ANF, no history layer (L=0, F=64), readout selection + search | 3 | 3.447 ± 0.042 | 3.464 ± 0.043 | 29.0 ± 0.7 | 61184 learned bits + 3968 count entries | 475 bits | 11.4 | 150597 |
| ANF, no history layer (L=0, F=64), readout selection only | 3 | 3.472 ± 0.009 | 3.489 ± 0.013 | 28.6 ± 0.2 | 61184 learned bits + 3968 count entries | 475 bits | 2.6 | 170945 |
| ANF, history L=2 (F=64), readout selection only | 3 | 2.874 ± 0.005 | 2.862 ± 0.004 | 39.6 ± 0.3 | 327712 learned bits + 20253 count entries | 518 bits | 3.8 | 166895 |
| ANF, history L=4 (F=64), readout selection only | 3 | 2.834 ± 0.001 | 2.815 ± 0.002 | 41.6 ± 0.1 | 327744 learned bits + 24972 count entries | 534 bits | 4.5 | 165246 |
| ANF, history L=4 (F=64), readout selection + search | 3 | 2.834 ± 0.001 | 2.815 ± 0.002 | 41.6 ± 0.1 | 327744 learned bits + 24972 count entries | 534 bits | 6.6 | 168283 |
| ANF, history L=4 (F=64), readout fan-in <= 20, readout selection + search | 3 | 2.522 ± 0.001 | 2.474 ± 0.001 | 49.2 ± 0.1 | 5050432 learned bits + 80052 count entries | 565 bits | 11.9 | 138618 |
| ANF, history L=4 (F=992, N=1024), readout selection + search | 3 | 2.837 ± 0.001 | 2.820 ± 0.002 | 41.6 ± 0.0 | 387136 learned bits + 24788 count entries | 6102 bits | 57.7 | 118753 |

### Interpretation

- **The n-gram wins, as expected.**
  - Context-3 and context-4 Witten-Bell n-grams reach 2.24 and 2.02 bits per
    byte, against 2.47 for the best ANF variant.
  - They are also about 10x faster at inference and train in under 0.1 s.
- **The best ANF variant (L=4, readout fan-in ≤ 20) beats:**
  - the context-2 n-gram (2.47 vs 2.73);
  - the 15-epoch RNN baseline (2.47 vs 2.54). That RNN is under-trained, so
    this comparison is weak.
- **The history layer is what works.**
  - Without it (L=0), the readouts can only read the current byte and random
    free gates: 3.46-3.49 bits per byte, the level of a context-1 n-gram (3.50).
  - L=2 gives 2.86 and L=4 gives 2.82.
- **The readout fan-in cap is the binding limit.** With a dense table of total
  fan-in 16, the readout for the last bit has 7 target bits and only 9 context
  bits. That is not even two full bytes of context. Raising the cap to 20
  (16 context bits) moves L=4 from 2.82 to 2.47.
  - In the logs, almost every readout stops at a cap (12 context bits, or
    total fan-in 16 or 20) rather than because the leave-one-out loss stopped
    improving.
- **The free-gate search adds almost nothing.**
  - With a history layer: 0 strict improvements in about 250 proposals per
    seed, at N=96 and at N=1024. Early stopping then ends the search.
  - Without history (L=0): one seed out of three improved (3.47 → 3.40 val,
    85 accepted improvements). The other two found none.
  - **Why:** a random free gate only matters if a readout reads it. Rewiring a
    readout input to a random free gate almost always raises the leave-one-out
    loss, and coefficient flips on unread gates are neutral.
  - The task-loss local search has no gradient-like signal towards useful
    features. This is the step-1 credit-assignment problem again, now at the
    feature level.
- **N=1024 is affordable but not useful yet.** It costs about 58 s per seed (about 54 s
  of that is readout selection over 1032 candidates, 2.2-2.6 s is the search) for no
  gain over N=96.
- **What the ANF model is, honestly.** Here it is a learned
  *bit-subset context model*:
  - a history delay line, plus count tables over greedily chosen context bits;
  - in effect, a constrained n-gram with per-bit chain rule and no backoff.
  It has none of the n-gram's smoothing across context lengths, and its dense
  tables waste memory: 5 M table bits for 80 k non-empty entries.

## Limitations

- **Scale.** A single small corpus (433 KB, one book, one language). Few seeds
  (3 for ANF, 2 for the RNN), and the ANF variants are nearly deterministic
  given the history layer.
- **Weak RNN baseline.** It is stopped at 15 epochs while still improving, and
  has no GRU or LSTM. A tuned recurrent model would very likely beat every
  model here.
- **No backoff.** The ANF probabilities use per-address KT estimates. Unseen
  context bits fall back to `1/2` per bit; there is no backoff to shorter
  contexts.
- **Readout cost.** Readout tables are dense (`2^k` entries), which caps the
  fan-in at about 20.
- **Incremental speedups degrade.** They drop sharply in long searches, see
  Part 1.
- **Free-gate search was ineffective.** With the current move set it brought
  essentially nothing, so the "learned recurrent features" part of the model
  is not demonstrated on bytes.
- **Timings.** Single-threaded on a shared machine; indicative only.

## Next step proposal

1. **Backoff and sparse readouts.**
   - Use hashed or sparse count tables, so the fan-in can exceed 20.
   - Use a hierarchy of readouts over nested context subsets, combined like
     Witten-Bell (a "bit-subset n-gram" with interpolation).
   - This is the shortest path to matching the n-gram at equal context.
2. **Constructive features instead of random free gates.**
   - Fit new gates greedily and exactly to *residual* targets: for example the
     readout's per-address error indicator, or the next-byte bit conditioned on
     a selected address.
   - Select their sources by information gain, as for the history layer.
   - This gives the free gates a direct local target instead of relying on
     random flips.
3. **Long-range state.**
   - Add gates with task-agnostic targets beyond a fixed lag: word-boundary
     indicators, "bits since last space", parity/hash of the current word.
   - State clearly that this injects text-specific priors.
4. **Bounded incremental cost.**
   - Cap the cone (no self-loop on free gates, sparse fan-out).
   - Or evaluate proposals on random sub-windows with periodic full refreshes,
     so long searches keep the 100x+ speedups.
