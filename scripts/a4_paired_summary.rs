//! Summarize matched nanosecond durations without making speedup claims.
pub fn summarize(pairs: &[(u64, u64)]) -> Result<(i64, usize, usize), &'static str> {
    if pairs.is_empty() {
        return Err("empty");
    }
    let mut diffs = Vec::new();
    let mut wins = 0;
    for &(before, after) in pairs {
        if before == 0 || after == 0 || before > i64::MAX as u64 || after > i64::MAX as u64 {
            return Err("invalid time");
        }
        if after < before {
            wins += 1;
        }
        diffs.push(before as i64 - after as i64);
    }
    diffs.sort_unstable();
    let n = diffs.len();
    Ok((diffs[n / 2], wins, n))
}
fn main() {
    println!("A4_PAIRED_SUMMARY_TOOL,requires_explicit_samples");
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn median() {
        assert_eq!(summarize(&[(20, 10), (10, 12), (30, 20)]), Ok((10, 2, 3)));
    }
    #[test]
    fn invalid() {
        assert!(summarize(&[]).is_err());
        assert!(summarize(&[(0, 1)]).is_err());
    }
}
