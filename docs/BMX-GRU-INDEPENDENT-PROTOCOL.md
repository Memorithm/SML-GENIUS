# BMX-v5 + frozen Dell GRU: prospective independent-panel protocol v1

Status: no DEV or TEST quality has been observed when this protocol is authored.
Execution is permitted only after this protocol, complete hashed manifest and
qualified source are published on an immutable research commit. No main merge.

## Question and scope

Does the frozen native Dell GRU improve BMX-v5 conditional code length on this
new convenience panel? This is not a universal compression claim, a comparison
against all classical models, or evidence of corpus-wide pretraining exclusion.
The training corpus was not fully hashed/pinned. New authors/genres and Rust
repositories reduce known exposure; that cannot establish total disjointness.

## Fixed candidates and inputs

Use the three arms, native kernel, original/native weight hashes and all default
BMX-v5 settings in `BMX-GRU-NATIVE-QUALIFICATION.md`. No parameter grid, network
training, weight updates, model selection or post-TEST tuning. BMX alone learns
online in every arm. DEV is qualification/descriptive only, not selection.

`scripts/eng1d/gru-panel.py` fixes twelve units: Sand, Daudet, Huysmans, Loti
(FR novels); Dolbear, Tyndall, Lodge, Maxwell (EN nonfiction); one source file
each from pinned tokio, rayon, serde, regex repositories (RS). One author or
repository is one inferential unit; byte blocks and phase repeats are not units.
Selection is by previously untested identity, genre and sufficient byte length,
not by model score. There are four units per domain, not a representative sample.
An initial preparation attempt used Eddington 29782, which has PDF/TeX only
and returned HTTP 404 for plain text. It was replaced by Dolbear 29444 before
any model scoring or protocol publication. The failed preparation is retained.
A second preparation encountered a Serde path HTTP 404. Its pinned tree places
the same intended deserializer module at `serde_core/src/de/mod.rs`; that path
was corrected before scoring/publication, without changing repository/commit.

Exact downloaded bytes are retained without newline/Unicode normalization.
For Gutenberg, remove only START/END wrapper lines, retain all body/front matter.
For Rust use the entire pinned file. Use byte intervals on the resulting body:

| Phase | Half-open byte interval | Bytes |
|---|---|---:|
| PRIME | [0, 16384) | 16384 |
| DEV | [16384, 24576) | 8192 |
| TEST | [24576, 40960) | 16384 |

Reset predictors independently for every arm/phase and prime all with the same
PRIME. TEST does not consume DEV history. Finish all twelve DEV before TEST.
Compute every arm for every unit, no early winner selection. Retain incomplete
and failed cases, never replace a source after scoring. A failed integrity,
download/hash, size, replay or complete-panel check prevents a positive verdict.

## Metrics and decision

Primary unit gain: `(v5.model_bits - v5-gru.model_bits) / TEST_bytes`, in
bits/byte, positive means GRU better. Equal-weight twelve-unit mean, equivalent
to equal-weight domain means since domain counts/sizes are equal. Report all
unit gains, each domain, placebo gains, DEV, ideal/effective bits, real payload
and full container sizes separately. Do not use training loss as primary data.

Minimum worthwhile conditional gain delta = **0.01 bit/byte**: an engineering
target equivalent to 1.28 KiB saved per MiB of raw input, not the old WB threshold
and not a power claim. A positive panel verdict requires all simultaneously:

1. All twelve TEST units and all three arms pass full-input replay/integrity.
2. The lower 2.5% percentile of 9999 stratified bootstrap mean gains exceeds
   delta. Resample four units with replacement within each fixed domain, using
   Python `random.Random(20261014)`, fixed FR/EN/RS and source order.
   Quantiles use zero-based floor indices `int(q * (9999 - 1))`.
3. Exact one-sided sign-flip test of unit gains minus delta has p <= 0.05:
   enumerate all 4096 sign configurations, including observed, count means at
   least as high as observed, divide by 4096. This test relies on independent
   units and sign symmetry; convenience panel/related software limits inference.
4. Each domain has a nonnegative mean gain and at least three strictly positive
   unit gains. Four units cannot establish separate per-domain significance.

If a complete valid panel fails these criteria, report **superiority not
established**, not equivalence or proof that all possible GRU integrations lose.
No multiple alternative thresholds, optional stopping or selective reporting.

## Cost and execution boundaries

Primary comparisons condition on shared pretrained weights. Report incremental
native weights (4,142,128 bytes), memory, header overhead and CPU timing. Also
report total size if weights must be bundled, and amortization break-even
`8 * native_weight_bytes / positive_mean_gain` raw bytes (undefined if <=0).
Do not call the conditional gain a self-contained-file size victory.

CPU native Rust only; no CUDA, PyTorch inference, privileged host changes or
production services. Same-build roundtrip is mandatory; cross-architecture
floating replay is not qualified. Record source, weights, binary and corpus
hashes, environment, precise run/commit identifiers and all failures. CPU shared
host timings are diagnostics, not controlled throughput superiority evidence.
Bound total supervised evaluation to 345 minutes, sequential cases, no Thor
training retry and no automatic scientific promotion or merge.
