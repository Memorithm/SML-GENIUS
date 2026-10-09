//! Distinguish observed resident samples from actual peak memory.
#[derive(Debug, PartialEq, Eq)]
pub enum Observation {
    Missing,
    Sampled(u64),
}
pub fn observation(samples: &[u64]) -> Observation {
    match samples.iter().copied().max() {
        None | Some(0) => Observation::Missing,
        Some(n) => Observation::Sampled(n),
    }
}
pub fn comparable(a: &Observation, b: &Observation) -> bool {
    matches!((a, b), (Observation::Sampled(_), Observation::Sampled(_)))
}
fn main() {
    println!("A4_RSS_IS_SAMPLED_NOT_PEAK");
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zero_is_not_zero_memory() {
        assert_eq!(observation(&[]), Observation::Missing);
        assert_eq!(observation(&[0]), Observation::Missing);
    }
    #[test]
    fn no_false_comparison() {
        assert!(!comparable(
            &Observation::Missing,
            &Observation::Sampled(20)
        ));
    }
}
