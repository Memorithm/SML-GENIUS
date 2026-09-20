# SML-0.3 latent Boolean gate trainability contract

Status: executable reference trainability experiment. No language-model or scaling claim is made.

## Objective

SML requires discrete Boolean execution but cannot assume that useful discrete circuits will be hand-authored. SML-0.3 therefore tests one minimal question:

> Can a continuous latent training state reliably select an exact hard Boolean function after optimization?

The experiment covers all 16 possible Boolean functions of two Boolean inputs.

## Representation

A trainable gate owns 16 finite latent logits, one per four-bit truth table.

During training only, the logits are normalized with softmax. For a given input row, the predicted probability is the weighted mixture of the 16 candidate truth-table outputs.

The loss is binary cross entropy over all four input rows.

The reference optimizer performs deterministic full-batch gradient descent. No random sampling, shuffled minibatch, or nondeterministic reduction is used.

## Hard forward

The inference path never executes the soft mixture.

After or during training, `hard_table` selects the greatest latent logit with deterministic lowest-index tie breaking. `hard_forward` then performs only a discrete four-bit truth-table lookup.

Thus the training representation and inference representation are intentionally different:

    training: latent logits -> soft mixture -> gradient
    inference: argmax(logits) -> hard truth table -> Boolean output

## Qualification gate

The SML-0.3 oracle iterates over truth-table codes 0 through 15.

For every code, starting from the same zero-logit initialization, the default reference training configuration must:

- select the exact target truth table;
- obtain 4/4 hard-forward accuracy;
- remain finite;
- report deterministic optimization counters.

This exhaustive 16-function gate is stronger than showing only XOR or AND.

## Scope limitation

This result, if green, establishes only trainability of one categorical Boolean gate under a complete four-row training set.

It does not establish:

- trainability of deep Boolean circuits;
- learned wiring or routing;
- recurrent-state training;
- associative-memory training;
- natural-language modeling;
- 25B-scale optimization;
- superiority to existing differentiable logic networks.

## Next research step

SML-0.4 should compose multiple trainable gates under an acyclic circuit contract and separate two questions:

1. continuous optimization of gate identities;
2. discrete search of circuit topology / wiring.

The second question is intended to become a Forge domain only after the evaluator and holdout boundary are explicit.
