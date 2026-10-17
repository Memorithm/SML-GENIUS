//! Native Rust-only synthetic qualification of supervised lag mining.
//! Input source seeds are distinct from model/selector labels on the null.
use sml_core::train_lag_discovery::TrainLagDiscovery;
use std::{env, error::Error, time::Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const LOOKBACK: usize = 1024;
const TRAIN: usize = 65_536;

fn mix(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn stream(seed: u64, size: usize) -> Vec<u8> {
    (0..size)
        .map(|i| {
            mix(seed
                .wrapping_add(i as u64)
                .wrapping_mul(0x9e37_79b9_7f4a_7c15)) as u8
        })
        .collect()
}

fn run(panel: u64, target_lag: usize) -> Result<()> {
    if panel > 2 || ![0, 64, 1023].contains(&target_lag) {
        return Err("usage: sml-rtlr-lag-discovery-bench PANEL(0|1|2) LABEL_LAG(0|64|1023)".into());
    }
    let seed = 0x91f8_31a2_00c7_6d15_u64
        ^ mix(panel.wrapping_mul(0x9e37_79b9_7f4a_7c15));
    let data = stream(seed, TRAIN + LOOKBACK);
    let independent = stream(seed ^ 0xe1aa_1b56_1f43_09b7, TRAIN + LOOKBACK);
    let mut discovery = TrainLagDiscovery::new(LOOKBACK, 32, 3, 64, 0.01)?;
    let tick = Instant::now();
    for i in 2..LOOKBACK {
        discovery.prime_pair(data[i - 2], data[i - 1])?;
    }
    for i in LOOKBACK..LOOKBACK + TRAIN {
        discovery.observe_train_pair(data[i - 2], data[i - 1])?;
        let target = if target_lag == 0 {
            independent[i] & 1 != 0
        } else {
            ((data[i - target_lag - 1] & data[i - target_lag]).count_ones() & 1) != 0
        };
        discovery.learn_train_target(target)?;
    }
    let candidates = discovery.freeze()?;
    let seconds = tick.elapsed().as_secs_f64();
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err("invalid native discovery duration".into());
    }
    let first = candidates.first().map_or(0, |candidate| candidate.lag);
    if first != target_lag {
        return Err(format!(
            "TRAIN discovery incorrect for lag={target_lag}: selected={first} candidates={candidates:?}"
        )
        .into());
    }
    println!(
        "RTLR_LAG_DISCOVERY_V1\t{panel}\t{target_lag}\t{first}\t{}\t{}\t{}\t{}\t{seconds:.9}\t{:.2}",
        candidates.len(),
        discovery.training_positions(),
        discovery.bit_backing_bytes(),
        discovery.logical_discovery_bytes(),
        TRAIN as f64 / seconds
    );
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        return Err("usage: sml-rtlr-lag-discovery-bench PANEL LABEL_LAG".into());
    }
    run(args[1].parse()?, args[2].parse()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inputs_are_separate_and_indices_are_bounded() {
        assert!(run(3, 64).is_err());
        assert!(run(0, 65).is_err());
        let a = stream(11, LOOKBACK + 3);
        let b = stream(12, LOOKBACK + 3);
        assert_ne!(a, b);
        for lag in [64_usize, 1023] {
            let i = LOOKBACK;
            assert!(i - lag - 1 < i);
            assert!(i - lag < i);
        }
    }
}
