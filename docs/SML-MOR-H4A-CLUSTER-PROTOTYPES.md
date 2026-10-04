# MOR-H4a — bounded learned cluster prototypes

Status: experimental reference implementation. No model-quality, scaling, or
deployment-promotion claim.

## Scope

MOR-H4a learns only the Boolean semantic signature of an existing MOR-H1/H2
cluster.

The following remain frozen:

- cluster IDs;
- expert membership of every cluster;
- router/expert graph;
- native MixtureOfRouters final selection;
- MOR-H2 widening policy;
- MOR-H3 specialization-balance semantics.

This is intentionally narrower than learned topology or learned expert
membership.

## Training input

Each training example contains:

- one packed SBG routing field;
- an explicitly supplied target cluster ID.

The trainer does not infer ground truth from the current router, model output, or
evaluation set. Label provenance remains a caller responsibility.

Unknown cluster labels fail closed.

## Boolean prototype fitting

For each cluster and semantic coordinate:

1. count true and false observations over that cluster's permitted training
   examples;
2. compute the absolute majority margin in integer ppm;
3. ignore exact ties;
4. ignore coordinates below the configured minimum margin;
5. if the supported majority differs from the seed prototype, create a mutation
   candidate;
6. sort candidates by margin, then coordinate index;
7. apply at most the hard per-cluster mutation budget.

Low-margin or tied coordinates retain the original seed bit. No arbitrary
tie-break bit is learned.

## Fixed membership contract

`SemanticHierarchicalMorBridge::with_cluster_prototypes` accepts learned
signatures only when:

- the number of clusters is unchanged;
- every original cluster ID is present exactly once;
- no unknown cluster ID is introduced;
- each cluster's expert vector is unchanged.

A membership change is rejected rather than silently turning prototype learning
into topology search.

`fit_hierarchy_cluster_prototypes` applies the trained prototypes through this
same fail-closed path.

## Evaluation

The module reports both seed and fitted training classification. A separate
`evaluate_cluster_prototypes` function evaluates frozen prototypes on any
caller-supplied split.

Cluster prediction uses full Boolean XNOR similarity and deterministic cluster-ID
tie breaking.

Training accuracy is not a holdout result. Synthetic unit tests demonstrating
improvement are software fixtures only.

## Accounting

Each cluster training report retains:

- example count;
- number of supported bits;
- number of proposed mutations;
- number of committed mutations;
- mutations dropped by the hard budget;
- strongest supported margin;
- weakest committed margin;
- mean committed margin.

The aggregate report retains seed and fitted training accuracy plus total mutated
bits.

## Qualification sequence

Before any architectural promotion:

1. freeze train/development/holdout task and source identities;
2. compare fixed seed prototypes against learned prototypes at identical expert
   membership and H1/H2/H3 settings;
3. measure shortlist recall and final task quality separately;
4. report per-cluster example counts and imbalance;
5. repeat seeds and retain regressions/ties;
6. test domain-frequency shift and unseen-state behavior;
7. only after prototype learning qualifies, consider MOR-H4b membership/topology
   search as a separate preregistered experiment.

MOR-H4a does not establish that the learned semantic geometry is optimal, that
cluster labels are correct, or that larger SML models will benefit.
