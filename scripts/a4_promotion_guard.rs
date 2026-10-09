//! Require independent evidence before promoting an architecture result.
#[derive(Clone, Copy)]
pub struct Evidence {
    pub exact: bool,
    pub matched: bool,
    pub measured: bool,
    pub learned: bool,
}
pub fn may_claim_performance(e: Evidence) -> bool {
    e.exact && e.matched && e.measured
}
pub fn may_claim_model_quality(e: Evidence) -> bool {
    e.exact && e.learned
}
fn main() {
    println!("A4_PROMOTION,BLOCKED,requires_matched_measured_evidence");
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn correctness_is_not_speed() {
        let e = Evidence {
            exact: true,
            matched: false,
            measured: false,
            learned: false,
        };
        assert!(!may_claim_performance(e));
        assert!(!may_claim_model_quality(e));
    }
    #[test]
    fn independent_gates() {
        let e = Evidence {
            exact: true,
            matched: true,
            measured: true,
            learned: false,
        };
        assert!(may_claim_performance(e));
        assert!(!may_claim_model_quality(e));
    }
}
