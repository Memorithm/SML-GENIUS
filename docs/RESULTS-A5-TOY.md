# A5 toy results: can a local fan-in ANF recurrent model learn?

*Status: diagnostic prototype (gate A5, step 1). Synthetic toy tasks only. This is
not a language-model, quality or latency claim.*

## Question

Can the coefficients of a Boolean ANF recurrent model, plugged into the SBG
recurrent interface, be **learned without gradients** on sequence tasks with known
answers, and how does that compare with simple baselines on identical data?

## Setup (reproducible)

    cargo run --release -p sml-core --bin sml-learn-toy -- 5 32 6 local    # main table
    cargo run --release -p sml-core --bin sml-learn-toy -- 5 32 6 random   # wiring ablation
    cargo run --release -p sml-core --bin sml-learn-toy -- 5 <N> <k> local # size ablations

Accuracies are deterministic per seed (verified by re-running the binary: same
numbers). Wall-clock times are not deterministic; they were measured on an 8-core
x86_64 box (1 thread per run, up to 3 benchmark processes concurrently).

**Model** (`crates/sml-core/src/anf_local.rs`):
- State of `N` bits plus the current input bit `x`. Each state gate reads `k` source
  bits (fan-in `k`, here 4 to 8): `x`, its own previous bit, and (`local` wiring) its
  neighbour `s_{g-1}` plus random bits, or (`random` wiring) only random bits. The
  wiring is drawn from the seed and then frozen. Each gate is an ANF polynomial over its `k` inputs,
  with `2^k` learned coefficient bits. A gate never addresses the whole state, so the
  31-bit address limit of `anf_large` does not apply.
- Readout `y_t = R(s_{t+1}, x_t)`: one ANF gate with fan-in 6 (`x` plus 5 state bits;
  which state bits it reads is learnable).
- `LocalAnfRewriter<S>` implements `SemanticRewriter<S>`: state in `value[0..N]`, input
  in `value[N]`. A unit test checks that it reproduces the reference run cycle by cycle.
- Evaluation is bit-sliced: 64 sequences per `u64` lane word, and each monomial
  product is one `AND`.

**Learning** (`anf_local_learn.rs`): gradient-free stochastic local search on the hard
coefficient bits (the `delta(M) = L(base) - L(base XOR M)` intervention from
`boolean_integral`, with an immediate accept/reject decision). Move mix:
- 40 % one coefficient flip;
- 20 % two coefficient flips in one gate;
- 25 % replace a gate by a random sparse low-degree ANF (1 or 2 monomials of degree ≤ 2);
- 15 % rewire one readout input.

A move is kept if the training error does not increase. Neutral moves are kept
so the search can cross plateaus. For every proposal, the readout truth table is
fitted **exactly** (majority per readout address on train) and Möbius-converted to ANF.
After each epoch of 256 proposals, the validation error is measured; the
best-validation network is kept (early stopping, patience 60 epochs, max 300).

**Tasks** (`toy_tasks.rs`, uniform random bits, inputs before `t = 0` are `0`):
- running parity;
- parity of the last `w` bits;
- delayed recall `x_{t-d}`;
- majority of the last 5 bits.

Splits per seed: train 128 × 32 steps, val 64 × 32, test 128 × 32, **test-long
64 × 128** (4× longer than training). Five seeds (0..4); the seed changes the data, the wiring and
the search. The metric is per-step accuracy (mean ± std over seeds). "Exact runs" counts the
seeds with 100 % on test-long.

**Baselines** (`toy_baselines.rs`, same splits):
- constant majority class;
- n-gram lookup on the last 9 inputs (optimal for every task except running parity);
- Elman RNN, f32, H=16, trained by BPTT with Adam and validation early stopping. Its
  gradients are checked against finite differences in a unit test.

**Control column "ANF init+readout only"**: the same random initial network with
an exactly fitted readout and **no** state search (`max_epochs = 0`). The difference
between this column and "ANF test" is what the search learned.

## Main results (N=32, k=6, local wiring)

| Task | Constant | n-gram (w=9) | RNN test | RNN test-long | ANF init+readout only (test) | ANF train | ANF test | ANF test-long | ANF exact runs (test-long 100%) | ANF time/run (s) | RNN time/run (s) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| running-parity | 49.5 ± 1.0 | 61.5 ± 0.9 | 80.4 ± 22.3 | 79.3 ± 23.6 | 66.4 ± 18.8 | 100.0 ± 0.0 | 100.0 ± 0.0 | 100.0 ± 0.0 | 5/5 | 0.0 | 0.3 |
| window-parity w=3 | 50.0 ± 0.4 | 99.8 ± 0.2 | 100.0 ± 0.0 | 100.0 ± 0.0 | 53.0 ± 1.1 | 99.7 ± 0.7 | 99.7 ± 0.7 | 99.6 ± 0.7 | 4/5 | 1.7 | 0.0 |
| window-parity w=5 | 50.4 ± 0.7 | 99.8 ± 0.2 | 82.5 ± 21.6 | 81.9 ± 22.3 | 52.5 ± 3.0 | 96.3 ± 4.1 | 95.7 ± 5.1 | 95.6 ± 4.8 | 2/5 | 5.5 | 0.2 |
| delayed-recall d=1 | 51.5 ± 0.8 | 99.8 ± 0.1 | 100.0 ± 0.0 | 100.0 ± 0.0 | 52.1 ± 1.1 | 100.0 ± 0.0 | 100.0 ± 0.0 | 100.0 ± 0.0 | 5/5 | 0.1 | 0.0 |
| delayed-recall d=2 | 53.1 ± 0.9 | 99.9 ± 0.1 | 100.0 ± 0.0 | 100.0 ± 0.0 | 53.1 ± 0.9 | 100.0 ± 0.0 | 100.0 ± 0.0 | 100.0 ± 0.0 | 5/5 | 0.6 | 0.0 |
| delayed-recall d=4 | 56.4 ± 0.9 | 99.9 ± 0.1 | 100.0 ± 0.0 | 100.0 ± 0.0 | 56.4 ± 0.9 | 98.8 ± 2.5 | 98.7 ± 2.7 | 98.6 ± 2.8 | 4/5 | 3.7 | 0.0 |
| delayed-recall d=8 | 62.7 ± 0.7 | 99.8 ± 0.1 | 100.0 ± 0.0 | 100.0 ± 0.0 | 62.7 ± 0.7 | 70.4 ± 1.1 | 65.0 ± 0.8 | 54.4 ± 1.4 | 0/5 | 6.7 | 0.1 |
| window-majority w=5 | 55.2 ± 1.6 | 99.9 ± 0.2 | 100.0 ± 0.0 | 100.0 ± 0.0 | 67.2 ± 0.4 | 97.9 ± 3.2 | 97.7 ± 3.4 | 97.5 ± 3.7 | 1/5 | 7.6 | 0.1 |

ANF learned coefficient bits: 2112 (264 bytes); state/input bits read per token: 198; coefficient bits touched per token by direct ANF evaluation: 2112. RNN f32 parameters: 305 (1220 bytes). n-gram table: 512 entries.

## Wiring ablation (N=32, k=6, random wiring)

| Task | Constant | n-gram (w=9) | RNN test | RNN test-long | ANF init+readout only (test) | ANF train | ANF test | ANF test-long | ANF exact runs (test-long 100%) | ANF time/run (s) | RNN time/run (s) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| running-parity | 49.5 ± 1.0 | 61.5 ± 0.9 | 80.4 ± 22.3 | 79.3 ± 23.6 | 61.6 ± 19.2 | 100.0 ± 0.0 | 99.9 ± 0.1 | 99.8 ± 0.4 | 3/5 | 0.0 | 0.3 |
| window-parity w=3 | 50.0 ± 0.4 | 99.8 ± 0.2 | 100.0 ± 0.0 | 100.0 ± 0.0 | 52.4 ± 0.9 | 100.0 ± 0.0 | 100.0 ± 0.0 | 100.0 ± 0.0 | 5/5 | 0.4 | 0.0 |
| window-parity w=5 | 50.4 ± 0.7 | 99.8 ± 0.2 | 82.5 ± 21.6 | 81.9 ± 22.3 | 52.1 ± 2.9 | 99.6 ± 0.8 | 99.6 ± 0.8 | 99.5 ± 1.1 | 4/5 | 3.0 | 0.2 |
| delayed-recall d=1 | 51.5 ± 0.8 | 99.8 ± 0.1 | 100.0 ± 0.0 | 100.0 ± 0.0 | 51.6 ± 0.8 | 100.0 ± 0.0 | 100.0 ± 0.0 | 100.0 ± 0.1 | 4/5 | 0.1 | 0.0 |
| delayed-recall d=2 | 53.1 ± 0.9 | 99.9 ± 0.1 | 100.0 ± 0.0 | 100.0 ± 0.0 | 52.9 ± 1.1 | 100.0 ± 0.0 | 100.0 ± 0.0 | 100.0 ± 0.0 | 4/5 | 0.2 | 0.0 |
| delayed-recall d=4 | 56.4 ± 0.9 | 99.9 ± 0.1 | 100.0 ± 0.0 | 100.0 ± 0.0 | 56.4 ± 0.9 | 97.2 ± 5.6 | 96.7 ± 6.6 | 96.2 ± 7.5 | 3/5 | 3.9 | 0.0 |
| delayed-recall d=8 | 62.7 ± 0.7 | 99.8 ± 0.1 | 100.0 ± 0.0 | 100.0 ± 0.0 | 62.7 ± 0.7 | 69.0 ± 0.8 | 65.4 ± 0.9 | 54.4 ± 0.9 | 0/5 | 4.6 | 0.1 |
| window-majority w=5 | 55.2 ± 1.6 | 99.9 ± 0.2 | 100.0 ± 0.0 | 100.0 ± 0.0 | 67.2 ± 0.4 | 98.6 ± 1.1 | 98.2 ± 1.4 | 98.2 ± 1.4 | 1/5 | 8.8 | 0.0 |

## Size ablations (local wiring): ANF test-long accuracy (exact runs, train time/run)

| Config | learned bits | bits read/token | running-parity | window-parity w=3 | window-parity w=5 | delayed-recall d=1 | delayed-recall d=2 | delayed-recall d=4 | delayed-recall d=8 | window-majority w=5 |
|---|---|---|---|---|---|---|---|---|---|---|
| N=32, k=6 (main) | 2112 | 198 | 100.0 ± 0.0 (5/5, 0.0 s) | 99.6 ± 0.7 (4/5, 1.7 s) | 95.6 ± 4.8 (2/5, 5.5 s) | 100.0 ± 0.0 (5/5, 0.1 s) | 100.0 ± 0.0 (5/5, 0.6 s) | 98.6 ± 2.8 (4/5, 3.7 s) | 54.4 ± 1.4 (0/5, 6.7 s) | 97.5 ± 3.7 (1/5, 7.6 s) |
| N=32, k=4 | 576 | 134 | 100.0 ± 0.0 (5/5, 0.0 s) | 100.0 ± 0.0 (4/5, 0.1 s) | 95.4 ± 6.4 (3/5, 2.2 s) | 100.0 ± 0.0 (5/5, 0.0 s) | 100.0 ± 0.0 (5/5, 0.1 s) | 94.3 ± 5.9 (2/5, 2.5 s) | 58.0 ± 4.6 (0/5, 1.7 s) | 98.0 ± 3.7 (1/5, 1.8 s) |
| N=32, k=8 | 8256 | 262 | 100.0 ± 0.0 (5/5, 3.0 s) | 100.0 ± 0.0 (5/5, 2.3 s) | 96.2 ± 7.6 (3/5, 13.4 s) | 100.0 ± 0.1 (4/5, 0.6 s) | 100.0 ± 0.0 (5/5, 1.4 s) | 100.0 ± 0.0 (4/5, 3.1 s) | 54.9 ± 1.0 (0/5, 31.3 s) | 98.7 ± 1.3 (2/5, 41.1 s) |
| N=128, k=6 | 8256 | 774 | 100.0 ± 0.0 (5/5, 0.0 s) | 99.9 ± 0.1 (3/5, 2.7 s) | 99.9 ± 0.2 (3/5, 19.9 s) | 99.7 ± 0.6 (3/5, 0.6 s) | 100.0 ± 0.0 (5/5, 1.1 s) | 98.7 ± 2.5 (3/5, 15.4 s) | 63.5 ± 18.3 (1/5, 16.9 s) | 99.7 ± 0.5 (3/5, 18.5 s) |
| N=256, k=6 | 16448 | 1542 | 100.0 ± 0.0 (5/5, 0.2 s) | 100.0 ± 0.0 (5/5, 3.7 s) | 99.6 ± 0.5 (3/5, 81.3 s) | 100.0 ± 0.1 (4/5, 2.1 s) | 100.0 ± 0.0 (3/5, 5.4 s) | 100.0 ± 0.0 (4/5, 44.1 s) | 53.9 ± 0.4 (0/5, 35.9 s) | 100.0 ± 0.0 (4/5, 34.6 s) |

## Interpretation

**The ANF model learns, gradient-free, on tasks whose solution is a short chain of
gates.** Running parity, delayed recall with d ≤ 4, window parity (w = 3, 5) and window majority (w = 5) reach
95.7 to 100 % mean test accuracy. The search did that work: the init+readout-only control
stays at the constant level (51 to 67 %). The solutions generalize to sequences 4× longer
than training (test-long ≈ test), as expected from a learned exact recurrent rule.

**Where ANF wins:**
- *Running parity*. XOR is native to ANF: `s' = s ⊕ x` is 2 monomials. The ANF model is
  exact in 5/5 seeds and in ≤ 100 proposals. The n-gram (61.5 %) cannot represent unbounded
  parity, and the Elman RNN is unstable (80.4 ± 22.3 %; 2 seeds near chance). This advantage
  is partly an **inductive bias of the representation**, not of the learner. In some seeds,
  the random initial network already contains a parity gate (control column 66 ± 19 %).
- *Window parity w=5*: ANF 95.6 to 99.9 % depending on the configuration, vs RNN 82.5 ± 21.6 %.
- Model size and compute: 2,112 learned bits (264 bytes) vs 305 f32 parameters (1,220 bytes)
  for the RNN, with integer XOR/AND evaluation only.

**Where ANF loses:**
- *Delayed recall d=8 fails* in every configuration except one seed out of 5 at N=128. Test-long
  ≈ 54 %, i.e. chance; the test score (65 %) is at the level of the constant (62.7 %), which
  benefits from the zero padding. RNN and n-gram are at 100 %. Reason: copying `x_{t-8}`
  to a readout-visible bit requires a chain of about 8 gates. Every intermediate link gives
  **zero** reduction of the training error until the chain is complete, so the search
  receives no credit signal and wanders on a plateau. The RNN gets graded gradient signal
  through partial correlations; this ANF learner does not.
- *Reliability*: even on tasks it masters, the search reaches an exact solution in only
  1 to 5 of 5 seeds (e.g. majority w=5: 97.5 % mean but 1/5 exact at N=32). Near-solutions
  (98 to 99.9 %) are common. The RNN and the n-gram are exact on almost all window tasks.
- *Training cost*: seconds to tens of seconds per run vs < 1 s for the RNN. Cost grows
  with N (window parity w=5: 5.5 s at N=32, 81 s at N=256), because every proposal
  re-simulates the whole training set.

**What drove learnability (honest note on the design iteration):** a first version
used only random single/pair coefficient flips, a readout fixed to state bits 0..4,
a smaller budget (patience 25) and random wiring. In a single-seed run it learned
running parity but plateaued at 60 to 97 % on all delay/window tasks (d=1: 96.5 %). The
version reported here adds sparse low-degree gate replacement and readout rewiring.
Wiring then matters little: random wiring is as good as or better than local wiring on
most rows of the two tables above. **The move set (structured, low-degree proposals) was the
decisive factor**, not the topology. That first comparison changed several things at once and
used 1 seed, so it is indicative only.

**Size:**
- larger N (128, 256) helps a little on window tasks (more candidate chains);
- k=4 is comparable to k=6 at 4× fewer bits, except d=4 (94.3 vs 98.6 %);
- k=8 gives small gains (d=4: 100 %, majority: 98.7 %) at 4× the bits and 3 to 5× the time;
- none of them reliably fixes d=8 (one exact seed out of 5 at N=128, 0/5 elsewhere).

## Implications for scaling to 16,384 state bits

1. Local fan-in removes the representational limit (N=256 runs fine with k=6). Learning
   cost, not representation, is now the bottleneck. Full re-simulation per proposal is
   O(N · 2^k · T); at 16,384 bits that is about 64× the N=256 cost per proposal, with
   more parameters to search.
2. Credit assignment through time is the real wall. Any capability that needs a long chain
   of individually useless gates (long-range memory) is not discovered reliably by plain local search.
   This is exactly what a language model needs.
3. Low-degree sparse ANF moves are what made search work here. Dense 2^k coefficient
   spaces cost much more for small gains (k=8 vs k=6).

## Limitations

- Toy synthetic tasks, 1 input bit, 1 output bit, sequences ≤ 128 steps, 5 seeds.
- One learner design. No hyper-parameter search was done for either side (RNN: H=16, lr 0.01,
  ≤ 300 epochs; ANF: 256 proposals/epoch, patience 60). A GRU/LSTM or a Tsetlin machine
  were not implemented.
- Readout fan-in is fixed at 6 (`x` + 5 state bits), which caps what a single readout
  can combine. The readout fit is exact (majority table), so it is "learning" only in a
  counting sense.
- The bit-flip learner does not use the `boolean_learning` page transaction:
  `BooleanLearningTransaction` targets a single committed `PackedBooleanPage` with a
  hierarchical group search. A per-proposal accept/reject over a recurrent simulation was
  simpler and sufficient for this question.
- Times are wall-clock on a shared box with concurrent runs, so they are indicative only.

## Proposed next step (A5 step 2)

Attack the credit-assignment wall directly, then re-run this exact benchmark:
- **Auxiliary targets for hidden gates**: give each state gate its own local target
  (e.g. "predict `x_{t-j}`" or a value from a short history), and fit hidden gates greedily per
  gate with the same exact truth-table fitting used for the readout. This turns the neutral chain into
  individually rewarded steps; d=8 is the acceptance test.
- Compare with a continuous relaxation (differentiable logic-gate style, as in `train.rs`) trained
  by BPTT and then hardened, on the same splits.
- Incremental evaluation (re-simulate only the affected gates and time steps) to make
  N ≥ 1024 affordable, then a first character-level task.
