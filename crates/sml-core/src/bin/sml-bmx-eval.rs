//! ENG-1d/BMX: ideal code length of every released mixer configuration.
//!
//! For each (PRIME, INPUT) pair, prints the ideal code length (sum of
//! -log2 p, in bytes) of INPUT for `bmx-v5`, `bmx-v4`, `bmx-v3` and `bmx-v2`, coded
//! alone and after priming on PRIME (`-` for no prime). Real streams are
//! within a few bytes of these values (see `sml-bool-codec encode-mix`).
//!
//! Usage: sml-bmx-eval PRIME|- INPUT [PRIME|- INPUT ...]

use std::{error::Error, fs, time::Instant};

use sml_core::{
    bit_mixer::{BitMixer, MixerConfig},
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

fn run(args: &[String]) -> Result<()> {
    if args.len() < 3 || (args.len() - 1) % 2 != 0 {
        return Err("usage: sml-bmx-eval PRIME|- INPUT [PRIME|- INPUT ...]".into());
    }
    for pair in args[1..].chunks(2) {
        let prime = if pair[0] == "-" {
            Vec::new()
        } else {
            fs::read(&pair[0])?
        };
        let input = fs::read(&pair[1])?;
        for config in MixerConfig::released() {
            let label = config.label();
            let version = label.split(':').next().unwrap_or("?");
            let started = Instant::now();
            let alone = code_bytes(&mut BitMixer::try_new(config)?, &input);
            let mut mixer = BitMixer::try_new(config)?;
            mixer.prime(&prime);
            let primed = code_bytes(&mut mixer, &input);
            let speed =
                (2 * input.len() + prime.len()) as f64 / 1024.0 / started.elapsed().as_secs_f64();
            println!(
                "BMX_EVAL\t{}\t{version}\talone={alone:.1}B\tprimed={primed:.1}B\tprime_bytes={}\tKiB_per_s={speed:.0}",
                pair[1],
                prime.len()
            );
        }
    }
    Ok(())
}

fn main() {
    if let Err(e) = run(&std::env::args().collect::<Vec<_>>()) {
        eprintln!("BMX_EVAL_STOP: {e}");
        std::process::exit(1);
    }
}
