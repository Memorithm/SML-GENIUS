# NBE-2 — pre-registered Q4 atlas × order-2 mixture arms (next-byte eval)

Date 2026-11-21. Protocol frozen **before any NBE-2 number exists**.
Engineering research only: no model promotion, no language-model quality
claim, no biological-realism claim. Author: Grok (assisting GPT-6 on
SML-GENIUS, issue #1018). This PR adds code, synthetic tests and this
document; it adds **no measured value** and launches no campaign.

Parent protocol: `docs/SML-INTEGRATED-LOOP-EVAL-PREREGISTRATION.md` (NBE-1).
Every frozen input, DEV roster, position definition, metric and validity gate
of NBE-1 applies unchanged. TEST stays unopened.

## Why this experiment (published inputs only)

NBE-1 (RemoteOps run [44912119606](https://github.com/Memorithm/RemoteOps/actions/runs/44912119606),
SML-GENIUS `2819d8c7`, reproduction gate PASS) reported on DEV:
S2a Q4 atlas 4.007063135 bits/byte, `order2_dirichlet_backoff_train`
3.592544112, `unigram_add1_train` 5.280199447. Single deterministic pass,
descriptive only.

### Code reading (not a measurement): the atlas already has the order-2 context

The #1018 comment asked whether the atlas covers the previous two bytes. It
does — so "add an order-2 context feature" is **not** a meaningful arm:

- `route(a, b, bit, pages)` and `feature_slot(a, b, bit, f)` in
  `sparse_gradient_atlas.rs` key every weight by the full two-byte context
  `(a, b)` and the output head.
- The 17 features are a bias plus the 16 sign bits of `a` and `b`. For a fixed
  `(a, b, head)` all 17 signs are constants, so the logit is one scalar per
  `(a, b, head)`: the atlas is an **order-2 per-head logistic table**, the same
  context family as the order-2 count baseline (the MSB tree factorizes any
  order-2 conditional exactly).
- That table is stored with hash sharing: 65,536 contexts × 255 MSB heads =
  16,711,680 addresses over 48,829 pages (≈ 342 per page), and
  16,711,680 × 17 slot uses over 48,829 × 1,024 slots ≈ **5.68 addresses per
  slot** if every context were touched (analytic upper figure from code
  constants; TRAIN touches fewer contexts).

So the 0.415 bits/byte NBE-1 gap is between two estimators of the *same*
order-2 conditional: SGD + slot sharing + Q4 quantization versus counts with
Dirichlet back-off. NBE-2 asks the only question that matters before the
atlas is used as the predictive head of GPT-6's loop: **does the Q4 atlas
carry any signal complementary to the order-2 counts?** If not, the loop
should be gated against the count baseline and the atlas fit (collisions,
optimiser) is the thing to fix, not its context.

## Arms (new, opt-in; default CLI output unchanged)

Code: `LinearMixture` and `CausalBayesMixture` in
`crates/sml-core/src/next_byte_eval.rs`; CLI arms `mix_half`, `mix_bayes` in
`crates/sml-core/src/bin/sml-next-byte-eval.rs`. Experts are the NBE-1 S2a Q4
atlas (fresh cold instance through the same `Q4Checkpoint` loader) and the
NBE-1 TRAIN-only `Order2Counts` (same fitted object, frozen).

1. **M1 `mix_q4_order2_linear_half`**:
   `P(c|h) = 0.5 · P_q4(c|h) + 0.5 · P_o2(c|h)`. The weight 0.5
   (`LINEAR_MIXTURE_WEIGHT`) is fixed a priori and **not tuned**.
2. **M2 `mix_q4_order2_bayes_doc`**: per-document causal Bayesian model
   average, prior 1/2 each, reset at every document. The posterior at
   position `i` uses only the revealed bytes `doc[2..i]`; the target is never
   visible. No free parameter.

Nothing is fitted on DEV. No weight is selected on DEV. No other arm,
weight, or fixed-share rate will be added after seeing NBE-2 numbers without
a new pre-registration.

Not pre-registered here (would need its own protocol): a learned or
TRAIN-tuned mixture weight (the atlas has seen all of TRAIN, so a TRAIN
held-out weight would be biased toward it), larger atlases, or retraining.

## Hypotheses and decision rule (fixed now)

Primary metric: DEV bits/byte, as in NBE-1. Effect threshold
**Δ = 0.010 bits/byte**, fixed a priori (single deterministic pass, no seeds,
no confidence interval; this is a descriptive threshold, not a significance
test).

- **H1 (complementarity, Bayes):** `bpc(M2) ≤ bpc(order2) − 0.010`.
- **H2 (complementarity, linear):** `bpc(M1) ≤ bpc(order2) − 0.010`.
- **Decision:** if H1 and H2 both fail, NBE-2 reports *"no complementary
  signal ≥ 0.010 bits/byte from the S2a Q4 atlas over TRAIN-only order-2
  counts on this protocol"*, and recommends that the integrated loop be gated
  against `order2_dirichlet_backoff_train`. If either holds, the gain is
  reported descriptively with both experts' rows; the outcome stays
  `NOT_PROMOTED` and is not a language-model-quality claim.
- **Prior expectation (recorded so it can be wrong):** H1 fails (the
  per-document posterior should collapse onto order-2 quickly); H2 uncertain.

Secondary, descriptive only: Δ of each mixture versus the Q4 arm; probability
ns/byte; page loads. **Top-1 is not measured for mixture arms** (exact
mixture argmax needs 256 Q4 probability calls per position, ≈ 2.3 h–17 h on
the Dell); pinned mixture runs must pass `--no-top1` (enforced by the CLI).
NBE-1 top-1 values stay the reference.

## Validity gates (run is INVALID if any fails; rows are then discarded)

All NBE-1 gates (manifest/Q4/roster pins, reproduction gate `4.007063135`
exact, trait-path vs grouped path within 1e-6, 16,317,746 positions per arm,
normalization probe), plus, printed on a `MIXTURE_GATES` line:

- **O2 reproduction:** the order-2 row of the same run must print
  `3.592544112` exactly (pinned mode).
- **M1 Jensen gate:** `L(M1) ≤ (L(q4) + L(order2)) / 2` (concavity of log2;
  slack 1e-9 bit per position for summation order).
- **M2 Bayes bound:** `L(M2) ≤ min(L(q4), L(order2)) + D` bits, `D` = 583
  documents (prior 1/2), same slack. A violation means an implementation bug.
- Mixture arms must be listed after both `q4` and `order2` in `--arms`
  (enforced) so both expert rows exist in the same run.
- Each expert receives exactly one probability call per scored position in
  M2 (tested), so a future stateful expert sees the NBE-1 call contract.

## Execution (follow-up, not in this PR)

After this PR merges, a separate RemoteOps PR pins its merge SHA in the
existing Dell next-byte workflow (#1867 pattern: exact SHAs, checkpoint and
split hashes, fail-closed, CPU only, no GPU toolkit) and runs:

```bash
cargo run --locked --release -p sml-core --bin sml-next-byte-eval -- \
  --mode pinned --data-root DATA_ROOT --manifest MANIFEST.tsv \
  --checkpoint /home/remoteops/sml-artifacts/sml50m-hf500-cc-s2a-b13217cadac12443b5aef00ec22bc72dd57759b1 \
  --arms q4,order2,unigram,mix_half,mix_bayes --no-top1
```

One deterministic pass on one host. stdout is recorded verbatim; results go
to #1018 and a separate results document that keeps negative outcomes.

## Tests (synthetic fixtures only)

`crates/sml-core/tests/next_byte_eval_mixture.rs`: linear-mixture formula,
normalization and Jensen bound; weight validation; Bayes posterior equals the
closed form, normalization at several depths, one-bit-per-document bound,
identical experts reproduce the expert exactly (loss and top-1 stream),
one expert call per position, top-1 independent of the target; Q4 × order-2
mixtures pass both theorem gates; CLI: mixtures are opt-in, default output
has no mixture row, existing rows are unchanged (except wall-clock latency),
arm-order and Q4-input requirements, pinned mixture runs refuse top-1.
