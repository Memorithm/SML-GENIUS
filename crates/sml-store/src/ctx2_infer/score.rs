//! Scorer (prereg sections 5–7). Reads only generated text and gold labels;
//! arm identity is not an input of [`exact_match`]. No result is produced by
//! this module on its own: status stays `NOT_MEASURED` until a run exists.

use super::SplitMix64;

/// Fixed resampling seed of the session-clustered bootstrap (prereg section 7).
pub const BOOTSTRAP_SEED: u64 = 0xC72_1AFE;
/// Bootstrap resamples (prereg section 7).
pub const BOOTSTRAP_RESAMPLES: usize = 10_000;
/// Family-wise α for the Holm-adjusted H3b family.
pub const FAMILY_ALPHA: f64 = 0.05;
/// Two-sided 95 % normal quantile used by the Wilson interval.
pub const Z95: f64 = 1.959_963_984_540_054;

/// EM: first maximal run of ASCII digits equals `gold` exactly (no digits → false).
#[must_use]
pub fn exact_match(generated: &str, gold: &str) -> bool {
    let b = generated.as_bytes();
    let Some(start) = b.iter().position(u8::is_ascii_digit) else {
        return false;
    };
    let end = b[start..]
        .iter()
        .position(|c| !c.is_ascii_digit())
        .map_or(b.len(), |e| start + e);
    &generated[start..end] == gold
}

/// Wilson 95 % interval for `successes / n`; `None` when `n = 0`.
#[must_use]
pub fn wilson95(successes: u64, n: u64) -> Option<(f64, f64)> {
    if n == 0 || successes > n {
        return None;
    }
    let n_f = n as f64;
    let p = successes as f64 / n_f;
    let z2 = Z95 * Z95;
    let denom = 1.0 + z2 / n_f;
    let centre = (p + z2 / (2.0 * n_f)) / denom;
    let half = Z95 * (p * (1.0 - p) / n_f + z2 / (4.0 * n_f * n_f)).sqrt() / denom;
    Some(((centre - half).max(0.0), (centre + half).min(1.0)))
}

/// Paired 2×2 table of `X` versus A0 on the same queries (prereg section 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct McNemarTable {
    pub n11: u64,
    /// X correct, A0 wrong.
    pub b: u64,
    /// X wrong, A0 correct.
    pub c: u64,
    pub n00: u64,
}

impl McNemarTable {
    /// Build from `(x_correct, a0_correct)` pairs.
    #[must_use]
    pub fn from_pairs(pairs: &[(bool, bool)]) -> Self {
        let mut t = Self::default();
        for &(x, a0) in pairs {
            match (x, a0) {
                (true, true) => t.n11 += 1,
                (true, false) => t.b += 1,
                (false, true) => t.c += 1,
                (false, false) => t.n00 += 1,
            }
        }
        t
    }

    /// One-sided exact p-value for "X has the advantage" (`b > c`).
    #[must_use]
    pub fn p_one_sided(&self) -> f64 {
        mcnemar_one_sided(self.b, self.c)
    }
}

/// `p = Σ_{i=b}^{b+c} C(b+c, i) · 2^-(b+c)`; `p = 1` when `b + c = 0`.
#[must_use]
pub fn mcnemar_one_sided(b: u64, c: u64) -> f64 {
    let n = b + c;
    if n == 0 {
        return 1.0;
    }
    // Log-space binomial terms: ln C(n, i) − n ln 2.
    let ln_fact = |m: u64| -> f64 { (2..=m).map(|v| (v as f64).ln()).sum() };
    let ln_n = ln_fact(n);
    let ln2n = n as f64 * core::f64::consts::LN_2;
    let mut ln_terms = Vec::with_capacity((n - b + 1) as usize);
    for i in b..=n {
        ln_terms.push(ln_n - ln_fact(i) - ln_fact(n - i) - ln2n);
    }
    let max = ln_terms.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let sum: f64 = ln_terms.iter().map(|t| (t - max).exp()).sum();
    (max + sum.ln()).exp().min(1.0)
}

/// Holm step-down adjusted p-values, in input order, capped at 1.
#[must_use]
pub fn holm(p: &[f64]) -> Vec<f64> {
    let m = p.len();
    let mut order: Vec<usize> = (0..m).collect();
    order.sort_by(|&a, &b| p[a].total_cmp(&p[b]).then(a.cmp(&b)));
    let mut adjusted = vec![0.0; m];
    let mut running = 0.0_f64;
    for (rank, &i) in order.iter().enumerate() {
        let v = (p[i] * (m - rank) as f64).min(1.0);
        running = running.max(v);
        adjusted[i] = running;
    }
    adjusted
}

/// Session-clustered percentile bootstrap (95 %) of the mean paired EM
/// difference `X − Y`. `sessions[s]` holds the `(x_correct, y_correct)` pairs
/// of session `s`. `None` when there is no pair.
#[must_use]
pub fn clustered_bootstrap_diff(
    sessions: &[Vec<(bool, bool)>],
    resamples: usize,
    seed: u64,
) -> Option<(f64, f64)> {
    let non_empty: Vec<&Vec<(bool, bool)>> = sessions.iter().filter(|s| !s.is_empty()).collect();
    if non_empty.is_empty() || resamples == 0 {
        return None;
    }
    let stats: Vec<(i64, u64)> = non_empty
        .iter()
        .map(|s| {
            let d: i64 = s.iter().map(|&(x, y)| i64::from(x) - i64::from(y)).sum();
            (d, s.len() as u64)
        })
        .collect();
    let mut rng = SplitMix64::new(seed);
    let mut means = Vec::with_capacity(resamples);
    for _ in 0..resamples {
        let (mut d, mut n) = (0_i64, 0_u64);
        for _ in 0..stats.len() {
            let (sd, sn) = stats[rng.below(stats.len() as u64) as usize];
            d += sd;
            n += sn;
        }
        means.push(d as f64 / n as f64);
    }
    means.sort_by(f64::total_cmp);
    let idx = |q: f64| ((q * (resamples - 1) as f64).round() as usize).min(resamples - 1);
    Some((means[idx(0.025)], means[idx(0.975)]))
}

/// Classification of strict H2 (prereg section 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum H2Outcome {
    Pass,
    Fail,
    Indeterminate,
}

/// Strict H2: prompt and refusal failures fail regardless of determinism; text
/// differences count only when the determinism precheck passed.
#[must_use]
pub const fn classify_h2(
    precheck_passed: bool,
    any_prompt_mismatch: bool,
    any_corruption_accepted: bool,
    any_text_mismatch: bool,
) -> H2Outcome {
    if any_prompt_mismatch || any_corruption_accepted {
        H2Outcome::Fail
    } else if !precheck_passed {
        H2Outcome::Indeterminate
    } else if any_text_mismatch {
        H2Outcome::Fail
    } else {
        H2Outcome::Pass
    }
}
