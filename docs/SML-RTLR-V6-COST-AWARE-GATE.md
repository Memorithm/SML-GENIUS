# RTLR-V6 cost-aware recurrent activation (unmeasured control policy)

Measured paired RTLR-v5 evidence in RemoteOps: [54 native trials](https://github.com/Memorithm/RemoteOps/blob/main/docs/SML_RTLR_V5_SPLIT_TRAIN_MEASURED_20261017.md). On exact single-lag ANF synthetic tasks, recurrent V5 and a simpler Boolean-only decoded feature have **equal DEV BPC**, but recurrent execution is substantially slower. It is not scientifically appropriate to keep recurrence active without a new predictive gain.

This experimental TRAIN-only gate is a conservative path-admission primitive, not a new trained SML model. For each of two disjoint supervised TRAIN data partitions the caller records paired mean loss (bits per target) and measured per-position CPU time (nanoseconds) for an equal-feature Boolean-only candidate and a recurrent hybrid. Recurrence is admitted for a **whole subsequent document only** when **both** TRAIN partitions satisfy:

gain_bpc = boolean_bpc - recurrent_bpc > minimum_gain_bpc + cpu_penalty_bpc_per_ns * max(0, recurrent_ns - boolean_ns).

If data are insufficient, one partition fails, or both models tie, the gate chooses the simpler Boolean route. Once frozen, no DEV update is possible. This removes the need to advance recurrent hidden state during an **entire** document routed Boolean-only; it does not support mid-document hot switching, which would leave recurrent hidden state stale. The caller must reset volatile state at each document boundary and must pass paired measurements on identical source data; these upstream invariants still require pipeline validation.

This library PR is a policy primitive with unit tests for ties, cost, two-partition evidence, malformed values and fail-closed behavior. **No end-to-end speedup, RSS reduction, natural-language quality or new recurrence advantage is claimed.** Next integrate with a benchmark where recurrence actually beats an equally informed Boolean-only model on unseen independent DEV: context-conditioned feature composition, interactions across delays and missing/noisy observations. Measure true wall-clock pipeline time, logical/allocated memory and error.

No SEC data transfer (G0 NOT_ADMITTED), frozen 2024 TEST UNOPENED, no 50M physical MOR/ANF, no 25B training or GPU.
