# A6-I5 — Preregistration: strength-3 passive support and the unavoidable fourth-order blind spot

Frozen on 2026-10-12, **before implementing this experiment or observing its results**. Baseline: merged A6-I4 PR #506 on main at commit `11ff545df2ce4623cdf387f865e4ea302db5b514`. This document is append-only for scientific hypotheses, seeds, corpus, thresholds, metrics and success/failure tests. No tuning from held-out TEST.

## Question and mandatory counterexample

A6-I4 demonstrated that storing all seen Boolean pair patterns is insufficient to detect a previously unseen triple 111. I5 asks whether a **global, exact strength-three empirical input-coverage guard** (not an ANF coefficient estimator) removes that particular confident OOD error while admitting a **mandatory unobservable fourth-order interaction**. Covering all triple input patterns is not proof that the teacher belongs to the hypothesis bank, that the labels are accurate, or that the predictor is calibrated.

Hypothesis H1: with a hidden teacher `g3=h0 XOR r4*r5*r6` absent from the bank and TRAIN restricted to the seven 3-bit patterns except 111, the triple guard should refuse all forced triple111 test contexts; pair guard should not detect them. The model retains correct nontrigger predictions. The teacher and h0 are equal *on the entire censored TRAIN support*.

Hypothesis H2: on the identical passive TRAIN distribution but the genuinely supplied `h0` teacher, the guard should refuse exactly as many correct forced 111 TEST predictions: an honest false-abstention cost.

Hypothesis H3 (mandatory **negative**): with absent teacher `g4=h0 XOR r4*r5*r6*r7`, TRAIN visits all 15 quadruple bit patterns except 1111. Every binary triple combination of those four variables is nevertheless observed. On forced 1111 TEST, the triple guard must remain blind and permit confident **wrong** predictions, including with all its monitored triple patterns covered. No algorithm based only on strength-3 TRAIN input support can deduce the missing teacher term from these labels.

H4: exact full-width strength-three enumeration must fail **before allocation** if the declared combinatorial capacity is insufficient. In particular `C(128,3)=341376` fits the specified `max_triple_slots=350000`, whereas full `C(256,3)=2763520` and `C(1024,3)=178433024` do not. No sparse projection is secretly treated as full-width certification. Partial sealing, mismatched history, test-before-seal and exceedance of retained missing-record budget all fail closed without committing state.

## Data, baselines and freeze

Five physical widths `k=[8,16,32,64,128]`, three independent seed IDs `[0,1,2]`, two TRAIN-label panels clean / Bernoulli(1/16) noise, and three regimes `TRIPLE_ABSENT`, `TRIPLE_INCLUDED`, `QUAD_ABSENT`: **90 fitted policies**, of which noise panels share underlying teachers and input trajectories (not 90 independent data-generating populations). All 90 TRAIN histories and committees must be frozen before any independent TEST sequence is generated.

Each fit has 512 passive TRAIN samples and 512 held-out clean TEST inputs; 256 supported contexts alternate with 256 forced triggers; total **46080 TEST decisions**, 15360 per regime. Independently permute eight role positions throughout the full width by a seed-driven generator; the four candidate gates are supplied and contain only roles r0..r3, with h0=r2 XOR r3, and the three alternatives adding r0*r1, r0, or r1. The hidden teacher selection and random-role mapping are private to the benchmark generator; the guards see only the full-width TRAIN Boolean vectors and do not receive role indices. The committee receives the four candidate gates but not teacher identity. The true hidden terms use r4..r7, deliberately *outside the candidate bank*. Fitting uses no hidden TEST input or truth labels. Seeding must be explicit and deterministic with separate role/TRAIN/noise/TEST streams and identical TRAIN distribution for TRIPLE_ABSENT and TRIPLE_INCLUDED.

TRIPLE TRAIN cycles through 000..110 on roles r4,r5,r6, never 111. QUAD TRAIN cycles through 0000..1110 on r4..r7, never 1111. Other positions are independently Boolean-uniform. TEST alternates r4=0 with the triggering all-one triple or quadruple; non-role positions are Boolean-uniform. The paired panels may share input streams, and should be reported as such.

The committee is the existing native `PassiveCommittee` (four native `WideAnfGate` candidates), prior `[4,2,2,1]/9`, label-noise likelihood 1/16; selective confidence threshold >=0.90, TRAIN bank-adequacy error <=1/8. The comparator is the existing `AnfPairSupportGuard` on exactly the same input histories and TEST. Report accepted, errors *among accepted*, novelty alerts, forced trigger rejections and false abstentions separately. Never count abstention as correct prediction.

Predeclared expected counts if the generated controls and bank thresholds pass:
- H1: triple guard 7680 accepted / 0 errors; pair guard 15360 accepted / 7680 errors; 7680 novel-triple trigger contexts;
- H2: triple guard 7680 accepted / 0 errors and 7680 known-correct trigger refusals;
- H3: triple guard 15360 accepted / **7680 confident errors** and **0 novel triple alerts**; paired teacher absent but invisible throughout TRAIN.
Any observation differing from these expectations is a failing result to publish and diagnose, not permission to change seeds, thresholds or task definitions.

## Implementation and resource envelope

Public Rust `anf_triple_support::AnfTripleSupportGuard`, separate from the I4 pair guard. TRAIN-only bit-sliced `u64` input columns; at `seal()` scan **all** unordered triples i<j<l and all eight 3-bit patterns with masked final word, retaining only triples missing at least one pattern. Query costs O(number of retained missing triples); full sealing is O(C(k,3)*8*ceil(n/64)) worst-case bit operations, **not** scalable by assertion. Fixed budgets: width 3..1024, `max_train=512`, `max_triple_slots=350000`, `max_missing_triples=100000`, fallible allocations and no unsafe. At k128 TRAIN-history primitive payload is 128*8 u64=8192 bytes, **excluding** headers, capacities, scratch and the retained missing records; report record payload independently.

Contract checks include wrong widths, empty TRAIN, no post-seal observations, wrong history length relative to native committee, transparent evidence fields and transactional rejection of missing-entry overflow. A unit test must separately construct a deterministic exhaustive k8 TRAIN corpus with all 3-wise assignments but without quadruple1111 and confirm that this cannot be certified by an order-3 support guard. Another must verify seven-of-eight triple coverage and the exact missing triple111. Negative k256/k1024 resource tests are mandatory.

## Execution and promotion boundary

Pinned Rust 1.89, exact-head checkout, GitHub-hosted Ubuntu24.04 only, read-only pull-request CI, strict fmt/check/clippy/tests plus a reproducible native 90-fit smoke and holdout. Not authorized to use a private RemoteOps runner, CUDA/NVIDIA software beyond driver, unsafe, or to merge without green comprehensive CI at final documentary SHA.

Success means an explicitly **bounded** strength-3 input-novelty detector and a reproduced fourth-order blind spot. No monomial learning, real language prediction, million-function bank, 1024-variable triple certificate, physical device speed, or intuition-human claim is established. Later independent research must examine budgeted/adaptive higher-order observation design and actual intervention economics.
