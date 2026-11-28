//! Original R2 qualification CLI backed by the shared native library.
use sml_core::tutor_rust_r2;
#[cfg(test)]
use sml_core::tutor_rust_r2::{examine, fixture, induce};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tutor_rust_r2::run_cli()
}

#[test]
fn r2_shared_learner_compiles_and_rejects_bad_teacher() {
    let raw = fixture();
    let bank = induce(&raw).unwrap();
    assert_eq!(examine(&bank).unwrap().iter().sum::<u64>(), 458_752);
    assert!(induce(&raw.replacen("0|0|0|0", "0|0|0|1", 1)).is_err());
}
