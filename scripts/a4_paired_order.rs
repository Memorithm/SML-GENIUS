//! Generate a deterministic, balanced AB/BA experimental schedule.
pub fn schedule(pairs: usize) -> Result<Vec<(usize, &'static str, &'static str)>, &'static str> {
    if pairs == 0 || pairs > 10000 {
        return Err("invalid pairs");
    }
    Ok((1..=pairs)
        .map(|i| {
            if i % 2 == 1 {
                (i, "baseline", "candidate")
            } else {
                (i, "candidate", "baseline")
            }
        })
        .collect())
}
fn main() {
    for (i, a, b) in schedule(10).unwrap() {
        println!("PAIR,{i},{a},{b}");
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alternates() {
        let s = schedule(4).unwrap();
        assert_eq!(s[0], (1, "baseline", "candidate"));
        assert_eq!(s[1], (2, "candidate", "baseline"));
    }
    #[test]
    fn bounds() {
        assert!(schedule(0).is_err());
        assert!(schedule(10001).is_err());
    }
}
