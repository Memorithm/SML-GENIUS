//! ENG-1d/BMX-ANF: does the SML recurrent ANF byte model add information to
//! the Boolean context mixer?
//!
//! For each (PRIME, INPUT) pair: trains the frozen `anf_side` configuration
//! on PRIME, primes both predictors on PRIME, then reports the ideal code
//! length of INPUT for `bmx-v3` alone and with the ANF side input in each
//! [`SideInputs`] mode. Use tuning or DEV texts; this is an ablation tool,
//! not a confirmation.
//!
//! Usage: sml-bmx-anf-eval PRIME INPUT [PRIME INPUT ...]

use std::{error::Error, fs, time::Instant};

use sml_core::{
    anf_side::AnfCharSide,
    bit_mixer::{Architecture, BitMixer, MaskSet, MixerConfig, SideInputs},
    bool_coder::BitPredictor,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn code_bytes(mixer: &mut BitMixer, input: &[u8]) -> f64 {
    let mut bits = 0.0f64;
    for &byte in input {
        for shift in (0..8).rev() {
            let bit = (byte >> shift) & 1 == 1;
            let p = f64::from(mixer.p1()) / 4_294_967_296.0;
            bits -= if bit { p } else { 1.0 - p }.log2();
            mixer.update(bit);
        }
    }
    bits / 8.0
}

/// The documented ablation was measured against bmx-v3; pinned so later
/// default architectures do not change what this tool reproduces.
fn baseline() -> MixerConfig {
    MixerConfig {
        architecture: Architecture::Classic,
        mask_set: MaskSet::V3,
        ..MixerConfig::default()
    }
}

fn run(args: &[String]) -> Result<()> {
    if args.len() < 3 || (args.len() - 1) % 2 != 0 {
        return Err("usage: sml-bmx-anf-eval PRIME INPUT [PRIME INPUT ...]".into());
    }
    let config = baseline();
    for pair in args[1..].chunks(2) {
        let prime = fs::read(&pair[0])?;
        let input = fs::read(&pair[1])?;
        let started = Instant::now();
        let side = AnfCharSide::train_on(&prime)?;
        let train_seconds = started.elapsed().as_secs_f64();
        let mut base = BitMixer::new(config);
        base.prime(&prime);
        let reference = code_bytes(&mut base, &input);
        print!("ANF_EVAL\t{}\tbmx-v3={reference:.1}B", pair[1]);
        for mode in [
            SideInputs::Both,
            SideInputs::AddressOnly,
            SideInputs::ProbabilityOnly,
        ] {
            let mut mixer =
                BitMixer::try_with_side(config, MaskSet::V3.masks(), Box::new(side.clone()), mode)?;
            mixer.prime(&prime);
            let bytes = code_bytes(&mut mixer, &input);
            print!(
                "\t{mode:?}={bytes:.1}B({:+.2}%)",
                100.0 * (bytes - reference) / reference
            );
        }
        println!("\tanf_train_seconds={train_seconds:.1}");
    }
    Ok(())
}

fn main() {
    if let Err(e) = run(&std::env::args().collect::<Vec<_>>()) {
        eprintln!("ANF_EVAL_STOP: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ablation_baseline_is_the_released_bmx_v3() {
        assert_eq!(baseline(), MixerConfig::released()[2]);
        assert!(baseline().label().starts_with("bmx-v3:"));
    }
}
