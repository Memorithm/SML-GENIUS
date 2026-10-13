use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 5 {
        return Err(
            "usage: sml-publication DOSSIER RUN OWNER_BINARY INDEPENDENT_POLICY ALLOWED_SIGNERS"
                .into(),
        );
    }
    let admitted = sml_publication::admit_external_weights(
        Path::new(&args[0]),
        &args[1],
        Path::new(&args[2]),
        Path::new(&args[3]),
        Path::new(&args[4]),
    )?;
    println!("{}", serde_json::to_string(admitted.receipt())?);
    Ok(())
}
