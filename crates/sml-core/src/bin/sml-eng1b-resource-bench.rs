//! ENG-1b: three separate hosted Linux processes measure reference resource
//! costs on the preregistered synthetic 64-lane, 96-byte/lane workload.
//! This is NOT a model-quality benchmark or a production optimization result.

use std::{
    error::Error,
    hint::black_box,
    time::{Duration, Instant},
};

use sml_core::{
    anf_char::LANES,
    elastic_ngram::{ElasticNGram, ElasticNGramLimits},
    elastic_ngram_long::{ElasticNGramLong, LongNGramLimits},
    elastic_ngram_trie::{ElasticNGramTrie, TrieLimits},
};

const BYTES_PER_LANE: usize = 96;
const WARMUP: usize = 256;
const SAMPLES: usize = 2048;

fn bytes_for_lanes(offset: usize) -> Vec<u8> {
    (0..LANES)
        .flat_map(|lane| {
            (0..BYTES_PER_LANE)
                .map(move |t| (((lane + offset) * 19 + t * 17 + ((t / 7) % 5) * 31) % 251) as u8)
        })
        .collect()
}

fn proc_kib(label: &str) -> Result<u64, Box<dyn Error>> {
    // Linux proc status is a real process resident/peak measurement, but
    // includes runtime + allocator + binary and is not attributed to the model.
    let status = std::fs::read_to_string("/proc/self/status")?;
    let entry = status
        .lines()
        .find(|line| line.starts_with(label))
        .ok_or("missing Linux VmRSS/VmHWM field")?;
    let mut items = entry.split_whitespace();
    if items.next() != Some(label) {
        return Err("unexpected proc status label".into());
    }
    let number = items
        .next()
        .ok_or("missing Linux physical-residency number")?
        .parse::<u64>()?;
    if number == 0 || items.next() != Some("kB") || items.next().is_some() {
        return Err("invalid physical memory sample".into());
    }
    Ok(number)
}

enum Arm {
    Trie(ElasticNGramTrie),
    Long(ElasticNGramLong),
    Legacy(ElasticNGram),
}

impl Arm {
    fn predict(&self, history: &[u8], depth: usize) -> Result<[f64; 256], Box<dyn Error>> {
        Ok(match self {
            Self::Trie(model) => model.distribution(history, depth)?,
            Self::Long(model) => model.distribution(history, depth)?,
            Self::Legacy(model) => model.distribution(history, depth)?,
        })
    }

    fn accounting(&self) -> (usize, usize, usize, usize) {
        match self {
            Self::Trie(model) => {
                let a = model.accounting();
                (
                    a.nodes,
                    a.follower_pairs,
                    a.logical_edge_ref_bytes + a.logical_label_bytes + a.logical_node_depth_bytes,
                    a.logical_counter_bytes,
                )
            }
            Self::Long(model) => {
                let a = model.accounting();
                (
                    a.contexts,
                    a.follower_pairs,
                    a.context_key_bytes + a.follower_key_bytes,
                    a.counter_value_bytes,
                )
            }
            Self::Legacy(model) => {
                let a = model.accounting();
                (
                    a.stored_contexts,
                    a.stored_follower_pairs,
                    a.logical_key_bytes,
                    a.counter_value_bytes,
                )
            }
        }
    }
}

fn run(kind: &str) -> Result<(), Box<dyn Error>> {
    let train = bytes_for_lanes(0);
    let heldout = bytes_for_lanes(137);
    if train.len() != LANES * BYTES_PER_LANE || heldout.len() != train.len() {
        return Err("incorrect frozen ENG-1b fixture".into());
    }
    let start = Instant::now();
    let (depth, arm) = match kind {
        "trie" => (
            32,
            Arm::Trie(ElasticNGramTrie::train(
                &train,
                TrieLimits {
                    max_depth: 32,
                    ..TrieLimits::default()
                },
            )?),
        ),
        "long" => (
            32,
            Arm::Long(ElasticNGramLong::train(
                &train,
                LongNGramLimits {
                    max_depth: 32,
                    ..LongNGramLimits::default()
                },
            )?),
        ),
        "legacy" => (
            7,
            Arm::Legacy(ElasticNGram::train(
                &train,
                ElasticNGramLimits {
                    max_order: 7,
                    ..ElasticNGramLimits::default()
                },
            )?),
        ),
        _ => return Err("usage: sml-eng1b-resource-bench trie|long|legacy".into()),
    };
    let train_us = start.elapsed().as_micros();
    let rss_training_kib = proc_kib("VmRSS:")?;
    let (contexts, follower_pairs, logical_key_bytes, logical_counter_bytes) = arm.accounting();

    let histories = heldout
        .chunks_exact(BYTES_PER_LANE)
        .flat_map(|chunk| (1..chunk.len()).map(|t| &chunk[..t]))
        .collect::<Vec<_>>();
    if histories.len() != LANES * (BYTES_PER_LANE - 1) {
        return Err("invalid frozen query stream".into());
    }

    for i in 0..WARMUP {
        black_box(arm.predict(histories[i % histories.len()], depth)?);
    }
    let mut timings = Vec::with_capacity(SAMPLES);
    let mut digest = 0x517c_c1b7_d529_a4e3u64;
    for i in 0..SAMPLES {
        let history = histories[(i * 13) % histories.len()];
        let at = Instant::now();
        let distribution = arm.predict(history, depth)?;
        let nanos = at.elapsed().as_nanos();
        timings.push(nanos);
        for &probability in &distribution {
            digest = digest.rotate_left(5) ^ probability.to_bits();
        }
        black_box(&distribution);
    }
    if timings.len() != SAMPLES || timings.contains(&0) {
        return Err("incomplete/zero monotonic timing sample".into());
    }
    timings.sort_unstable();
    let p50_ns = timings[SAMPLES / 2];
    let p95_ns = timings[SAMPLES * 95 / 100];
    let hwm_kib = proc_kib("VmHWM:")?;
    let total_wall = Duration::from_micros(u64::try_from(train_us)?);
    println!(
        "ENG1B_MEASURE,arm={},depth={},train_events={},contexts={},followers={},logical_key_bytes={},logical_counter_bytes={},rss_training_kib={},vmhwm_kib={},train_us={},p50_ns={},p95_ns={},samples={},warmup={},digest={:016x},arch={},integrity=PASS",
        kind.to_ascii_uppercase(),
        depth,
        LANES * (BYTES_PER_LANE - 1),
        contexts,
        follower_pairs,
        logical_key_bytes,
        logical_counter_bytes,
        rss_training_kib,
        hwm_kib,
        total_wall.as_micros(),
        p50_ns,
        p95_ns,
        SAMPLES,
        WARMUP,
        digest,
        std::env::consts::ARCH
    );
    Ok(())
}

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 2 {
        eprintln!("usage: sml-eng1b-resource-bench trie|long|legacy");
        std::process::exit(2);
    }
    if let Err(err) = run(&args[1]) {
        eprintln!("ENG1B_FAILURE,{err}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_benchmark_is_64_isolated_lanes() {
        let first = bytes_for_lanes(0);
        let second = bytes_for_lanes(137);
        assert_eq!(first.len(), LANES * BYTES_PER_LANE);
        assert_eq!(second.len(), first.len());
        assert_ne!(first, second);
        assert_eq!(first[0], 0);
        assert_eq!(first[1], 17);
    }
}
