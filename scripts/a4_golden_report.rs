//! Verify an exact golden report by checking its shape and terminal summary.
use std::io::{self, Read};
pub fn verify(report: &str) -> Result<(), &'static str> {
    let lines: Vec<_> = report.lines().collect();
    if lines.len() != 675 {
        return Err("line count");
    }
    if !lines[0].starts_with("A4_CLOSED_LOOP,schema=1,") {
        return Err("header");
    }
    if !lines.iter().any(|x|x==&"A4_CLOSED_SUMMARY,cases=108,cycles=456,key_changes=158,route_changes=28,full_pipeline_resumes=108,mismatches=0"){return Err("summary");}
    if lines.iter().filter(|x| x.starts_with("CASE,")).count() != 108 {
        return Err("cases");
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut s = String::new();
    io::stdin().take(2_000_000).read_to_string(&mut s)?;
    verify(&s).map_err(io::Error::other)?;
    println!("A4_REPORT_SHAPE_OK");
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_header_fails() {
        assert_eq!(verify(""), Err("line count"));
    }
    #[test]
    fn false_shape_fails() {
        let s = "A4_CLOSED_LOOP,schema=1,\n".repeat(675);
        assert_eq!(verify(&s), Err("summary"));
    }
}
