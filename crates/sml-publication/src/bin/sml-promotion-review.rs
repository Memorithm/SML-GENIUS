use std::path::Path;
fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 5 {
        return Err("usage: sml-promotion-review DOSSIER RUN OWNER POLICY ALLOWED_SIGNERS".into());
    }
    let token = std::env::var("GITHUB_TOKEN").ok();
    let receipt = sml_publication::promotion_review::review_authenticated_evidence(
        Path::new(&args[0]),
        &args[1],
        Path::new(&args[2]),
        Path::new(&args[3]),
        Path::new(&args[4]),
        token.as_deref(),
    )?;
    println!("{}", serde_json::to_string(&receipt)?);
    Ok(())
}
