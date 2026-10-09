//! Check the frozen closed-loop SHA-256 string supplied by a trusted hash tool.
const GOLDEN: &str = "7ac9f94f1f98680e6d3789f5840433917d5bbeba0cd6dd8a8e0b61c9f5bcf0dc";
pub fn admit(hash: &str) -> Result<(), &'static str> {
    if hash.trim() == GOLDEN {
        Ok(())
    } else {
        Err("golden drift")
    }
}
fn main() {
    let h = std::env::args().nth(1).unwrap_or_default();
    if let Err(e) = admit(&h) {
        eprintln!("{e}");
        std::process::exit(1);
    }
    println!("A4_GOLDEN_HASH_OK");
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact() {
        assert!(admit(GOLDEN).is_ok());
    }
    #[test]
    fn corrupted() {
        assert!(admit("0").is_err());
    }
}
