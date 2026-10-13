# ENG-1c BETA-DEV — frozen exploratory estimator grid

Author: Memorithm / Tarek Zekriti. Registered 2026-10-13 before new DEV
measurements. This is step 2 after the negative TESTTEST FR129/EN64 panel.
It neither replaces that result nor confirms a new policy on its exposed TEST.
Claude's `/tmp/claude-0/wbx/` script is unavailable in this workspace;
the formula below defines our implementation, without claiming script parity.

## Exact estimator and frozen grid

For an observed TRAIN context c at any depth including depth 0:

`p_beta(y|c) = n(y)/(N + beta*U) + beta*U/(N + beta*U)*p_beta(y|suffix(c))`

N is the total TRAIN follower-event count and U the number of distinct
observed follower bytes. Begin with uniform 1/256; missing contexts back off.
One positive finite beta applies at every depth, including the root.
Beta >1 increases the escape mass before backoff. Counts, keys and the
TRAIN model are unchanged. This changes the probability estimator, not a
resource policy or the occurrence gate. Invalid beta/nonfinite probabilities
must fail. Floating arithmetic is f64, with the existing normalization
tolerance 1e-10. Beta=1 must bitwise reproduce all 256 old probabilities,
context-event counts, depths and actual DEV total-bit aggregates.

Betas in fixed selection order: **[1, 0.25, 0.5, 2, 4, 8]**.
Depths in fixed order: **[4, 7, 12, 32]**. Exactly 24 beta/depth candidates.
One secondary arm: unchanged old occurrence gate FR129/EN64 at beta=1,
for 25 predictive rows per work. No additional values after seeing scores.
Strict unrounded `<` selects the best; ties retain the earlier beta and then
the shorter depth, so beta=1 is preferred in an exact tie. Boundary beta
0.25/8 is reported, never used to extend this registered grid.

Primary descriptive comparison: each work's minimum DEV loss over the 24
candidates versus its minimum over the four beta=1 classical depths.
The classical set is contained in the candidate set: nonnegative best-grid
DEV gain is guaranteed by selection and is not an independent benefit.
Report the entire grid, exact selected beta/depth, old gate, pooled/language
means and TRAIN-size strata. No significance tests or scientific PASS from
these tuning losses. Endpoint-only, beta=1 wins and negligible improvements
must be retained, not excluded.

## Frozen data, splits and TEST closure

Reuse exactly the twelve-work eligible TESTTEST roster and TRAIN/DEV hashes
registered at 0bf47640eb769f6b6ed7a4efeff7a98269880f3f. No Opticks or extra
text, no new TRAIN sizes in this step. DATA identities are metadata from
`scripts/eng1c/testtest-manifest.tsv`, SHA256
`ffd768bfe236eb1c7e9cee689d84c50702d551315dfe3327ed9dced10074e990`.
TRAIN remains first 60% capped at 600000 bytes. DEV begins at floor(3*N/5),
capped at 32768 bytes. Same 64 lanes, reset history and t>=1 denominator.
Each of the twelve DEV splits has 32704 target events.

The new grid executable accepts **TRAIN and DEV paths only**, rejects their
wrong lengths/hashes before fitting, and has no TEST scoring command/path.
Source preparation reads the raw header and normalized body prefix only
through DEV's end, then closes it. It does not read/hash the remaining
body or construct a TEST file. Raw source length, header identity and both
split hashes are checked. This validates the consumed TRAIN/DEV data, not
the unread tail's current whole-source hash. Network download may fetch
the original source file; that does not authorize the scorer to read TEST.
The prior archived TEST losses are not inputs to this grid or its selection.

Training-size strata fixed before scoring:
small <262144 bytes; medium 262144<=bytes<524288; large >=524288.
Report each author and each language within these bins, including small
groups without inferential claims. Volumes vary across works and cannot
isolate a causal TRAIN-volume effect. Reusing DEV after inspecting the
panel outcome is explicitly exploratory.

## API, validation and resources

Preserve the existing depth_ladder_readout implementation as a beta=1
reference. Add a parameterized scalar readout and a bounded batched readout
for up to 16 betas. The batch shares context-count lookup across all betas,
without claiming throughput improvement before measurement. It must match
scalar beta readouts and the independent legacy oracles at beta=1.
New beta math must have an independent direct TRAIN-frequency oracle,
normalization/positivity tests, unseen/short-history cases, count-invariance
and bad-beta rejection. Grid tests cover endpoints/order/ties, wrong split
identity, bounded source-prefix reading, TEST-path rejection, and an actual
TRAIN/DEV fixture with beta=1 aggregate score parity.

Rust 1.89 release, existing CAP2b depth32 quotas, 262144-KiB virtual-process
limit and 3000-second phase deadline. One work at a time per language,
at most two workers. No TRAIN/DEV corpus or learned store uploaded; retain
compact raw grids, metadata, hashes, complete summary and source/binary identity.
Exact-head general CI and dedicated FR/EN DEV-only checks must be green
before merge. A complete unchanged/no-benefit result is software success.
SML retains estimator/scientific authority; RemoteOps remains the distant
control plane. No GPU, serving, memory-saving or general-superiority claim.

## Next independent confirmation, after this diagnostic

A later preregistration must define new unexposed works, the same DEV/TEST
per work across nested 128/256/512-KiB TRAIN prefixes, fixed beta/depth
selection rules, an author-level analysis accounting for repeated volumes,
power/precision and an independently justified practical effect size.
Classic beta=1 DEV-selected depth is primary; beta/depth DEV-selected
candidate is primary; the old gate remains secondary. Previously exposed
TESTTEST and any Opticks used for development are excluded from confirmation.
No new confirmatory panel is opened by this step's code or CI.
