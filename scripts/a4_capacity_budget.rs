//! Checked workload sizing for larger ANF banks; not a VRAM estimator.
pub fn work(gates: u64, queries: u64, depth: u64) -> Result<u64, &'static str> {
    if gates == 0 || queries == 0 || depth == 0 {
        return Err("zero");
    }
    gates
        .checked_mul(queries)
        .and_then(|n| n.checked_mul(depth))
        .ok_or("overflow")
}
fn main() {
    for k in [8192, 65536, 1048576] {
        println!("A4_WORK,K={k},gate_queries={}", work(16, k, 8).unwrap());
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn large() {
        assert_eq!(work(16, 1048576, 8), Ok(134217728));
    }
    #[test]
    fn overflow() {
        assert!(work(u64::MAX, 2, 2).is_err());
    }
}
