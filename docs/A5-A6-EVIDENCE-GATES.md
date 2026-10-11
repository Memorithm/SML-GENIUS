# A5/A6 evidence gates after the 2026-10-11 audit

Status: research decision record; no new performance claim.

## What the merged evidence actually supports

A5 step 3, byte prediction on one 433,036-byte Jules Verne corpus:
- Witten-Bell context-4 n-gram: 2.019 test bits/byte, 57.8% exact next-byte accuracy.
- Best reported ANF history L=4, readout fan-in <=20: 2.474 bits/byte, 49.2%.
- Elman RNN H=64 after 15 epochs: 2.535 bits/byte, 48.2%; the baseline was still improving.
- ANF history L=4 with default readout: 2.815 bits/byte.
- Increasing free gates from F=64 to F=992 did not improve the measured score.
- Search over free gates gave essentially no strict gains when history was present.

These are reported benchmark observations, not an independent rerun. They do not
support a 25B quality, latency, or attention-replacement claim.

A6 on BTCUSDT/ETHUSDT 2019-2025 hourly public data, 20 quarterly OOS folds:
no tested decision model beats buy-and-hold after costs. Short-horizon ANF
direction signals turn over too often; this is not a profitable strategy.
See `docs/RESULTS-A6-CRYPTO.md` for data hashes, assumptions and caveats.

## Stop doing

- Do not count PRs, ANF address-space size or gate count as research outcomes.
- Do not enlarge free-gate count without a preregistered measurable gain.
- Do not promote the crypto circuit as a trading system.
- Do not treat passing unit tests as evidence of model quality.
- Do not claim physical RAM/VRAM ceilings from logical gate/trace budgets.

## Next falsifiable experiment: A5-4 sparse hierarchical readout

Primary hypothesis: train-only sparse hierarchical/backoff readouts reduce
held-out bits/byte at the same input history and train/test split compared
with the current dense ANF readout, without needing additional free gates.

Required controls, identical corpus and splits:
1. Existing ANF history L=4, F=64, dense readout fan-in <=20.
2. Context-4 Witten-Bell byte n-gram.
3. New sparse count/readout with nested context subsets, train-only smoothing.
4. Ablation: same new readout without backoff, and same history with random
   unused free gates disabled.
5. An adequately trained recurrent baseline, or mark the RNN comparison
   inconclusive until convergence.

Primary endpoint: test bits/byte; secondary: accuracy, bytes of model storage,
actual peak RSS, training wall time, inference throughput. Record seeds,
exact commit, corpus SHA-256, toolchain, machine, warmup, sample count,
training-only selection, and variance. Keep the held-out test set untouched
until hyperparameters are frozen on validation. Report failure if no robust
improvement; never silently retune on test.

Admission gate: add unit tests for probability normalization, finite loss,
unseen contexts, empty/rare count buckets, deterministic replay and identical
train/validation/test separation. Report memory allocation separately from
logical ANF state size.

## Subsequent experiment: constructive recurrent features

If the readout gate passes, train residual-target free gates by an explicit
train-only objective and compare against history-only and random-gate controls.
Require a strict held-out improvement beyond measurement uncertainty at
matched parameter/memory and compute budgets before scaling N.

## A6 methodological gate

Before any further crypto search: preserve the existing negative result,
predeclare a fresh evaluation interval, use economically realistic slippage
and execution constraints, adjust for multiple comparisons, and avoid any
live-trading integration. Trading results are not a proxy for language-model
quality.

## Ecosystem transfer

Use RemoteOps for resource admission and native host evidence. Only after
the algorithmic gate passes, evaluate bit-packed execution and vector
projection with SciRust and FLAT-ATTENTION, and qualify inference via NNIS.
No external GPU SDK requirement is introduced.
