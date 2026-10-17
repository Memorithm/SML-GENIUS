//! CPU-only, identical-source, cross-architecture RTLR-V2/V3 microbenchmark.
//! No SEC source, no MOR/ANF 50M atlas, no CUDA or GPU execution claims.
use sml_core::rtlr_elastic::{RtlrElastic, RtlrElasticConfig, RtlrV2Policy};
use std::{env, error::Error, fs, path::Path, time::Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const TRAIN: usize = 419_430_400;
const DEV: usize = 104_857_600;
const BLOCK: usize = 65_536;
const MAX_LAG: usize = 1024;

fn mix(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

// Seekable source: any bounded chunk yields the same bytes as the old full
// buffer, without allocating TRAIN+DEV (more than 500 MiB at 100x).
fn stream_range(start: usize, end: usize, seed: u64) -> Vec<u8> {
    (start..end)
        .map(|i| {
            mix(seed
                .wrapping_add(i as u64)
                .wrapping_mul(0x9e37_79b9_7f4a_7c15)) as u8
        })
        .collect()
}

#[cfg(test)]
fn stream(len: usize, seed: u64) -> Vec<u8> {
    stream_range(0, len, seed)
}

fn sample_label(bytes: &[u8], i: usize) -> f64 {
    // Long-lag synthetic parity target: depends on lags 1, 16, 128, 1024.
    let a = bytes[i - 1].count_ones();
    let b = bytes[i - 16].count_ones();
    let c = bytes[i - 128].count_ones();
    let d = bytes[i - 1024].count_ones();
    f64::from((a ^ b ^ c ^ d) & 1)
}

fn prob(feature: f64) -> f64 {
    1.0 / (1.0 + (-4.0 * feature).exp())
}

fn loss(p: f64, y: f64) -> f64 {
    let p = p.clamp(1e-12, 1.0 - 1e-12);
    if y == 1.0 {
        -p.log2()
    } else {
        -(1.0 - p).log2()
    }
}

fn vm_hwm_kib() -> Option<u64> {
    fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find(|s| s.starts_with("VmHWM:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

fn bench(mode: &str, seed: u64, path: &Path) -> Result<()> {
    let width = match mode {
        "scalar" => 1,
        "v2-8" | "v3-8" => 8,
        "v3-16" => 16,
        _ => {
            return Err(
                "usage: sml-rtlr-multiscale-bench MODE scalar|v2-8|v3-8|v3-16 SEED BANK_PATH"
                    .into(),
            )
        }
    };
    let config = RtlrElasticConfig {
        max_active: width,
        learning_rate: 0.005,
        grow_above_bpc: 4.1,
        shrink_below_bpc: 3.8,
        patience: 2,
        warmup_documents: 2,
        seed,
    };
    let policy = RtlrV2Policy {
        ewma_alpha: 0.25,
        plateau_tolerance_bpc: 0.05,
        marginal_cost_bpc_per_cell: 0.001,
    };
    let mut model = match mode {
        "scalar" => RtlrElastic::new(config)?,
        "v2-8" => RtlrElastic::new_v2(config, policy, path)?,
        _ => RtlrElastic::new_v3(config, policy, path)?,
    };
    // Content is generated in bounded 65,536-position chunks. All blocks
    // share the same deterministic absolute-offset stream as the old
    // full-buffer benchmark; the 1,024-byte halo makes labels exact at
    // chunk boundaries. Data generation and allocation are excluded from
    // the timed sections on *both* CPU architectures.
    let training_seed = 0x91f8_31a2_00c7_6d15;
    let heldout_seed = 0x1357_9bdf_2468_ace0;
    let mut train_s = 0.0f64;
    let mut total_train = 0.0f64;
    let mut count_train = 0usize;
    for document in 0..8 {
        model.reset_state();
        let base = MAX_LAG + document * (TRAIN / 8);
        let end = base + TRAIN / 8;
        let mut doc_loss = 0.0;
        let mut shadow_loss = 0.0;
        for chunk_start in (base..end).step_by(BLOCK) {
            let chunk_end = (chunk_start + BLOCK).min(end);
            let prefix = chunk_start - MAX_LAG;
            let bytes = stream_range(prefix, chunk_end, training_seed);
            let start = Instant::now();
            for i in chunk_start..chunk_end {
                let offset = i - prefix;
                let z = model.advance_train(bytes[offset - 2], bytes[offset - 1])?;
                let y = sample_label(&bytes, offset);
                let p = prob(z);
                let anchor = prob(model.anchor_state());
                doc_loss += loss(p, y);
                shadow_loss += loss(anchor, y);
                model.learn_credit(4.0 * (p - y))?;
                count_train += 1;
            }
            train_s += start.elapsed().as_secs_f64();
        }
        total_train += doc_loss;
        if mode != "scalar" {
            model.observe_train_document_v2(
                doc_loss / (TRAIN / 8) as f64,
                shadow_loss / (TRAIN / 8) as f64,
            )?;
        }
    }
    model.reset_state();
    let before = model.stats();
    let mut dev_s = 0.0f64;
    let mut dev_loss = 0.0;
    let end = DEV + MAX_LAG;
    for chunk_start in (MAX_LAG..end).step_by(BLOCK) {
        let chunk_end = (chunk_start + BLOCK).min(end);
        let prefix = chunk_start - MAX_LAG;
        let bytes = stream_range(prefix, chunk_end, heldout_seed);
        let start = Instant::now();
        for i in chunk_start..chunk_end {
            let offset = i - prefix;
            let z = model.advance_inference(bytes[offset - 2], bytes[offset - 1])?;
            dev_loss += loss(prob(z), sample_label(&bytes, offset));
        }
        dev_s += start.elapsed().as_secs_f64();
    }
    if before != model.stats()
        || !total_train.is_finite()
        || !dev_loss.is_finite()
        || train_s <= 0.0
        || dev_s <= 0.0
    {
        return Err("invalid or mutable read-only RTLR benchmark".into());
    }
    let stats = model.stats();
    println!(
        "RTLR_TIMESCALE_CPU_V1\t{mode}\t{seed}\t{TRAIN}\t{DEV}\t{:.9}\t{:.9}\t{:.6}\t{:.6}\t{:.2}\t{:.2}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        total_train / count_train as f64,
        dev_loss / DEV as f64,
        train_s, dev_s,
        TRAIN as f64 / train_s, DEV as f64 / dev_s,
        model.active(), stats.peak_active,
        stats.grown, stats.pruned, stats.bank_parks, stats.bank_restores,
        model.active_payload_bytes(),
        vm_hwm_kib().unwrap_or_default()
    );
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = env::args().collect();
    if args.len() != 4 {
        return Err("usage: sml-rtlr-multiscale-bench MODE SEED UNIQUE_BANK_DIR".into());
    }
    bench(&args[1], args[2].parse()?, Path::new(&args[3]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disjoint_deterministic_streams_and_bounded_label() {
        let a = stream(2048, 14);
        let b = stream(2048, 17);
        assert_ne!(a, b);
        assert_eq!(a, stream(2048, 14));
        for i in 1024..2048 {
            assert!(sample_label(&a, i) == 0.0 || sample_label(&a, i) == 1.0);
        }
    }

    #[test]
    fn chunked_large_stream_preserves_full_source_and_long_lag_targets() {
        let seed = 0x91f8_31a2_00c7_6d15;
        let full = stream(BLOCK * 3 + MAX_LAG + 17, seed);
        for &(base, end) in &[
            (MAX_LAG, MAX_LAG + BLOCK),
            (MAX_LAG + BLOCK, MAX_LAG + 2 * BLOCK),
            (MAX_LAG + 2 * BLOCK, full.len()),
        ] {
            let prefix = base - MAX_LAG;
            let chunk = stream_range(prefix, end, seed);
            assert_eq!(&chunk[..], &full[prefix..end]);
            for i in base..end {
                let offset = i - prefix;
                assert_eq!(sample_label(&full, i), sample_label(&chunk, offset));
            }
        }
    }

    #[test]
    fn logistic_is_finite_and_bounded() {
        for x in [-1.0, 0.0, 1.0] {
            assert!(prob(x).is_finite());
            assert!(loss(prob(x), 0.0).is_finite());
        }
    }
}
