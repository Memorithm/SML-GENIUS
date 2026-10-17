//! Native, standalone RTRL-3 kernel benchmark. Not a SEC/25B experiment.
//! Every row uses the same deterministic synthetic train and disjoint DEV
//! streams; three independent estimator seeds expose stochastic variation.
use sml_core::{
    elastic_ngram_paged_sha::{hex, Sha256},
    rtrl3_elastic::{ElasticRtrl3, Rtrl3Config},
};
use std::{env, error::Error, fs, time::Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const TRAIN_BYTES: usize = 16_386;
const DEV_BYTES: usize = 8_194;
const REPEATS: [u64; 3] = [11, 23, 37];
const ARMS: [(usize, usize, usize); 5] = [(1, 0, 1), (8, 2, 2), (16, 2, 4), (32, 4, 8), (64, 4, 8)];

fn next_u64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *state = x;
    x.wrapping_mul(0x2545_f491_4f6c_dd1d)
}

/// A deliberately simple causal synthetic problem:
/// 7-step history and previous-byte parity determine one of two byte patterns,
/// with rare seeded noise. TRAIN and DEV have disjoint RNG streams.
fn generate(len: usize, seed: u64) -> Vec<u8> {
    let mut rng = seed;
    let mut data = Vec::with_capacity(len);
    for i in 0..len {
        let noise = next_u64(&mut rng);
        if i < 8 {
            data.push((noise >> 32) as u8);
        } else {
            let previous = data[i - 1];
            let lagged = data[i - 7];
            let parity = ((previous ^ lagged).count_ones() & 1) != 0;
            let pattern = if parity { 0xaa } else { 0x55 };
            data.push(if noise & 31 == 0 {
                (noise >> 40) as u8
            } else {
                pattern
            });
        }
    }
    data
}

fn checksum(data: &[u8]) -> String {
    let mut hash = Sha256::default();
    hash.update(data);
    hex(&hash.finish())
}

fn run() -> Result<()> {
    let args: Vec<_> = env::args().collect();
    if args.len() != 2 || args[1] != "--synthetic-v1" {
        return Err("usage: sml-rtrl3-bench --synthetic-v1".into());
    }
    let train = generate(TRAIN_BYTES, 0xf52d_4a80_1ff0_3123);
    let dev = generate(DEV_BYTES, 0xd72c_83f5_0abc_4129);
    let train_sha = checksum(&train);
    let dev_sha = checksum(&dev);
    if train_sha == dev_sha || train.len() != TRAIN_BYTES || dev.len() != DEV_BYTES {
        return Err("invalid synthetic protocol".into());
    }
    println!("RTRL3_SYNTHETIC_V1 train_sha256={train_sha} dev_sha256={dev_sha} train_bytes={TRAIN_BYTES} dev_bytes={DEV_BYTES} data=generated test2024=UNOPENED");
    println!("version\thidden\tmixing_rank\teligibility_rank\tseed\ttrain_events\tdev_events\ttrain_bpc\tdev_bpc\ttrain_s\tdev_s\ttrain_events_per_s\tdev_events_per_s\tmodel_payload_bytes\tlearned_recurrence_parameters\tclipped_steps\tmax_gradient_norm");
    for &(hidden, mixing_rank, eligibility_rank) in &ARMS {
        for &seed in &REPEATS {
            let mut model = ElasticRtrl3::new(Rtrl3Config {
                hidden,
                mixing_rank,
                eligibility_rank,
                learning_rate: 0.005,
                recurrent_clip_norm: 1.0,
                seed,
            })?;
            let start = Instant::now();
            let measured = model.train_document(&train)?;
            let training = start.elapsed().as_secs_f64();
            let start = Instant::now();
            let (evaluation_loss, dev_events) = model.evaluate_document(&dev)?;
            let evaluation = start.elapsed().as_secs_f64();
            if measured.trained_bytes != (TRAIN_BYTES - 2) as u64
                || dev_events != (DEV_BYTES - 2) as u64
                || !training.is_finite()
                || !evaluation.is_finite()
                || training <= 0.0
                || evaluation <= 0.0
                || !measured.logloss_bits.is_finite()
                || !evaluation_loss.is_finite()
            {
                return Err("failed finite or sample-accounting RTRL-3 benchmark gate".into());
            }
            let train_bpc = measured.logloss_bits / measured.trained_bytes as f64;
            let dev_bpc = evaluation_loss / dev_events as f64;
            println!(
                "RTRL3_SYNTHETIC_V1\t{hidden}\t{mixing_rank}\t{eligibility_rank}\t{seed}\t{}\t{dev_events}\t{train_bpc:.9}\t{dev_bpc:.9}\t{training:.6}\t{evaluation:.6}\t{:.2}\t{:.2}\t{}\t{}\t{}\t{:.9}",
                measured.trained_bytes,
                measured.trained_bytes as f64 / training,
                dev_events as f64 / evaluation,
                model.resident_payload_bytes(),
                model.recurrent_trainable_parameters(),
                measured.clipped_steps,
                measured.maximum_gradient_norm
            );
        }
    }
    let memory = fs::read_to_string("/proc/self/status")?;
    let peak = memory
        .lines()
        .find(|line| line.starts_with("VmHWM:"))
        .ok_or("missing Linux VmHWM peak memory")?;
    println!("RTRL3_PROCESS_PEAK_MEMORY {peak}");
    println!("RTRL3_SYNTHETIC_5_ARM_DONE seeds=3 arms=5 train=16384 dev=8192 test2024=UNOPENED");
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("RTRL3_BENCH_FAIL: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_train_dev_no_identity_and_fixed_lag() {
        let a = generate(512, 0x1234);
        let b = generate(512, 0x1234);
        let other = generate(512, 0x5678);
        assert_eq!(a, b);
        assert_ne!(a, other);
        assert_eq!(a.len(), 512);
        assert_ne!(checksum(&a), checksum(&other));
        assert_eq!(ARMS.len(), 5);
        assert_eq!(REPEATS.len(), 3);
    }

    #[test]
    fn small_benchmark_is_finite() {
        let bytes = generate(130, 0x1234);
        let mut model = ElasticRtrl3::new(Rtrl3Config {
            hidden: 8,
            eligibility_rank: 2,
            mixing_rank: 2,
            learning_rate: 0.005,
            recurrent_clip_norm: 1.0,
            seed: 11,
        })
        .unwrap();
        let result = model.train_document(&bytes).unwrap();
        assert_eq!(result.trained_bytes, 128);
        let (loss, events) = model.evaluate_document(&bytes).unwrap();
        assert!(loss.is_finite());
        assert_eq!(events, 128);
    }
}
