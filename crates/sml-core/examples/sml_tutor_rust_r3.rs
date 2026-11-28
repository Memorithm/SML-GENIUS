//! Original R3 qualification CLI backed by the shared native library.
use sml_core::tutor_rust_r3;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tutor_rust_r3::run_cli()
}

#[test]
fn r3_shared_learner_compiles_and_retains_r2() {
    use sml_core::tutor_rust_r2;
    let parent = tutor_rust_r2::induce(&tutor_rust_r2::fixture()).unwrap();
    let bank = tutor_rust_r3::induce(&parent, &tutor_rust_r3::fixture()).unwrap();
    assert_eq!(
        tutor_rust_r3::examine(&parent, &bank).unwrap().iter().sum::<u64>(),
        458_752
    );
    assert!(tutor_rust_r3::examine("invalid parent", &bank).is_err());
}
