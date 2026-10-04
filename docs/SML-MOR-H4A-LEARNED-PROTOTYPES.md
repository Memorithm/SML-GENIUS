# MOR-H4a — learned semantic cluster prototypes

Status: experimental bounded trainer. No runtime promotion, model-quality or scaling claim.

## Scope

MOR-H4a learns only the Boolean semantic prototype attached to each already declared MOR-H1/H2 cluster. Expert membership remains frozen.

This deliberately separates cluster geometry, expert-to-cluster membership, final expert selection, and H3 exposure-conditioned balancing. A future H4b may investigate learned membership, but H4a does not.

## Training contract

Each training example provides a packed SBG routing field and a target expert already registered in exactly one cluster.

For each cluster, the trainer partitions training-only examples into positive and negative sets, takes unique positive routing states as bounded candidate prototypes, sorts candidates deterministically by packed words, evaluates mean XNOR similarity to positives and negatives, computes positive-minus-negative separation in integer ppm, selects the highest-separation candidate, and rejects the cluster if the declared minimum separation is not reached.

Candidate count, minimum examples and minimum separation are explicit hard budgets/gates. The prototype is an observed training routing state. H4a does not synthesize unbounded dense vectors, run gradient descent, add floating-point inference or introduce attention.

## Development gate

A separate development holdout is evaluated after all prototypes are frozen. The artifact reports baseline and candidate cluster-classification accuracy on training and development holdout.

Promotion eligibility requires no regression on training classification and candidate development accuracy to exceed the baseline by at least the configured development-gain threshold. Development data never changes a prototype. Protected/final evaluation remains outside this API.

Promotion eligibility is evidence for review only. It does not mutate the repository, authorize an answer or satisfy SML-D calibration.

## Frozen topology

SemanticHierarchicalMorBridge::rebuild_with_cluster_signatures rebuilds only cluster signatures. Router signatures, expert signatures, expert memberships, hierarchy policy and all H1/H2/H3 contracts remain unchanged.

The candidate can therefore be compared directly against the fixed-prototype baseline without simultaneously changing routing topology.

## Evidence

The trainer reports per cluster: positive/negative training counts, bounded candidate count, candidate evaluations, selected positive mean similarity, selected negative mean similarity and selected separation margin.

Global evidence includes total candidate evaluations, baseline/candidate training accuracy, baseline/candidate development accuracy, signed development gain and promotion eligibility.

## Reference controls

Tests retain a deliberately swapped fixed-cluster geometry where H4a recovers the correct training/development cluster classification; a correct fixed baseline where a non-zero required gain prevents promotion; unknown target experts fail closed; insufficient per-cluster examples fail closed; invalid training budgets fail closed; and learned geometry preserves exact expert-to-cluster membership.

These are controlled mechanism fixtures, not representative language or decision-quality evidence.

## Qualification sequence

Before any MOR-H4 promotion: freeze train/development/task/source partitions and seeds; compare fixed H1/H2 prototypes, H4a learned prototypes, matched random prototypes and a simple majority/centroid-style control; report cluster recall, exhaustive-primary recall, final task quality, widening behavior, exposure/starvation and active work separately; repeat over multiple seeds and retain negative runs; evaluate distribution shift and new-domain states; only then consider H4b learned memberships or end-to-end SML-D3 integration.

H4a does not establish that semantic clusters are optimal, that observed-state medoids are the best prototype class, or that learned cluster geometry improves general language quality.
