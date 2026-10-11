# A5 step 2: self-supervised history layer + greedy exact gate fitting

Diagnostic results, toy scale only. This continues `docs/RESULTS-A5-TOY.md`
(step 1). In step 1 the local fan-in ANF learner solved the short-chain tasks
but failed delayed recall at d=8 (54.4 % test-long, 0/5 seeds). The reason was
credit assignment: an intermediate gate in a delay chain gets no task-loss
signal until the whole chain exists.

## What changed

- **History layer (new, `crates/sml-core/src/anf_local_history.rs`).** The
  last `L` state gates `h_1..h_L` (positions `N-L..N-1`) each get an
  auxiliary local target `h_j(t+1) = x_{t+1-j}`, the input seen `j-1` steps
  before the current one. Inputs before the sequence start are 0.
- **Greedy exact fitting, in order j = 1..L.** For each gate:
  - **Source selection.** The gate's sources are learned by forward
    selection. Candidates are `x` and the history gates fitted earlier
    (`h_1..h_{j-1}`). The gate's own bit and the free gates are excluded, so
    there is no circular dependency and no dependency on task-trained gates.
  - **Exact fit.** The truth table is fitted exactly: the majority target per
    input address on the train split, converted to ANF by the Möbius transform.
    This is the same procedure as the readout fit.
  - **Wiring.** This replaces the step-1 random/local wiring for these gates.
    That wiring could not guarantee that `h_j` reads `h_{j-1}`. Fan-in stays
    at most k=6; in practice one source is selected (`h_{j-1}`, or `x` for
    `h_1`). That is a tapped delay line, verified exact on test-long by the
    unit tests.
- **Task-loss search on the rest.** The history gates are then frozen. The
  free gates `0..N-L-1` and the readout (including readout rewiring) are
  learned on the task loss with the unchanged step-1 search: same move mix,
  same validation early stopping, same budget.
  - `anf_local_learn::search` now takes the number of mutable gates.
  - When all gates are mutable, the RNG stream is identical to step 1.
- **Benchmark.** The same splits and seeds as step 1, plus:
  - delayed recall d=12 and d=16;
  - a harder generalization split, test-xlong (64 x 512 steps, 16x the
    training length).

## Which knowledge the auxiliary targets inject (honest statement)

- **The targets never see task labels.** They come from the input stream
  only; a unit test checks that the fitted layer is identical whatever the
  task.
- **They do inject a strong, generic prior.** The prior is "the last L inputs
  are useful features": a learned tapped delay line with horizon L. That is
  the same kind of prior as the n-gram baseline's window.
- **No per-task lag was chosen.** All tasks use the same L=16.
- **The prior is close to the answer for delayed recall.** For d <= L-1 the
  answer already sits in one state bit; the learner only has to find it by
  readout rewiring and fit an identity.
- **The d=8 result is therefore a test of this prior,** not evidence that the
  learner discovers memory structure by itself.
- **The parity and majority tasks are not handed over.** No window-parity or
  majority auxiliary targets were used, because they would directly inject
  those task solutions.

## Results (N=32, k=6, readout fan-in 6, local wiring, L=16, 5 seeds)

Command: `cargo run --release -p sml-core --bin sml-learn-toy -- 5 32 6 local 16`.
Accuracies are percentages, mean ± std over 5 seeds. "(a/5)" is the number of
seeds with zero errors on test-long. The columns are:

- **ANF v1:** the step-1 learner. This is the baseline without auxiliary
  targets, and its numbers are bit-identical to step 1.
- **hist+readout:** an ablation. The history layer is used, but only the
  readout is searched; the free gates stay at random init.
- **ANF+hist:** the full step-2 learner.

Times are per seed, single thread, on the shared box.

| Task | Constant | n-gram (w=9) | RNN test-long | ANF v1 test-long (exact) | hist+readout test-long (exact) | ANF+hist train | ANF+hist test | ANF+hist test-long (exact) | ANF+hist test-xlong | RNN test-xlong | time v1 (s) | time hist fit (ms) | time ANF+hist total (s) | time RNN (s) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| running-parity | 49.5 ± 1.0 | 61.5 ± 0.9 | 79.3 ± 23.6 | 100.0 ± 0.0 (5/5) | 65.8 ± 17.7 (1/5) | 100.0 ± 0.0 | 100.0 ± 0.0 | 100.0 ± 0.1 (4/5) | 99.9 ± 0.1 | 79.6 ± 23.8 | 0.0 | 0.23 | 0.09 | 0.4 |
| window-parity w=3 | 50.0 ± 0.4 | 99.8 ± 0.2 | 100.0 ± 0.0 | 99.6 ± 0.7 (4/5) | 90.5 ± 19.1 (4/5) | 100.0 ± 0.0 | 100.0 ± 0.0 | 100.0 ± 0.0 (5/5) | 100.0 ± 0.0 | 100.0 ± 0.0 | 1.8 | 0.21 | 0.03 | 0.1 |
| window-parity w=5 | 50.4 ± 0.7 | 99.8 ± 0.2 | 81.9 ± 22.3 | 95.6 ± 4.8 (2/5) | 81.1 ± 23.2 (3/5) | 94.6 ± 10.8 | 93.8 ± 12.4 | 92.3 ± 15.4 (3/5) | 92.1 ± 15.7 | 81.6 ± 22.7 | 5.9 | 0.22 | 1.16 | 0.2 |
| delayed-recall d=1 | 51.5 ± 0.8 | 99.8 ± 0.1 | 100.0 ± 0.0 | 100.0 ± 0.0 (5/5) | 100.0 ± 0.0 (5/5) | 100.0 ± 0.0 | 100.0 ± 0.1 | 100.0 ± 0.1 (4/5) | 100.0 ± 0.1 | 100.0 ± 0.0 | 0.1 | 0.20 | 0.02 | 0.0 |
| delayed-recall d=2 | 53.1 ± 0.9 | 99.9 ± 0.1 | 100.0 ± 0.0 | 100.0 ± 0.0 (5/5) | 100.0 ± 0.0 (5/5) | 100.0 ± 0.0 | 100.0 ± 0.0 | 100.0 ± 0.0 (5/5) | 100.0 ± 0.0 | 100.0 ± 0.0 | 0.7 | 0.23 | 0.02 | 0.0 |
| delayed-recall d=4 | 56.4 ± 0.9 | 99.9 ± 0.1 | 100.0 ± 0.0 | 98.6 ± 2.8 (4/5) | 100.0 ± 0.0 (5/5) | 100.0 ± 0.0 | 100.0 ± 0.0 | 100.0 ± 0.0 (5/5) | 100.0 ± 0.0 | 100.0 ± 0.0 | 4.3 | 0.21 | 0.03 | 0.0 |
| delayed-recall d=8 | 62.7 ± 0.7 | 99.8 ± 0.1 | 100.0 ± 0.0 | 54.4 ± 1.4 (0/5) | 100.0 ± 0.0 (5/5) | 100.0 ± 0.0 | 100.0 ± 0.0 | 100.0 ± 0.0 (4/5) | 100.0 ± 0.1 | 99.9 ± 0.1 | 7.3 | 0.22 | 0.02 | 0.1 |
| delayed-recall d=12 | 69.0 ± 0.5 | 65.7 ± 0.6 | 82.0 ± 21.8 | 55.4 ± 0.9 (0/5) | 100.0 ± 0.0 (5/5) | 100.0 ± 0.0 | 100.0 ± 0.0 | 100.0 ± 0.0 (5/5) | 100.0 ± 0.0 | 80.4 ± 23.6 | 6.2 | 0.21 | 0.03 | 0.6 |
| delayed-recall d=16 | 75.0 ± 0.4 | 71.5 ± 0.7 | 56.3 ± 0.4 | 56.4 ± 0.8 (0/5) | 58.9 ± 4.4 (0/5) | 100.0 ± 0.0 | 100.0 ± 0.0 | 100.0 ± 0.0 (4/5) | 100.0 ± 0.0 | 51.8 ± 0.2 | 4.2 | 0.27 | 0.16 | 0.5 |
| window-majority w=5 | 55.2 ± 1.6 | 99.9 ± 0.2 | 100.0 ± 0.0 | 97.5 ± 3.7 (1/5) | 100.0 ± 0.0 (5/5) | 100.0 ± 0.0 | 100.0 ± 0.0 | 100.0 ± 0.0 (4/5) | 100.0 ± 0.0 | 100.0 ± 0.0 | 8.0 | 0.22 | 0.08 | 0.1 |

ANF+hist: 1120 learned coefficient bits (16 history gates fitted greedily,
16 free gates searched); 118 state/input bits read per token. RNN: 305 f32
parameters.

### Horizon ablation: L=8 (`... local 8`), delayed-recall rows

| Task | Constant | n-gram (w=9) | RNN test-long | ANF v1 test-long (exact) | hist+readout test-long (exact) | ANF+hist train | ANF+hist test | ANF+hist test-long (exact) | ANF+hist test-xlong | RNN test-xlong | time v1 (s) | time hist fit (ms) | time ANF+hist total (s) | time RNN (s) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| delayed-recall d=8 | 62.7 ± 0.7 | 99.8 ± 0.1 | 100.0 ± 0.0 | 54.4 ± 1.4 (0/5) | 55.1 ± 3.8 (0/5) | 100.0 ± 0.0 | 100.0 ± 0.0 | 100.0 ± 0.0 (4/5) | 100.0 ± 0.0 | 99.9 ± 0.1 | 7.0 | 0.000 | 0.2 | 0.1 |
| delayed-recall d=12 | 69.0 ± 0.5 | 65.7 ± 0.6 | 82.0 ± 21.8 | 55.4 ± 0.9 (0/5) | 54.4 ± 0.5 (0/5) | 86.2 ± 11.4 | 84.1 ± 13.2 | 74.0 ± 21.3 (2/5) | 71.1 ± 23.6 | 80.4 ± 23.6 | 6.0 | 0.000 | 2.8 | 0.6 |
| delayed-recall d=16 | 75.0 ± 0.4 | 71.5 ± 0.7 | 56.3 ± 0.4 | 56.4 ± 0.8 (0/5) | 56.0 ± 0.5 (0/5) | 77.6 ± 2.2 | 75.8 ± 0.8 | 56.3 ± 0.8 (0/5) | 51.8 ± 0.3 | 51.8 ± 0.2 | 4.0 | 0.000 | 4.2 | 0.4 |

### Wiring ablation: random wiring, L=16 (`... random 16`), delayed-recall rows

| Task | Constant | n-gram (w=9) | RNN test-long | ANF v1 test-long (exact) | hist+readout test-long (exact) | ANF+hist train | ANF+hist test | ANF+hist test-long (exact) | ANF+hist test-xlong | RNN test-xlong | time v1 (s) | time hist fit (ms) | time ANF+hist total (s) | time RNN (s) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| delayed-recall d=8 | 62.7 ± 0.7 | 99.8 ± 0.1 | 100.0 ± 0.0 | 54.4 ± 0.9 (0/5) | 100.0 ± 0.0 (5/5) | 100.0 ± 0.0 | 100.0 ± 0.0 | 100.0 ± 0.0 (5/5) | 100.0 ± 0.0 | 99.9 ± 0.1 | 4.8 | 0.000 | 0.0 | 0.1 |
| delayed-recall d=12 | 69.0 ± 0.5 | 65.7 ± 0.6 | 82.0 ± 21.8 | 55.2 ± 0.6 (0/5) | 100.0 ± 0.0 (5/5) | 100.0 ± 0.0 | 100.0 ± 0.1 | 100.0 ± 0.0 (3/5) | 100.0 ± 0.1 | 80.4 ± 23.6 | 3.9 | 0.000 | 0.0 | 0.5 |
| delayed-recall d=16 | 75.0 ± 0.4 | 71.5 ± 0.7 | 56.3 ± 0.4 | 56.3 ± 0.6 (0/5) | 56.5 ± 0.6 (0/5) | 96.0 ± 8.0 | 95.4 ± 9.1 | 91.4 ± 17.1 (3/5) | 90.4 ± 19.1 | 51.8 ± 0.2 | 3.9 | 0.000 | 0.6 | 0.4 |

Full tables, including the non-delay tasks, can be reproduced with the commands
above. The binary is deterministic, so a re-run gives the same accuracies;
timings vary.

## Interpretation

- **Acceptance test (d=8, same splits, 5 seeds, test-long 4x longer): passed.**
  - ANF+hist reaches 100.0 ± 0.0 % on test-long, against 54.4 % for the
    step-1 learner.
  - 4/5 seeds have zero errors. The remaining seed has 99.9 % test-long and
    99.8 % test-xlong (validation early stopping kept 2 val errors).
  - The ablation without free-gate search (hist+readout) is 5/5 exact. On
    this task the history layer alone does the work.
- **Longer delays.**
  - d=12 is solved 5/5. Without auxiliary targets the step-1 learner gets
    55.4 %, and the RNN gets 82.0 ± 21.8.
  - d=16 is 100.0 % on test-long (4/5 exact) and 100.0 % on test-xlong.
  - The d=16 target `x_{t-16}` needs `h_17`, which is one step beyond the
    L=16 horizon. Consistently, the hist+readout ablation fails (58.9 %). The
    task-loss search on the free gates therefore had to build one extra delay
    stage on top of `h_16`. We checked this by accuracy only; we did not
    inspect the mechanism.
  - The L=8 ablation shows the same pattern one level down:
    - hist+readout fails d=8 (55.1 %);
    - full search recovers d=8 (4/5 exact);
    - d=12 is only partly recovered (74.0 %, 2/5);
    - d=16 is not recovered (56.3 %).
  - So the free-gate search can extend the line by a few steps, but not
    arbitrarily. The horizon L is the real capacity knob, exactly as for the
    n-gram window, which fails d=12/16 with w=9.
- **Harder generalization.**
  - On test-xlong (512 steps) the ANF+hist models keep their test-long accuracy
    on every task, since the learned circuits are exact and length-independent.
  - The RNN drops to 51.8 % on d=16 and 80.4 % on d=12.
- **No regression on the old table.**
  - The step-1 learner column is unchanged.
  - ANF+hist is equal or better on running parity (100.0 %, 4/5), w3
    (5/5), d1-d4, and majority5 (100.0 %, 4/5 against 97.5 %, 1/5).
  - The exception is window parity w=5: 92.3 ± 15.4 (3/5) against 95.6 ± 4.8
    (2/5). The mean is lower because one seed (seed 3) is stuck at 61.4 %.
- **Parity credit moves to source selection.**
  - With the history layer, w=5 parity could be solved by the readout alone
    (`x, h_1..h_4`, which fits fan-in 6).
  - The readout rewiring search sees no signal until all 5 sources are
    present, because any subset gives chance accuracy on parity.
  - This is the same credit-assignment problem, moved from gate coefficients to
    source selection.
- **Training cost.**
  - Greedy fitting of the 16 history gates takes about 0.2 ms per seed.
  - The full step-2 learner, including the task search on free gates, takes
    0.02-0.2 s on most tasks and 1.2 s on w=5 parity.
  - The step-1 learner takes 0.1-8 s per seed.
  - For d=8 the cost drops from 7.3 s to 0.02 s, because the search stops
    after 1-2 epochs once the readout has found the right history bit.

## Limitations

- **Toy scale only:** 1-bit inputs, short sequences, synthetic tasks, 5 seeds.
  Nothing here says anything about language-model quality.
- **The prior is very close to the answer for delayed recall.** Passing d=8 is
  only a test of the history layer, not of learned long-range memory. A
  dependency beyond about L + a few steps is still out of reach.
  - Running parity is the only task that genuinely needs unbounded memory. v1
    and step 2 solve it through the search on the free gates, not through the
    history layer.
- **Greedy source selection with one-bit targets.** It is trivial here: the
  correct source has zero error. With noisy or multi-bit inputs it would need
  a real stopping criterion.
- **Readout source selection is still local search.** It gets stuck on
  parity-like targets (w=5 seed 3).
- **The "4/5 exact" seeds are early-stopping artefacts,** not systematic
  errors: 1-3 val errors kept, with 99.8-99.9 % test-long.
- **Not tuned.** We did not tune L, or the split between history gates and free
  gates. L=16 and L=8 are the only values measured.

## Next step proposal

1. **Incremental evaluation.**
   - Re-evaluate only the gates downstream of a mutated gate. Cache the
     trajectories of frozen history gates; they are fixed after the greedy fit.
   - This is a prerequisite for N >= 1024: the current search re-runs the whole
     network for every proposal.
2. **A first character-level task.**
   - Byte input as 8 bit-planes, with a history layer per bit-plane. The
     readout becomes 8 output bits, each fitted exactly per address.
   - Compare with the n-gram and with a small RNN at equal bits read per token.
3. **Better readout source selection.**
   - Use the same greedy forward selection as the history layer, with
     conditional-entropy scores instead of single-move accuracy. This should
     address the parity source-selection plateau.
