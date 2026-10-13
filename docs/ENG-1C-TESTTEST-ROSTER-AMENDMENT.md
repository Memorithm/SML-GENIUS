# TESTTEST roster correction before any real model evaluation

Author: Memorithm / Tarek Zekriti. Date: 2026-10-13.
Initial registration: `73e73110ccc628435909cdf393a0330ed17993d3`.

Repository-history audit found that the initially proposed Gutenberg #800
Verne corpus has an exposed A6 TEST (see
`docs/A6-E4-INDEPENDENT-CORPUS-STAGING.md`) and #14155 Flaubert is reserved by
`docs/A6-E5-SPECTRAL-BEAM-PREREGISTRATION.md`. Neither is suitable as a new
confirmation author in this study. Both are excluded. This is a correction
of corpus eligibility before any real TRAIN fit, DEV loss or TEST loss;
no measured loss motivated the change. Synthetic software tests and source
fingerprinting do not constitute real model evaluation.

Replace exactly the two FR novel rows with Victor Hugo, *Les misérables
Tome I: Fantine*, Gutenberg [17489](https://www.gutenberg.org/ebooks/17489),
and Émile Zola, *Germinal*, Gutenberg
[5711](https://www.gutenberg.org/ebooks/5711). Gutenberg catalog metadata
verified on 2026-10-13. No prior evaluation of these two works was found in
the repository's source, scripts or evidence reports. This is an audit of
the available repository, not a claim about all other Memorithm projects.

The ten other works, language/genre balance, FR129/EN64 gate, DEV-selected
classical depths, splits, resource budgets, author weighting, statistical
tests, significance thresholds and effect-size gate remain as registered.
The final roster and complete twelve-work fingerprints must be committed
with this amendment before the first real model evaluation. The original
registration remains inspectable in Git history. The implementation rejects
unregistered manifest hashes, including the superseded roster's fingerprints.

Methodological references, consulted 2026-10-13:
- SciPy developers, *permutation_test* documentation: paired sample exchanges
  and the equivalent one-sample independent sign flips, including exact
  enumeration and one-sided tail conventions:
  https://docs.scipy.org/doc/scipy/reference/generated/scipy.stats.permutation_test.html
- R Core Team, *p.adjust* documentation; Holm, S. (1979), *A Simple Sequentially
  Rejective Multiple Test Procedure*, Scandinavian Journal of Statistics 6,
  65–70: https://stat.ethz.ch/R-manual/R-devel/library/stats/html/p.adjust.html

These references describe assumptions and methods; the Rust implementation
is independent and introduces no SciPy/R runtime dependency. The protocol
deliberately compares its deterministic f64 sign-flip sums without an epsilon;
analytic exact-probability fixtures cover ties. The panel's convenience
sampling and sign-symmetry assumptions limit population inference.
