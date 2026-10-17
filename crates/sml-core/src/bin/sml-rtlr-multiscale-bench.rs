//! CPU-only, identical-source, cross-architecture RTLR-V2/V3 microbenchmark.
//! No SEC source, no MOR/ANF 50M atlas, no CUDA or GPU execution claims.
use sml_core::rtlr_elastic::{RtlrElastic, RtlrElasticConfig, RtlrV2Policy};
use std::{env, error::Error, fs, path::Path, time::Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const TRAIN: usize = 32_768;
const DEV: usize = 8_192;

fn mix(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn stream(len: usize, seed: u64) -> Vec<u8> {
    (0..len).map(|i| mix(seed.wrapping_add(i as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)) as u8).collect()
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
    if y == 1.0 { -p.log2() } else { -(1.0 - p).log2() }
}

fn vm_hwm_kib() -> Option<u64> {
    fs::read_to_string("/proc/self/status").ok()?.lines()
        .find(|s| s.starts_with("VmHWM:"))?
        .split_whitespace().nth(1)?.parse().ok()
}

fn bench(mode: &str, seed: u64, path: &Path) -> Result<()> {
    let width = match mode {
        "scalar" => 1,
        "v2-8" | "v3-8" => 8,
        "v3-16" => 16,
        _ => return Err("usage: sml-rtlr-multiscale-bench MODE scalar|v2-8|v3-8|v3-16 SEED BANK_PATH".into()),
    };
    let config = RtlrElasticConfig {
        max_active: width, learning_rate: 0.005,
        grow_above_bpc: 4.1, shrink_below_bpc: 3.8,
        patience: 2, warmup_documents: 2, seed,
    };
    let policy = RtlrV2Policy {
        ewma_alpha: 0.25, plateau_tolerance_bpc: 0.05,
        marginal_cost_bpc_per_cell: 0.001,
    };
    let mut model = match mode {
        "scalar" => RtlrElastic::new(config)?,
        "v2-8" => RtlrElastic::new_v2(config, policy, path)?,
        _ => RtlrElastic::new_v3(config, policy, path)?,
    };
    // Seed-independent TRAIN and DEV content: distinct generated streams.
    // The tested seed only affects the recurrent initialization / bank.
    let training = stream(TRAIN + 1024, 0x91f8_31a2_00c7_6d15);
    let heldout = stream(DEV + 1024, 0x1357_9bdf_2468_ace0);
    let start = Instant::now();
    let mut total_train = 0.0f64;
    let mut count_train = 0usize;
    for document in 0..8 {
        model.reset_state();
        let base = 1024 + document * (TRAIN / 8);
        let mut doc_loss = 0.0;
        let mut shadow_loss = 0.0;
        for i in base..base + TRAIN / 8 {
            let z = model.advance_train(training[i - 2], training[i - 1])?;
            let y = sample_label(&training, i);
            let p = prob(z);
            let anchor = prob(model.anchor_state());
            doc_loss += loss(p, y);
            shadow_loss += loss(anchor, y);
            model.learn_credit(4.0 * (p - y))?;
            count_train += 1;
        }
        total_train += doc_loss;
        if mode != "scalar" {
            model.observe_train_document_v2(
                doc_loss / (TRAIN / 8) as f64,
                shadow_loss / (TRAIN / 8) as f64,
            )?;
        }
    }
    let train_s = start.elapsed().as_secs_f64();
    model.reset_state();
    let before = model.stats();
    let eval_start = Instant::now();
    let mut dev_loss = 0.0;
    for i in 1024..DEV + 1024 {
        let z = model.advance_inference(heldout[i - 2], heldout[i - 1])?;
        dev_loss += loss(prob(z), sample_label(&heldout, i));
    }
    let dev_s = eval_start.elapsed().as_secs_f64();
    if before != model.stats() || !total_train.is_finite() || !dev_loss.is_finite()
        || train_s <= 0.0 || dev_s <= 0.0
    {
        return Err("invalid or mutable read-only RTLR benchmark".into());
    }
    let stats = model.stats();
    println!(
        "RTLR_TIMESCALE_CPU_V1\t{mode}\t{seed}\t{TRAIN}\t{DEV}\t{:.9}\t{:.9}\t{:.6}\t{:.6}\t{:.2}\t{:.2}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
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
    fn logistic_is_finite_and_bounded() {
        for x in [-1.0, 0.0, 1.0] {
            assert!(prob(x).is_finite());
            assert!(loss(prob(x), 0.0).is_finite());
        }
    }
}
