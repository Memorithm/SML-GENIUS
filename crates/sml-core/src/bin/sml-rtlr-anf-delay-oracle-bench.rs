//! Causal, bit-packed and **non-learned** ANF delay-line oracle.
//! Matched source/label feature for the RTLR-V3 delayed-ANF benchmark.
//! No corpus, GPU, physical MOR atlas or learned weight training.
use sml_core::anf_delay_line::AnfDelayLine;
use std::{env, error::Error, time::Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const BLOCK: usize = 65_536;
const MAX_LAG: usize = 1024;
const TRAIN: usize = 524_288;
const DEV: usize = 65_536;
const TRAIN_SEED: u64 = 0x91f8_31a2_00c7_6d15;
const DEV_SEED: u64 = 0x1357_9bdf_2468_ace0;

fn mix(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn source_seed(base: u64, panel: u64) -> u64 {
    if panel == 0 {
        base
    } else {
        base ^ mix(panel.wrapping_mul(0x9e37_79b9_7f4a_7c15))
    }
}

fn stream_range(start: usize, end: usize, seed: u64) -> Vec<u8> {
    (start..end)
        .map(|i| {
            mix(seed
                .wrapping_add(i as u64)
                .wrapping_mul(0x9e37_79b9_7f4a_7c15)) as u8
        })
        .collect()
}

fn feature(older: u8, newer: u8) -> bool {
    ((older & newer).count_ones() & 1) != 0
}

fn label(bytes: &[u8], position: usize, lag: usize) -> bool {
    feature(bytes[position - lag - 1], bytes[position - lag])
}

fn loss(p: f64, positive: bool) -> f64 {
    let p = p.clamp(1e-12, 1.0 - 1e-12);
    if positive {
        -p.log2()
    } else {
        -(1.0 - p).log2()
    }
}

fn bench(panel: u64, lag: usize) -> Result<()> {
    if panel > 2 || ![1_usize, 16, 128, 1023].contains(&lag) {
        return Err("oracle usage: PANEL(0|1|2) LAG(1|16|128|1023)".into());
    }
    let train_seed = source_seed(TRAIN_SEED, panel);
    let dev_seed = source_seed(DEV_SEED, panel);
    let mut positives = [0_u64; 2];
    let mut totals = [0_u64; 2];
    let mut train_pos = 0_u64;

    // Fit two conditional Bernoulli frequencies and one unconditional
    // frequency exclusively from TRAIN; the oracle itself has no fitting.
    for start in (MAX_LAG..MAX_LAG + TRAIN).step_by(BLOCK) {
        let end = start + BLOCK;
        let bytes = stream_range(start - MAX_LAG, end, train_seed);
        for index in start..end {
            let at = index - (start - MAX_LAG);
            let current = usize::from(feature(bytes[at - 2], bytes[at - 1]));
            let target = label(&bytes, at, lag);
            totals[current] += 1;
            positives[current] += u64::from(target);
            train_pos += u64::from(target);
        }
    }

    let prior = (train_pos as f64 + 1.0) / (TRAIN as f64 + 2.0);
    let p_no_memory = [
        (positives[0] as f64 + 1.0) / (totals[0] as f64 + 2.0),
        (positives[1] as f64 + 1.0) / (totals[1] as f64 + 2.0),
    ];
    let mut line = AnfDelayLine::new(MAX_LAG)?;
    let halo = stream_range(0, MAX_LAG, dev_seed);
    // The feature at sample index i consumes only bytes i-2 and i-1.
    // At the first supervised index i=1024, 1022 older features have
    // already been observed in the pre-document halo.
    for t in 2..MAX_LAG {
        line.observe_pair(halo[t - 2], halo[t - 1]);
    }
    let mut mismatch = 0_u64;
    let mut zero_memory_bpc = 0.0f64;
    let mut prior_bpc = 0.0f64;
    let mut dev_positive = 0_u64;
    let mut kernel_seconds = 0.0_f64;
    for start in (MAX_LAG..MAX_LAG + DEV).step_by(BLOCK) {
        let end = start + BLOCK;
        let bytes = stream_range(start - MAX_LAG, end, dev_seed);
        let tic = Instant::now();
        for index in start..end {
            let at = index - (start - MAX_LAG);
            let current = feature(bytes[at - 2], bytes[at - 1]);
            line.observe_pair(bytes[at - 2], bytes[at - 1]);
            let prediction = line.read_lag(lag).ok_or("missing causal history")?;
            let target = label(&bytes, at, lag);
            mismatch += u64::from(prediction != target);
            dev_positive += u64::from(target);
            zero_memory_bpc += loss(p_no_memory[usize::from(current)], target);
            prior_bpc += loss(prior, target);
        }
        kernel_seconds += tic.elapsed().as_secs_f64();
    }
    let rate = DEV as f64 / kernel_seconds;
    if !rate.is_finite()
        || rate <= 0.0
        || !zero_memory_bpc.is_finite()
        || !prior_bpc.is_finite()
    {
        return Err("invalid oracle metrics".into());
    }
    println!(
        "RTLR_ANF_ORACLE_V1\t{panel}\t{lag}\t{TRAIN}\t{DEV}\t{mismatch}\t{:.9}\t{:.9}\t{:.2}\t{}\t{}\t{}",
        zero_memory_bpc / DEV as f64,
        prior_bpc / DEV as f64,
        rate,
        line.storage_bytes(),
        train_pos,
        dev_positive
    );
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = env::args().collect();
    if args.len() != 3 {
        return Err("usage: sml-rtlr-anf-delay-oracle-bench PANEL LAG".into());
    }
    bench(args[1].parse()?, args[2].parse()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_zero_matches_original_source_stream() {
        assert_eq!(source_seed(TRAIN_SEED, 0), TRAIN_SEED);
        assert_eq!(source_seed(DEV_SEED, 0), DEV_SEED);
        let bytes = stream_range(0, 2048, TRAIN_SEED);
        let a = stream_range(0, 1024, TRAIN_SEED);
        let b = stream_range(1024, 2048, TRAIN_SEED);
        assert_eq!(&bytes[..1024], &a);
        assert_eq!(&bytes[1024..], &b);
        assert_ne!(source_seed(TRAIN_SEED, 1), source_seed(TRAIN_SEED, 2));
    }

    #[test]
    fn exact_delayed_anf_features_match_ring_and_labels() {
        let seed = source_seed(DEV_SEED, 2);
        let bytes = stream_range(0, MAX_LAG + 3 * BLOCK, seed);
        let mut ring = AnfDelayLine::new(MAX_LAG).unwrap();
        for t in 2..MAX_LAG {
            ring.observe_pair(bytes[t - 2], bytes[t - 1]);
        }
        for i in MAX_LAG..bytes.len() {
            ring.observe_pair(bytes[i - 2], bytes[i - 1]);
            for lag in [1, 16, 128, 1023] {
                assert_eq!(ring.read_lag(lag), Some(label(&bytes, i, lag)));
            }
        }
        assert_eq!(ring.storage_bytes(), 128);
    }

    #[test]
    fn readout_is_train_only_and_bounded() {
        assert!(bench(3, 16).is_err());
        assert!(bench(0, 0).is_err());
        assert!(bench(0, 1024).is_err());
        for target in [false, true] {
            assert!(loss(0.5, target) == 1.0);
        }
    }
}
