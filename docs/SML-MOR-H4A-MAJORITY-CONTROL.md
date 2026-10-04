# MOR-H4a control — bounded majority-bit prototypes

Status: matched experimental control. No runtime promotion, model-quality or scaling claim.

MOR-H4a already contains an observed-state prototype search: candidates are real training routing states, selected with positive-versus-negative XNOR separation and checked on a frozen development split.

This control asks a distinct question: is an observed-state prototype actually needed, or can a bounded Boolean majority update of the existing seed perform as well?

## Frozen comparison contract

The control uses the same `SemanticMorHierarchyTrainingExample` schema, so target experts are mapped through the same frozen expert-to-cluster registry. Cluster IDs, expert membership, router/expert signatures and final MOR semantics remain unchanged.

For each cluster, only training examples assigned to that cluster contribute to the majority prototype. Development data never changes a bit.

Each coordinate is considered only when its true/false majority margin reaches the configured integer-ppm threshold. Ties and weak coordinates retain the seed bit. Supported changes are sorted by margin and coordinate index, then truncated by a hard per-cluster mutation budget.

The report retains baseline/candidate training accuracy, baseline/candidate development accuracy, signed development gain, promotion eligibility, per-cluster supported bits, proposed mutations, applied mutations, budget-dropped mutations and mutation margins.

Promotion eligibility uses the same shape as the observed-state arm: no training regression plus the configured minimum development gain. It is evidence for review only.

## Why retain both H4a arms

The observed-state arm constrains prototypes to actually observed routing states and explicitly contrasts positives against negatives.

The majority-bit arm can synthesize a bounded centroid-like Boolean prototype, but only by strongly supported mutations away from the seed and under a hard mutation budget.

Neither is privileged in advance. The next qualified experiment must compare fixed seeds, observed-state prototypes, majority-bit prototypes and matched random controls on identical train/development/task partitions and report shortlist recall and final task quality separately.

H4b expert-membership learning remains blocked until that matched comparison is complete.
