# MOR-H4a matched prototype controls

Status: qualification harness only. No representative-task or runtime-promotion result.

## Purpose

The merged H4a trainer learns a discriminative observed-state medoid for each fixed MOR cluster. Before H4b may alter expert membership, H4a must be compared against matched prototype controls under the same training/development split.

## Arms

The harness evaluates four frozen-topology arms:

- fixed: unchanged H1/H2 cluster prototypes;
- learned medoid: merged H4a discriminative medoid trainer;
- majority: training-only coordinate-wise Boolean majority, with exact ties retaining the fixed seed bit;
- random medoid: deterministic training-only observed-state medoid selected independently for each cluster from frozen seeds.

All arms preserve cluster IDs and expert membership. Development examples are read only after each arm has been fully constructed.

## Random control

Random medoids use a small deterministic SplitMix64 mapping of the frozen seed and cluster ID to an index in the sorted unique positive training states. This is a reproducible control, not a randomness-quality or cryptographic claim.

Seed count has an explicit hard budget. Duplicate seeds are removed before evaluation.

## Evidence

The report retains development cluster-classification accuracy for fixed, learned, majority and every random-seed arm, plus random mean accuracy and signed learned-minus-control gains.

A qualification gate may require declared minimum gains over fixed, majority and random-mean controls. The merged H4a trainer's own training/development non-regression gate must also pass.

This gate means only that the bounded cluster-geometry experiment is eligible for further review. It does not establish final task quality, exhaustive-primary recall, latency or language quality.

## H4b boundary

H4b learned expert-to-cluster membership remains blocked until a separately frozen development campaign uses this harness with repeated seeds and representative task definitions, and until later end-to-end routing/task evidence is retained.

