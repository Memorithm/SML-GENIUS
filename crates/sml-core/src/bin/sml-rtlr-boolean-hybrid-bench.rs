//! Paired native CPU experiment: frozen known-feature RTLR-V3 vs the
//! TRAIN-learned RTLR Boolean delayed-feature selector (including unseen lag64).
//! Synthetic only; no SEC or physical MOR/ANF atlas.
use sml_core::{
    rtlr_boolean_hybrid::RtlrBooleanHybrid,
    rtlr_elastic::{RtlrElastic, RtlrElasticConfig, RtlrV2Policy},
};
use std::{env, error::Error, fs, path::Path, time::Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const TRAIN: usize = 524_288;
const DEV: usize = 65_536;
const BLOCK: usize = 65_536;
const MAX_LAG: usize = 1024;
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

fn label(bytes: &[u8], i: usize, lag: usize) -> bool {
    ((bytes[i - lag - 1] & bytes[i - lag]).count_ones() & 1) != 0
}

fn prob(feature: f64) -> f64 {
    1.0 / (1.0 + (-4.0 * feature).exp())
}

fn bpc(probability: f64, positive: bool) -> f64 {
    let p = probability.clamp(1e-12, 1.0 - 1e-12);
    if positive {
        -p.log2()
    } else {
        -(1.0 - p).log2()
    }
}

fn prime(base: &mut RtlrElastic, hybrid: &mut RtlrBooleanHybrid, seed: u64, start: usize) -> Result<()> {
    let bytes = stream_range(start - MAX_LAG, start, seed);
    for i in 2..MAX_LAG {
        base.advance_inference(bytes[i - 2], bytes[i - 1])?;
        hybrid.advance_inference(bytes[i - 2], bytes[i - 1])?;
    }
    Ok(())
}

fn bench(panel: u64, lag: usize, seed: u64, root: &Path) -> Result<()> {
    if panel > 2 || ![1, 16, 64, 128, 1023].contains(&lag) || ![11, 23, 37].contains(&seed) {
        return Err("usage: binary PANEL(0..2) LAG(1|16|64|128|1023) SEED(11|23|37) UNIQUE_ROOT".into());
    }
    fs::create_dir(root)?;
    let config = RtlrElasticConfig {
        max_active: 8,
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
    let mut base = RtlrElastic::new_v3(config, policy, &root.join("base-v3"))?;
    let mut hybrid = RtlrBooleanHybrid::new(
        RtlrElastic::new_v3(config, policy, &root.join("hybrid-v3"))?,
        MAX_LAG,
        vec![1, 16, 128, 1023],
        0.25,
    )?;
    let train_seed = source_seed(TRAIN_SEED, panel);
    let dev_seed = source_seed(DEV_SEED, panel);
    let mut base_train_seconds = 0.0;
    let mut hybrid_train_seconds = 0.0;
    for document in 0..8 {
        let start = MAX_LAG + document * BLOCK;
        let end = start + BLOCK;
        base.reset_state();
        hybrid.reset_state()?;
        prime(&mut base, &mut hybrid, train_seed, start)?;
        let bytes = stream_range(start - MAX_LAG, end, train_seed);
        let mut base_loss = 0.0;
        let mut base_anchor_loss = 0.0;
        let timer = Instant::now();
        for i in start..end {
            let at = i - (start - MAX_LAG);
            let y = label(&bytes, at, lag);
            let p = prob(base.advance_train(bytes[at - 2], bytes[at - 1])?);
            let anchor = prob(base.anchor_state());
            base_loss += bpc(p, y);
            base_anchor_loss += bpc(anchor, y);
            base.learn_credit(4.0 * (p - f64::from(u8::from(y))))?;
        }
        base_train_seconds += timer.elapsed().as_secs_f64();
        let mut hybrid_recurrent_loss = 0.0;
        let mut hybrid_anchor_loss = 0.0;
        let timer = Instant::now();
        for i in start..end {
            let at = i - (start - MAX_LAG);
            let y = label(&bytes, at, lag);
            hybrid.advance_train(bytes[at - 2], bytes[at - 1])?;
            let (recurrent, anchor) = hybrid.pending_recurrent_and_anchor().ok_or("missing TRAIN recurrent measurement")?;
            hybrid_recurrent_loss += bpc(recurrent, y);
            hybrid_anchor_loss += bpc(anchor, y);
            hybrid.learn_target(y)?;
        }
        hybrid_train_seconds += timer.elapsed().as_secs_f64();
        base.observe_train_document_v2(base_loss / BLOCK as f64, base_anchor_loss / BLOCK as f64)?;
        hybrid.observe_train_document_v2(
            hybrid_recurrent_loss / BLOCK as f64,
            hybrid_anchor_loss / BLOCK as f64,
        )?;
    }

    base.reset_state();
    hybrid.reset_state()?;
    prime(&mut base, &mut hybrid, dev_seed, MAX_LAG)?;
    let trained_weights = hybrid.expert_weights();
    let mut base_loss = 0.0;
    let mut hybrid_loss = 0.0;
    let mut base_dev_seconds = 0.0;
    let mut hybrid_dev_seconds = 0.0;
    for start in (MAX_LAG..MAX_LAG + DEV).step_by(BLOCK) {
        let end = start + BLOCK;
        let bytes = stream_range(start - MAX_LAG, end, dev_seed);
        let timer = Instant::now();
        for i in start..end {
            let at = i - (start - MAX_LAG);
            let y = label(&bytes, at, lag);
            let p = prob(base.advance_inference(bytes[at - 2], bytes[at - 1])?);
            base_loss += bpc(p, y);
        }
        base_dev_seconds += timer.elapsed().as_secs_f64();
        let timer = Instant::now();
        for i in start..end {
            let at = i - (start - MAX_LAG);
            let y = label(&bytes, at, lag);
            let p = hybrid.advance_inference(bytes[at - 2], bytes[at - 1])?;
            hybrid_loss += bpc(p, y);
        }
        hybrid_dev_seconds += timer.elapsed().as_secs_f64();
    }
    if hybrid.expert_weights() != trained_weights || hybrid.train_updates() != TRAIN as u64 {
        return Err("read-only DEV changed learned selector or TRAIN step count".into());
    }
    let metrics = [
        base_loss / DEV as f64,
        hybrid_loss / DEV as f64,
        TRAIN as f64 / base_train_seconds,
        TRAIN as f64 / hybrid_train_seconds,
        DEV as f64 / base_dev_seconds,
        DEV as f64 / hybrid_dev_seconds,
    ];
    if metrics.iter().any(|x| !x.is_finite() || *x <= 0.0) {
        return Err("invalid hybrid paired CPU metrics".into());
    }
    println!(
        "RTLR_BOOLEAN_HYBRID_V1\t{panel}\t{lag}\t{seed}\t{TRAIN}\t{DEV}\t{:.9}\t{:.9}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{}\t{}\t{}\t{}\t{:.9}\t{:.9}\t{:.9}\t{:.9}\t{:.9}",
        metrics[0], metrics[1], metrics[2], metrics[3], metrics[4], metrics[5],
        base.active(), hybrid.active_cells(),
        hybrid.boolean_bytes(), hybrid.logical_payload_bytes(),
        trained_weights[0], trained_weights[1], trained_weights[2], trained_weights[3], trained_weights[4]
    );
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = env::args().collect();
    if args.len() != 5 {
        return Err("usage: sml-rtlr-boolean-hybrid-bench PANEL LAG SEED UNIQUE_ROOT".into());
    }
    bench(args[1].parse()?, args[2].parse()?, args[3].parse()?, Path::new(&args[4]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matched_data_panels_and_known_unknown_delays() {
        assert_eq!(source_seed(TRAIN_SEED, 0), TRAIN_SEED);
        assert_eq!(source_seed(DEV_SEED, 0), DEV_SEED);
        assert_ne!(source_seed(TRAIN_SEED, 1), source_seed(TRAIN_SEED, 2));
        let bytes = stream_range(0, MAX_LAG + 1, TRAIN_SEED);
        for lag in [1, 16, 64, 128, 1023] {
            let at = MAX_LAG;
            assert_eq!(
                label(&bytes, at, lag),
                ((bytes[at - lag - 1] & bytes[at - lag]).count_ones() & 1) != 0
            );
        }
        assert!(bpc(0.5, true) == 1.0);
    }
}
