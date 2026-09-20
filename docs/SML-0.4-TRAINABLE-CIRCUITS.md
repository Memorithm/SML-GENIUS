# SML-0.4 fixed-wiring trainable Boolean circuit contract

Status: executable trainability experiment. No language-model, scaling or novelty claim is made.

## Objective

SML-0.3 established that one hard two-input Boolean gate can be selected through continuous latent training. SML-0.4 asks the next bounded question:

> Can multiple latent gates be optimized jointly through an acyclic Boolean circuit while the qualified forward path remains fully discrete?

The first reference topology is:

    h = x0 XOR x1
    y = h AND x2

This requires two distinct learned gate identities and one internal dependency.

## Fixed wiring

SML-0.4 deliberately freezes wiring. Each gate has two explicit references to either an external input or an earlier gate output.

Construction rejects:

- zero-input circuits;
- zero-gate circuits;
- invalid input references;
- forward gate references;
- cyclic self-references;
- invalid output references.

A gate may therefore depend only on already-defined signals.

## Training surrogate

Each gate keeps the SML-0.3 latent distribution over all 16 two-input Boolean truth tables.

For soft training, every truth table is extended to continuous inputs with its multilinear extension. Reverse-mode derivatives are propagated through the acyclic graph.

A deterministic asymmetric initialization is required. Uniform zero logits make stacked symmetric gate mixtures capable of producing zero upstream gradients; SML-0.4 therefore uses a fixed seedless integer-mixing function derived solely from gate index and truth-table code.

There is no runtime randomness.

## Hard inference

The qualified circuit forward path performs:

1. deterministic argmax for each gate identity;
2. hard Boolean lookup for each selected truth table;
3. fixed DAG signal propagation.

No soft probability is required during hard inference.

## Representation accounting

Gate identity and wiring are reported separately.

For each hard two-input gate:

- learned truth-table state: 4 Boolean bits;
- training-only latent state: 16 floating-point scalars;
- wiring references: structural metadata, not learned parameters in SML-0.4.

The reference accounting reports:

- learned Boolean bits;
- fixed wiring metadata bits;
- training-only latent scalar count.

This prevents fixed structural metadata from being mislabeled as learned model capacity.

## First qualification oracle

The initial circuit has three inputs and two gates:

    gate0 = XOR(input0, input1)
    gate1 = AND(gate0, input2)

All eight possible input triples are evaluated.

The gate passes only if training:

- selects exact hard tables XOR then AND;
- reaches 8/8 hard-output correctness;
- lowers the soft training loss;
- is deterministic across independent identical runs;
- preserves exact representation counters.

## Scope limitation

This does not establish:

- learned circuit topology;
- learned routing addresses;
- recurrent-depth learning;
- associative-memory training;
- natural-language modeling;
- large-scale optimization.

The next structural step is discrete topology search under Forge with a frozen evaluator and explicit holdout boundaries.
