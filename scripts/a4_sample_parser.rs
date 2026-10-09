//! Strictly parse the diagnostic's source, pass, and observed RSS fields.
pub fn parse(s: &str) -> Result<(&str, u32, u64), &'static str> {
    let fields: Vec<_> = s.trim().split(',').collect();
    if fields.len() != 5 || fields[0] != "A4_COW_SAMPLE" {
        return Err("shape");
    }
    let source = fields[1].strip_prefix("source=").ok_or("source")?;
    if source.len() != 40 || !source.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("sha");
    }
    let pass = fields[2]
        .strip_prefix("pass=")
        .ok_or("pass")?
        .parse::<u32>()
        .map_err(|_| "pass")?;
    let rss = fields[3]
        .strip_prefix("observed_rss_kib=")
        .ok_or("rss")?
        .parse::<u64>()
        .map_err(|_| "rss")?;
    if pass == 0
        || pass > 10000
        || !fields[4].starts_with("output_sha256=")
        || fields[4].len() != 78
    {
        return Err("bounds");
    }
    Ok((source, pass, rss))
}
fn main() {
    println!("A4_SAMPLE_PARSER_READY");
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reject_malformed() {
        assert!(parse("A4_COW_SAMPLE,source=bad").is_err());
        assert!(parse("A4_COW_SAMPLE,source=0000000000000000000000000000000000000000,pass=0,observed_rss_kib=1,output_sha256=0000000000000000000000000000000000000000000000000000000000000000").is_err());
    }
}
