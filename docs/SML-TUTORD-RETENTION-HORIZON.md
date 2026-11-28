# Cumulative retention horizon and bounded canary budget

Simulation-only opt-in `cumulative_limit=N` extends the total root budget
from the default 64 to at most 512 cycles, with at most 8192 mathematical
entries. The separate per-process max_cycles remains mandatory. This is
not an indefinite production mode; full-snapshot manifest growth remains
a blocker. The already-running physical endurance is not changed.

Startup now reconstructs each expected job identity from seed, curriculum,
prior banks and retention policy, including compacted successes. All known
jobs must belong to this exact sequence, with at most one final pending
job; the store version and full audit must agree. Changing the curriculum
or policy fails closed even when old job records have been compacted.

Optional `retention_history=1` binds dyadic-v1 into job IDs (legacy IDs are
unchanged when disabled). In each cycle the immediate predecessor is
examined. Starting at seed 2, the extra review traverses earlier seeds:
for seed in [2^k, 2^(k+1)), revisit seed - 2^k. This revisits every earlier
cycle in a later complete epoch with a bounded maximum of two prior
mathematical curricula per cycle. It is deterministic spaced review,
not a performance-adaptive or general gap-detection policy. No teaching,
new certification, or duplicate acquisition is counted for these exams.

Tests force compaction after one full receipt, reopen the actual supervisor
four times, independently examine old mathematical checkpoints, verify
all 16 entries, and reject policy/curriculum changes. An explicit long
physical cumulative canary remains necessary before admission.
