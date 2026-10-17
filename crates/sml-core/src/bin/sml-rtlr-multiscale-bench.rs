//! CPU-only, identical-source, cross-architecture RTLR-V2/V3 microbenchmark.
//! No SEC source, no MOR/ANF 50M atlas, no CUDA or GPU execution claims.
use sml_core::rtlr_elastic::{RtlrElastic, RtlrElasticConfig, RtlrV2Policy};
use std::{env, error::Error, fs, path::Path, time::Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const TRAIN: usize = 419_430_400;
const DEV: usize = 104_857_600;
const BLOCK: usize = 65_536;
const MAX_LAG: usize = 1024;

// Diagnostic modes are opt-in. The four-argument 100x benchmark remains
// byte-for-byte compatible in CLI, task, training sizes and TSV schema.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LabelTask {
    Parity4,
    Lag1,
    Lag16,
    Lag128,
    Lag1024,
    Anf2,
}

impl LabelTask {
    fn parse(name: &str) -> Result<Self> {
        match name {
            "parity4" => Ok(Self::Parity4),
            "lag1" => Ok(Self::Lag1),
            "lag16" => Ok(Self::Lag16),
            "lag128" => Ok(Self::Lag128),
            "lag1024" => Ok(Self::Lag1024),
            "anf2" => Ok(Self::Anf2),
            _ => Err("unknown RTLR diagnostic task: parity4|lag1|lag16|lag128|lag1024|anf2".into()),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Parity4 => "parity4",
            Self::Lag1 => "lag1",
            Self::Lag16 => "lag16",
            Self::Lag128 => "lag128",
            Self::Lag1024 => "lag1024",
            Self::Anf2 => "anf2",
        }
    }

    fn label(self, bytes: &[u8], i: usize) -> f64 {
        match self {
            Self::Parity4 => sample_label(bytes, i),
            Self::Lag1 => f64::from(bytes[i - 1].count_ones() & 1),
            Self::Lag16 => f64::from(bytes[i - 16].count_ones() & 1),
            Self::Lag128 => f64::from(bytes[i - 128].count_ones() & 1),
            Self::Lag1024 => f64::from(bytes[i - 1024].count_ones() & 1),
            Self::Anf2 => f64::from((bytes[i - 2] & bytes[i - 1]).count_ones() & 1),
        }
    }
}

fn validate_diagnostic_counts(train: usize, dev: usize) -> Result<()> {
    // Exactly eight TRAIN documents and a stable block-sized evaluation.
    // This also bounds total evaluation and avoids silent partial documents.
    if train < BLOCK * 8
        || train > TRAIN
        || train % (BLOCK * 8) != 0
        || dev < BLOCK
        || dev > DEV
        || dev % BLOCK != 0
    {
        return Err("invalid diagnostic size: require 8*65536 <= TRAIN <= 419430400, TRAIN multiple of 524288, 65536 <= DEV <= 104857600, DEV multiple of 65536".into());
    }
    Ok(())
}

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

fn bench(
    mode: &str,
    seed: u64,
    path: &Path,
    task: LabelTask,
    train: usize,
    dev: usize,
    diagnostic: bool,
) -> Result<()> {
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
        let base = MAX_LAG + document * (train / 8);
        let end = base + train / 8;
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
                let y = task.label(&bytes, offset);
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
                doc_loss / (train / 8) as f64,
                shadow_loss / (train / 8) as f64,
            )?;
        }
    }
    model.reset_state();
    let before = model.stats();
    let mut dev_s = 0.0f64;
    let mut dev_loss = 0.0;
    let end = dev + MAX_LAG;
    for chunk_start in (MAX_LAG..end).step_by(BLOCK) {
        let chunk_end = (chunk_start + BLOCK).min(end);
        let prefix = chunk_start - MAX_LAG;
        let bytes = stream_range(prefix, chunk_end, heldout_seed);
        let start = Instant::now();
        for i in chunk_start..chunk_end {
            let offset = i - prefix;
            let z = model.advance_inference(bytes[offset - 2], bytes[offset - 1])?;
            dev_loss += loss(prob(z), task.label(&bytes, offset));
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
    let heading = if diagnostic {
        format!("RTLR_TEMPORAL_DIAG_V1\t{}", task.name())
    } else {
        "RTLR_TIMESCALE_CPU_V1".to_owned()
    };
    println!(
        "{heading}\t{mode}\t{seed}\t{train}\t{dev}\t{:.9}\t{:.9}\t{:.6}\t{:.6}\t{:.2}\t{:.2}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        total_train / count_train as f64,
        dev_loss / dev as f64,
        train_s, dev_s,
        train as f64 / train_s, dev as f64 / dev_s,
        model.active(), stats.peak_active,
        stats.grown, stats.pruned, stats.bank_parks, stats.bank_restores,
        model.active_payload_bytes(),
        vm_hwm_kib().unwrap_or_default()
    );
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = env::args().collect();
    let (task, train, dev, diagnostic) = match args.len() {
        4 => (LabelTask::Parity4, TRAIN, DEV, false),
        7 => (
            LabelTask::parse(&args[4])?,
            args[5].parse()?,
            args[6].parse()?,
            true,
        ),
        _ => {
            return Err(
                "usage: sml-rtlr-multiscale-bench MODE SEED UNIQUE_BANK_DIR [TASK TRAIN DEV]"
                    .into(),
            )
        }
    };
    validate_diagnostic_counts(train, dev)?;
    bench(
        &args[1],
        args[2].parse()?,
        Path::new(&args[3]),
        task,
        train,
        dev,
        diagnostic,
    )
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
    fn exact_lag_task_labels_and_chunk_seams() {
        let seed = 0x91f8_31a2_00c7_6d15;
        let full = stream(BLOCK * 3 + MAX_LAG + 17, seed);
        for task in [
            LabelTask::Lag1,
            LabelTask::Lag16,
            LabelTask::Lag128,
            LabelTask::Lag1024,
            LabelTask::Anf2,
            LabelTask::Parity4,
        ] {
            assert_eq!(LabelTask::parse(task.name()).unwrap(), task);
            for chunk_start in [MAX_LAG, MAX_LAG + BLOCK, MAX_LAG + 2 * BLOCK] {
                let end = (chunk_start + BLOCK).min(full.len());
                let prefix = chunk_start - MAX_LAG;
                let block = stream_range(prefix, end, seed);
                for i in chunk_start..end {
                    let local = i - prefix;
                    let label = task.label(&block, local);
                    assert!(label == 0.0 || label == 1.0);
                    assert_eq!(label, task.label(&full, i));
                }
            }
        }
        assert!(LabelTask::parse("unknown").is_err());
    }

    #[test]
    fn diagnostic_sizes_are_bounded_and_document_aligned() {
        assert!(validate_diagnostic_counts(BLOCK * 8, BLOCK).is_ok());
        assert!(validate_diagnostic_counts(TRAIN, DEV).is_ok());
        assert!(validate_diagnostic_counts(BLOCK * 8 - 1, BLOCK).is_err());
        assert!(validate_diagnostic_counts(BLOCK * 8, DEV + BLOCK).is_err());
        assert!(validate_diagnostic_counts(BLOCK * 8 + BLOCK, BLOCK).is_err());
    }

    #[test]
    fn logistic_is_finite_and_bounded() {
        for x in [-1.0, 0.0, 1.0] {
            assert!(prob(x).is_finite());
            assert!(loss(prob(x), 0.0).is_finite());
        }
    }
}
