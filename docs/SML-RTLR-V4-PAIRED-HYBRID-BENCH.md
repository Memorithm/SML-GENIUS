# Paired CPU validation design — RTLR-ELASTIC Boolean hybrid

This is the first strict native-performance qualification for the experimental **RTLR Boolean hybrid selector** (stacked on SML [#615](https://github.com/Memorithm/SML-GENIUS/pull/615)).

The binary `sml-rtlr-boolean-hybrid-bench` compares an independently trained plain **RTLR-V3 max8** recurrence against a second RTLR-V3 max8 coupled to the TRAIN-learned Boolean ANF lag selector. Both use exact same source panels, model seed, 524,288 TRAIN positions (8 documents), 65,536 independent generated read-only DEV positions, learning rate, causal 1024-byte burn-in and TRAIN-only document-width controller. The recurrent-only baseline is trained completely separately; there is no transfer of labels, recurrent weights or expert scores between models. The hybrid receives the same source inputs but also a **given ANF feature and predeclared Boolean candidate set [1,16,128,1023]**, which is an architectural advantage that MUST be disclosed.

Each run takes `PANEL LAG SEED UNIQUE_ROOT`, with deterministic synthetic source panels 0/1/2, targets at lag **1,16,64,128,1023**, and model seeds 11,23,37. Lag64 is intentionally **absent from the candidate bank**: its performance tests delay generalization rather than memorizing the given candidate labels. There are 45 unique processes per host, 90 paired CPU host processes, for 47,185,920 repeated TRAIN bit-position exposures and 5,898,240 DEV exposures, not independent text samples.

Output `RTLR_BOOLEAN_HYBRID_V1` has 21 TSV columns:
- panel, label delay, initialization seed, TRAIN and DEV count;
- standalone V3 and hybrid DEV logloss (both bits/target-position);
- both measured TRAIN/DEV CPU recurrent-kernel rates;
- standalone active width, hybrid active width (must be read from actual model; do not infer from capacity);
- packed ring backing bytes and hybrid logical active payload bytes, excluding allocator and dormant banks;
- selector probabilities for recurrent branch, lag1, lag16, lag128, lag1023.

DEV is strictly read-only for both the baseline recurrent weight state and hybrid selector. Train-only width decisions are based on each model's **recurrent raw prediction loss vs its own anchor**, not the hybrid mixture's mixed loss; therefore selector success does not fake structural improvement.

The evidence gate is not simply whether the hybrid wins on a task with an explicitly predeclared correct lag. It must beat the TRAIN-only constant null and baseline V3 **on independent data panels**, while accurately reporting the unknown-lag64 negative control, CPU/heap costs and candidate-selection advantages. Only then is further research into learned arbitrary-delay retrieval justified.

This program does not load the physical 50M paged MOR/ANF experts, does not qualify natural-language learning or a trained SML25B, and uses no GPU or NVIDIA software. SEC rights **G0 NOT_ADMITTED**, frozen 2024 TEST **UNOPENED**.
