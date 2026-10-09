//! Validate bounded continuation policy without mutating runtime state.
pub fn remaining(done: u32, minimum: u32, maximum: u32) -> Result<(u32, u32), &'static str> {
    if maximum == 0 || minimum == 0 || minimum > maximum || done > maximum {
        return Err("policy");
    }
    if done == maximum {
        return Ok((0, 0));
    }
    Ok((minimum.saturating_sub(done).max(1), maximum - done))
}
fn main() {
    println!("A4_CHECKPOINT_POLICY_VALIDATOR");
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split() {
        assert_eq!(remaining(2, 4, 8), Ok((2, 6)));
        assert_eq!(remaining(8, 4, 8), Ok((0, 0)));
    }
    #[test]
    fn invalid() {
        assert!(remaining(9, 4, 8).is_err());
    }
}
