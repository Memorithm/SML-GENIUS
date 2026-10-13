//! Predictor-independent paired study statistics. Tokens are weights only;
//! independent study units must be supplied by the caller, never inferred.

use crate::toy_tasks::SplitMix64;

pub const PANEL_DRAWS: usize = 9999;
pub const PANEL_SEED: u64 = 0x5454_2026_1013_B001;

#[derive(Clone, Copy, Debug)]
pub struct PairedSummary {
    pub units: usize,
    pub mean: f64,
    pub weighted_mean: f64,
    pub wins: usize,
    pub ties: usize,
    pub losses: usize,
    pub worst: f64,
    pub sign_flip_p: f64,
}

/// Exact one-sided sign flip under the independent symmetric zero-effect null.
/// At most twenty units: enumeration is explicitly bounded, with ties included.
pub fn paired_summary(gains: &[f64], weights: &[u64]) -> Result<PairedSummary, &'static str> {
    if gains.is_empty()
        || gains.len() > 20
        || gains.len() != weights.len()
        || gains.iter().any(|v| !v.is_finite())
        || weights.contains(&0)
    {
        return Err("invalid paired study units");
    }
    let weight = weights
        .iter()
        .try_fold(0u64, |n, w| n.checked_add(*w))
        .ok_or("paired weight overflow")?;
    let observed: f64 = gains.iter().sum();
    if !observed.is_finite() {
        return Err("nonfinite paired sum");
    }
    let assignments = 1usize << gains.len();
    let mut extreme = 0usize;
    for mask in 0..assignments {
        let sum: f64 = gains
            .iter()
            .enumerate()
            .map(|(i, v)| if mask & (1 << i) == 0 { *v } else { -*v })
            .sum();
        extreme += usize::from(sum >= observed);
    }
    let weighted_mean = gains
        .iter()
        .zip(weights)
        .map(|(v, w)| v * (*w as f64 / weight as f64))
        .sum();
    Ok(PairedSummary {
        units: gains.len(),
        mean: observed / gains.len() as f64,
        weighted_mean,
        wins: gains.iter().filter(|v| **v > 0.0).count(),
        ties: gains.iter().filter(|v| **v == 0.0).count(),
        losses: gains.iter().filter(|v| **v < 0.0).count(),
        worst: gains.iter().copied().fold(f64::INFINITY, f64::min),
        sign_flip_p: extreme as f64 / assignments as f64,
    })
}

/// Paired author resampling within groups, then equal group weight.
/// Empty/nonfinite groups stop. This is a sampling-assumption-dependent interval.
pub fn paired_group_interval(groups: &[&[f64]]) -> Result<(f64, f64), &'static str> {
    if groups.is_empty()
        || groups
            .iter()
            .any(|g| g.is_empty() || g.iter().any(|x| !x.is_finite()))
    {
        return Err("invalid bootstrap groups");
    }
    let mut rng = SplitMix64::new(PANEL_SEED);
    let mut draws = Vec::with_capacity(PANEL_DRAWS);
    for _ in 0..PANEL_DRAWS {
        let mut mean = 0.0;
        for group in groups {
            let mut sum = 0.0;
            for _ in 0..group.len() {
                sum += group[(rng.next_u64() % group.len() as u64) as usize];
            }
            mean += sum / group.len() as f64 / groups.len() as f64;
        }
        draws.push(mean);
    }
    if draws.iter().any(|v| !v.is_finite()) {
        return Err("nonfinite bootstrap draw");
    }
    draws.sort_by(f64::total_cmp);
    Ok((draws[249], draws[9749]))
}

pub fn holm_two_pass(a: f64, b: f64) -> bool {
    a.is_finite() && b.is_finite() && a >= 0.0 && b >= 0.0 && a.min(b) <= 0.025 && a.max(b) <= 0.05
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_known_null_probabilities_and_ties() {
        assert_eq!(
            paired_summary(&[1.0; 6], &[1; 6]).unwrap().sign_flip_p,
            1.0 / 64.0
        );
        assert_eq!(paired_summary(&[0.0; 6], &[1; 6]).unwrap().sign_flip_p, 1.0);
        assert_eq!(
            paired_summary(&[-1.0; 6], &[1; 6]).unwrap().sign_flip_p,
            1.0
        );
        assert_eq!(
            paired_summary(&[1.0, -1.0], &[1, 1]).unwrap().sign_flip_p,
            0.75
        );
        assert!(!holm_two_pass(0.03, 0.03));
        assert!(holm_two_pass(1.0 / 64.0, 1.0 / 64.0));
    }

    #[test]
    fn author_and_token_weighting_can_reverse_conclusion() {
        let s = paired_summary(&[0.02, -0.01], &[1, 100]).unwrap();
        assert!(s.mean > 0.0 && s.weighted_mean < 0.0);
        assert_eq!((s.wins, s.ties, s.losses), (1, 0, 1));
        assert!(paired_summary(&[f64::NAN], &[1]).is_err());
        assert!(paired_summary(&[1.0], &[0]).is_err());
        assert!(paired_summary(&[1.0; 21], &[1; 21]).is_err());
        assert!(paired_summary(&[1.0, 1.0], &[u64::MAX, 1]).is_err());
    }

    #[test]
    fn group_weights_and_replay_are_deterministic() {
        let a = [0.01; 6];
        let b = [0.03; 6];
        let interval = paired_group_interval(&[&a, &b]).unwrap();
        assert!((interval.0 - 0.02).abs() < 1e-15);
        assert_eq!(interval, paired_group_interval(&[&a, &b]).unwrap());
        assert!(paired_group_interval(&[&[]]).is_err());
    }
}
