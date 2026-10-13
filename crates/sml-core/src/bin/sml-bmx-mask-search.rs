//! ENG-1d/BMX: greedy search of Boolean byte-mask contexts for `bit_mixer`.
//!
//! Each candidate mask selects a Boolean sub-cube of the last 8 bytes'
//! bits. Starting from the masks already selected, every remaining candidate
//! is added in turn and scored by the total adaptive code length (sum of
//! -log2 p over all bits) on the tuning sets; the best one is kept if it
//! saves at least MIN_GAIN bits, for at most ROUNDS rounds.
//!
//! Tuning sets only: never pass an evaluation (TEST) text here. The result
//! is printed as a Rust literal to freeze into `bit_mixer::V3_MASKS`.
//!
//! Usage: sml-bmx-mask-search TABLE_BITS ROUNDS MIN_GAIN PRIME|- INPUT [PRIME|- INPUT ...]

use std::{error::Error, fs, sync::Arc, thread};

use sml_core::{
    bit_mixer::{masks_label, BitMixer, ByteMask, MaskSet, MixerConfig},
    bool_coder::BitPredictor,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Structured candidate pool (offsets: index 0 is the previous byte).
fn candidates() -> Vec<ByteMask> {
    let mut out = Vec::new();
    let seq = |m: u8, len: usize| {
        let mut mask = [0u8; 8];
        mask[..len].fill(m);
        mask
    };
    // Skip contexts.
    for skip in [
        [0, 0xFF, 0, 0],
        [0, 0, 0xFF, 0],
        [0, 0xFF, 0xFF, 0],
        [0xFF, 0, 0xFF, 0],
        [0, 0xFF, 0, 0xFF],
        [0, 0, 0xFF, 0xFF],
    ] {
        let mut mask = [0u8; 8];
        mask[..4].copy_from_slice(&skip);
        out.push(mask);
    }
    // Case-folded, character-class and coarse sequences.
    for len in 1..=5 {
        out.push(seq(0xDF, len));
    }
    for len in [2, 3, 4, 6, 8] {
        out.push(seq(0xE0, len));
    }
    for len in [4, 8] {
        out.push(seq(0xC0, len));
    }
    for len in [2, 3, 4] {
        out.push(seq(0xF0, len));
    }
    // Exact previous byte with coarse older bytes.
    for (older, len) in [(0xE0, 3), (0xE0, 5), (0xDF, 2), (0xC0, 7)] {
        let mut mask = seq(older, len + 1);
        mask[0] = 0xFF;
        out.push(mask);
    }
    out
}

fn code_bits(config: MixerConfig, masks: &[ByteMask], sets: &[(Vec<u8>, Vec<u8>)]) -> f64 {
    let mut bits = 0.0;
    for (prime, input) in sets {
        let mut mixer = BitMixer::try_with_masks(config, masks).expect("valid masks");
        mixer.prime(prime);
        for &byte in input {
            for shift in (0..8).rev() {
                let bit = (byte >> shift) & 1 == 1;
                let p = f64::from(mixer.p1()) / 4_294_967_296.0;
                bits -= if bit { p } else { 1.0 - p }.log2();
                mixer.update(bit);
            }
        }
    }
    bits
}

fn run(args: &[String]) -> Result<()> {
    if args.len() < 6 || (args.len() - 4) % 2 != 0 {
        return Err("usage: sml-bmx-mask-search TABLE_BITS ROUNDS MIN_GAIN PRIME|- INPUT [PRIME|- INPUT ...]".into());
    }
    let config = MixerConfig {
        table_bits: args[1].parse()?,
        mask_set: MaskSet::None,
        ..MixerConfig::default()
    };
    let rounds: usize = args[2].parse()?;
    let min_gain: f64 = args[3].parse()?;
    let mut sets = Vec::new();
    for pair in args[4..].chunks(2) {
        let prime = if pair[0] == "-" {
            Vec::new()
        } else {
            fs::read(&pair[0])?
        };
        sets.push((prime, fs::read(&pair[1])?));
    }
    let sets = Arc::new(sets);
    let pool = candidates();
    let mut selected: Vec<ByteMask> = Vec::new();
    let mut best = code_bits(config, &selected, &sets);
    println!("SEARCH\tcandidates={}\tbaseline_bits={best:.1}", pool.len());
    let workers = thread::available_parallelism().map_or(1, |n| n.get());
    for round in 0..rounds {
        let remaining: Vec<ByteMask> = pool
            .iter()
            .copied()
            .filter(|m| !selected.contains(m))
            .collect();
        let mut scored: Vec<(f64, ByteMask)> = Vec::new();
        for chunk in remaining.chunks(workers) {
            let handles: Vec<_> = chunk
                .iter()
                .map(|&mask| {
                    let sets = Arc::clone(&sets);
                    let mut trial = selected.clone();
                    trial.push(mask);
                    thread::spawn(move || (code_bits(config, &trial, &sets), mask))
                })
                .collect();
            for handle in handles {
                scored.push(handle.join().map_err(|_| "worker panicked")?);
            }
        }
        // Deterministic order: lowest bits, then lexicographic mask.
        scored.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        for (bits, mask) in scored.iter().take(5) {
            println!(
                "ROUND{round}\t{}\tgain_bits={:.1}",
                masks_label(&[*mask]),
                best - bits
            );
        }
        let (bits, mask) = scored[0];
        if best - bits < min_gain {
            println!("STOP\tbest_gain={:.1} < {min_gain}", best - bits);
            break;
        }
        best = bits;
        selected.push(mask);
        println!("KEEP\t{}\ttotal_bits={best:.1}", masks_label(&selected));
    }
    println!("pub const V3_MASKS: &[ByteMask] = &[");
    for mask in &selected {
        let bytes: Vec<String> = mask.iter().map(|b| format!("0x{b:02X}")).collect();
        println!("    [{}],", bytes.join(", "));
    }
    println!("];");
    Ok(())
}

fn main() {
    if let Err(e) = run(&std::env::args().collect::<Vec<_>>()) {
        eprintln!("MASK_SEARCH_STOP: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_pool_is_unique_nonzero_and_within_limits() {
        let pool = candidates();
        assert!(pool.len() >= 20);
        for (i, mask) in pool.iter().enumerate() {
            assert!(mask.iter().any(|&b| b != 0));
            assert!(!pool[..i].contains(mask), "duplicate {mask:?}");
        }
    }

    #[test]
    fn a_helpful_mask_is_found_on_a_synthetic_skip_pattern() {
        // Byte t depends only on byte t-2: the skip mask [0, FF, ...] is
        // the most useful candidate and must reduce the code length.
        let mut data = vec![0u8, 0];
        let mut x = 7u32;
        while data.len() < 30_000 {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let noise = (x >> 16) as u8;
            let prev2 = data[data.len() - 2];
            data.push(if noise % 4 == 0 {
                noise
            } else {
                prev2.wrapping_add(1)
            });
        }
        let config = MixerConfig {
            table_bits: 16,
            match_bits: 12,
            mask_set: MaskSet::None,
            ..MixerConfig::default()
        };
        let sets = vec![(Vec::new(), data)];
        let base = code_bits(config, &[], &sets);
        let skip = code_bits(config, &[[0, 0xFF, 0, 0, 0, 0, 0, 0]], &sets);
        assert!(skip < base, "{skip} vs {base}");
    }
}
