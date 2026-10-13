# ENG-1c/TESTTEST — prospective multi-author confirmation

Author: Memorithm / Tarek Zekriti. Registered 2026-10-13, before any new
model loss. The registration commit must precede the implementation commit
and every model evaluation. This is a new study, not another threshold grid.

## Question and scope

Does the frozen exact-TRAIN occurrence policy improve held-out byte log loss
over a competitively selected fixed-depth Witten–Bell policy across twelve
previously unused works, six authors per language and three declared genres?
The population is a convenience panel of historical Gutenberg FR/EN prose.
Even a positive verdict is evidence on this panel, never a theorem of universal
superiority, a comparison with all language models, or an SML25B promotion.
The Dumas/Austen works, their authors, their VAL and their original untouched
TEST are excluded. Nothing in this study opens the original TEST.

## Frozen corpus roster

Exactly one work per distinct author; no substitutions or exclusions after
seeing losses. Genres are study strata, not an automatic classification.
Metadata checked against Project Gutenberg catalog entries on 2026-10-13.
URLs and author/title identities are in `scripts/eng1c/testtest-roster.tsv`.

| Language | Stratum | Authors (Gutenberg identifiers) |
|---|---|---|
| FR | novel | Jules Verne (800), Gustave Flaubert (14155) |
| FR | essay | René Descartes (13846), Jean-Jacques Rousseau (65434) |
| FR | science | Claude Bernard (16234), Jean-Henri Fabre (16825) |
| EN | novel | Charles Dickens (98), Mary Shelley (84) |
| EN | essay | Ralph Waldo Emerson (2944), John Stuart Mill (34901) |
| EN | science | Charles Darwin (1228), Michael Faraday (14474) |

Source fingerprinting may inspect bytes to verify identity and boundaries,
but must not fit or score a predictor. Raw and normalized SHA256, lengths,
and TRAIN/DEV/TEST SHA256 are committed before the first model evaluation.
The UTF-8 raw source has CR removed, Gutenberg START/END marker lines
excluded, and all remaining lines retained exactly (including front matter
inside the body). No lowercasing, whitespace repair or deduplication within
a work. Invalid UTF-8, ambiguous markers, source drift, duplicate authors,
duplicate bodies, undeclared rows and missing sources stop the whole study.
No corpus or trained store is committed or uploaded as evidence.

For body length N: a=floor(3*N/5), b=floor(4*N/5).
TRAIN=body[0..min(a,600000)], DEV=body[a..min(b,a+32768)],
TEST=body[b..min(N,b+65536)]. The three intervals are disjoint; omitted gaps
and tails are reported. Each scored split uses the existing 64 contiguous
lanes with a history reset at lane start and t>=1. Its exact denominator is
64*(floor(split_bytes/64)-1); lane residues are reported, never scored.
All splits must contain at least 4096 bytes. These caps are frozen sampling
choices, not claims about an entire work. Source identity is checked again
before each phase. Hashing TEST bytes is not prediction or label exposure
for policy selection.

## Frozen predictors and sealing

The adaptive gate is FR129 / EN64. Choose the deepest of 32,12,7 whose exact
context TRAIN count meets that threshold, otherwise depth4. It reads only
the observed prefix and immutable TRAIN counts, never the current target.
Counts do not change on DEV or TEST. Thresholds cannot be tuned here.

Classical arms are FIXED_4/7/12/32 with the same Witten–Bell implementation
and same immutable depth32 store as the adaptive arm. For each work, choose
the classical depth with smallest unrounded DEV mean loss (strict <; ties
prefer 4 then 7 then 12 then 32). Do not refit on DEV. This per-work tuning
gives the classical comparator a genuine held-out development opportunity.
The primary comparator is that DEV-selected classical depth. FIXED_4 is a
prespecified secondary control; every fixed TEST depth is retained as a
diagnostic, with no TEST-based policy selection.

Two separate commands are required: `seal` fits TRAIN and scores DEV only;
`test` consumes the completed seal and scores TEST. A seal binds the corpus
manifest, source/predictor identities, TRAIN/DEV hashes, four DEV losses and
the selected classical depth. TEST rejects a changed seal, manifest, source
or executable, and requires a new output path. Interrupted or incomplete
panels have no scientific verdict. A new threshold or protocol after TEST
exposure is exploratory and requires another independent panel.

## Endpoints, uncertainty and decision rule

For author i, delta_i=classical_TEST_bpb-adaptive_TEST_bpb, positive is better.
The primary estimate is the equally weighted mean of all 12 authors (each
language and each genre has equal representation). Report per-work scores,
language/genre means, wins/ties/losses, worst regression and a secondary
token-weighted mean. Neither lanes nor tokens are independent study units.

Approximate paired author bootstrap: 9999 draws, SplitMix64 seed
0x5454_2026_1013_B001, resample six authors with replacement separately in
FR and EN, retain equal language weight, sort and use zero-based indices
249/9749 for 95% bounds. Report pooled and per-language bounds. This assumes
authors approximate independent sampling units; it does not make the roster
a random sample of all prose.

Exact one-sided sign-flip tests enumerate all 2^12 (pooled) and 2^6
(per-language) paired author sign assignments under a symmetric zero-effect
null. Count permuted means >= observed mean, including the observed assignment;
no Monte Carlo p-values or arbitrary epsilon. Pooled p<=0.05 is required.
The two language tests use Holm: smaller p<=0.025, larger p<=0.05.
Inference is conditional on author independence and sign symmetry; these
assumptions cannot be certified by a small convenience roster.

`PANEL_CONFIRMATION_PASS` requires all of:
- exactly twelve admitted works, six per language, two per genre/language;
- integrity checks, finite probabilities/scores, all denominators complete;
- pooled and each language mean primary gain >=0.010000 bits/byte;
- pooled and each language bootstrap lower bound >0;
- pooled sign-flip and Holm-corrected language conditions above;
- at least four strict primary author wins in each language;
- nonnegative primary mean in every language/genre stratum;
- nonnegative pooled and language means versus secondary FIXED_4.

Otherwise `PANEL_SUPERIORITY_NOT_ESTABLISHED`. Software correctness and a
negative scientific outcome are both successful completed executions. A
capacity/integrity/download failure is an execution error, never a loss or
an excluded author. Do not stop early after a favorable subset, extend the
panel after seeing results, or reinterpret the thresholds/endpoint.

## Resources, qualification and reproducibility

Existing CAP2b depth32 limits apply, with <=600000 TRAIN bytes per work,
262144 KiB process virtual-address limit and 3000s deadline per work phase.
One work at a time per language, at most two language workers; stores can be
removed after their sealed TEST result. No GPU, model-resource saving or
production latency assertion. The same full depth32 representation supports
every arm, so this study tests prediction quality only.

Meaningful tests must cover target-independent gating, DEV selection opposed
by a better TEST-only depth, source/seal corruption and missing/duplicate
works, independent n-gram oracle parity, known exact sign-flip probabilities,
macro versus token-weighted reversal, and an actual two-phase synthetic run.
Synthetic runs are software qualification, never evidence on real language.
CI archives compact hashes/seals/scores/aggregate verdicts only, at the exact
PR head; mandatory Rust 1.89 fmt/Clippy/workspace tests must pass before merge.
The first real evaluation uses a published implementation commit plus the
previously committed fingerprint manifest. Every subsequent run on these
TEST splits is replication, not new independent confirmation.

Generic paired-author analysis accepts checked rows independently of the
predictor and is reusable for later SML comparisons. The remote execution
entry point is a GitHub Actions dispatch under SML scientific authority;
RemoteOps remains the distant control plane. Reuse by SciRust/ElasticXxx
requires their own source fixtures and endpoint qualification. This study
introduces no mandatory dependency on those repositories.
