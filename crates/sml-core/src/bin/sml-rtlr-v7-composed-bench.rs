//! RTLR-V7 composed-delay study: does recurrence beat an equally informed,
//! TRAIN-calibrated Boolean control on composed, noisy or context-switched
//! delays? Synthetic ANF features, CPU Rust; NOT trained SML25B/physical MOR.
use sml_core::{
    anf_delay_line::AnfDelayLine,
    calibrated_delay_readout::CalibratedDelayReadout,
    rtlr_boolean_hybrid::RtlrBooleanHybrid,
    rtlr_elastic::{RtlrElastic, RtlrElasticConfig, RtlrV2Policy},
};
use std::{env, error::Error, fs, path::Path};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const BLOCK: usize = 65_536;
const HALO: usize = 1024;
const FIT_DOCS: usize = 3;
const TRAIN_SEED: u64 = 0x91f8_31a2_00c7_6d15;
const DEV_SEED: u64 = 0x1357_9bdf_2468_ace0;
const NOISE_OFFSET: u64 = 0xe1aa_1b56_1f43_09b7;
/// Label noise threshold on a uniform byte: 26/256 = 10.16 % flips.
const NOISE_BELOW: u8 = 26;
const ALPHA: f64 = 0.5;

#[derive(Clone, Copy)]
enum Task {
    Noisy,
    Xor2,
    Xor2Noisy,
    Context,
}

impl Task {
    fn parse(id: u8) -> Result<Self> {
        Ok(match id {
            0 => Self::Noisy,
            1 => Self::Xor2,
            2 => Self::Xor2Noisy,
            3 => Self::Context,
            _ => {
                return Err(
                    "usage: sml-rtlr-v7-composed-bench TASK(0..3) SEED(11|23|37) UNIQUE_DIR".into(),
                )
            }
        })
    }
    /// Delays that together determine the noiseless label.
    fn lags(self) -> &'static [usize] {
        match self {
            Self::Noisy => &[64],
            Self::Xor2 | Self::Xor2Noisy => &[16, 300],
            Self::Context => &[1, 16, 300],
        }
    }
    fn noisy(self) -> bool {
        !matches!(self, Self::Xor2)
    }
}

fn mix(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}
fn data_range(start: usize, end: usize, seed: u64) -> Vec<u8> {
    (start..end)
        .map(|i| {
            mix(seed
                .wrapping_add(i as u64)
                .wrapping_mul(0x9e37_79b9_7f4a_7c15)) as u8
        })
        .collect()
}
fn feature(source: &[u8], i: usize, lag: usize) -> bool {
    ((source[i - lag - 1] & source[i - lag]).count_ones() & 1) != 0
}
fn label(task: Task, source: &[u8], noise: &[u8], i: usize) -> bool {
    let clean = match task {
        Task::Noisy => feature(source, i, 64),
        Task::Xor2 | Task::Xor2Noisy => feature(source, i, 16) ^ feature(source, i, 300),
        Task::Context => {
            if feature(source, i, 1) {
                feature(source, i, 16)
            } else {
                feature(source, i, 300)
            }
        }
    };
    clean ^ (task.noisy() && noise[i] < NOISE_BELOW)
}
fn prob(x: f64) -> f64 {
    1.0 / (1.0 + (-4.0 * x).exp())
}
fn bpc(p: f64, y: bool) -> f64 {
    let p = p.clamp(1e-12, 1.0 - 1e-12);
    -if y { p } else { 1.0 - p }.log2()
}
fn irreducible_bits(task: Task) -> f64 {
    if !task.noisy() {
        return 0.0;
    }
    let p = f64::from(NOISE_BELOW) / 256.0;
    -(p * p.log2() + (1.0 - p) * (1.0 - p).log2())
}
fn prime_ring(ring: &mut AnfDelayLine, source: &[u8]) {
    for i in 2..HALO {
        ring.observe_pair(source[i - 2], source[i - 1]);
    }
}

fn run(task_id: u8, seed: u64, root: &Path) -> Result<()> {
    let task = Task::parse(task_id)?;
    if ![11, 23, 37].contains(&seed) {
        return Err("seed must be 11, 23 or 37".into());
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
    let mut recurrent = RtlrElastic::new_v3(config, policy, &root.join("v3"))?;
    let mut hybrid = RtlrBooleanHybrid::new(
        RtlrElastic::new_v3(config, policy, &root.join("v4-preset"))?,
        HALO,
        vec![1, 16, 128, 1023],
        0.25,
    )?;
    let mut joint = CalibratedDelayReadout::new(task.lags(), ALPHA)?;
    let mut first_only = CalibratedDelayReadout::new(&task.lags()[..1], ALPHA)?;
    let mut prior = CalibratedDelayReadout::new(&[], ALPHA)?;

    for doc in 0..FIT_DOCS {
        let start = HALO + doc * BLOCK;
        let source = data_range(start - HALO, start + BLOCK, TRAIN_SEED);
        let noise = data_range(start - HALO, start + BLOCK, TRAIN_SEED ^ NOISE_OFFSET);
        recurrent.reset_state();
        hybrid.reset_state()?;
        for i in 2..HALO {
            recurrent.advance_inference(source[i - 2], source[i - 1])?;
            hybrid.advance_inference(source[i - 2], source[i - 1])?;
        }
        let mut ring = AnfDelayLine::new(HALO)?;
        prime_ring(&mut ring, &source);
        let (mut v3_loss, mut v3_anchor, mut v4_loss, mut v4_anchor) = (0.0, 0.0, 0.0, 0.0);
        for i in HALO..HALO + BLOCK {
            let y = label(task, &source, &noise, i);
            ring.observe_pair(source[i - 2], source[i - 1]);
            joint.observe_train(&ring, y)?;
            first_only.observe_train(&ring, y)?;
            prior.observe_train(&ring, y)?;

            let p = prob(recurrent.advance_train(source[i - 2], source[i - 1])?);
            v3_loss += bpc(p, y);
            v3_anchor += bpc(prob(recurrent.anchor_state()), y);
            recurrent.learn_credit(4.0 * (p - f64::from(u8::from(y))))?;

            hybrid.advance_train(source[i - 2], source[i - 1])?;
            let (q, a) = hybrid
                .pending_recurrent_and_anchor()
                .ok_or("missing hybrid TRAIN forward")?;
            v4_loss += bpc(q, y);
            v4_anchor += bpc(a, y);
            hybrid.learn_target(y)?;
        }
        recurrent.observe_train_document_v2(v3_loss / BLOCK as f64, v3_anchor / BLOCK as f64)?;
        hybrid.observe_train_document_v2(v4_loss / BLOCK as f64, v4_anchor / BLOCK as f64)?;
    }
    joint.freeze()?;
    first_only.freeze()?;
    prior.freeze()?;

    let source = data_range(0, HALO + BLOCK, DEV_SEED);
    let noise = data_range(0, HALO + BLOCK, DEV_SEED ^ NOISE_OFFSET);
    recurrent.reset_state();
    hybrid.reset_state()?;
    for i in 2..HALO {
        recurrent.advance_inference(source[i - 2], source[i - 1])?;
        hybrid.advance_inference(source[i - 2], source[i - 1])?;
    }
    let mut ring = AnfDelayLine::new(HALO)?;
    prime_ring(&mut ring, &source);
    let (mut c_joint, mut c_first, mut c_prior, mut c_v3, mut c_v4) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for i in HALO..HALO + BLOCK {
        let y = label(task, &source, &noise, i);
        ring.observe_pair(source[i - 2], source[i - 1]);
        c_joint += bpc(joint.probability(&ring)?, y);
        c_first += bpc(first_only.probability(&ring)?, y);
        c_prior += bpc(prior.probability(&ring)?, y);
        c_v3 += bpc(
            prob(recurrent.advance_inference(source[i - 2], source[i - 1])?),
            y,
        );
        c_v4 += bpc(hybrid.advance_inference(source[i - 2], source[i - 1])?, y);
    }
    let n = BLOCK as f64;
    println!(
        "RTLR_V7_COMPOSED_V1\t{task_id}\t{seed}\t{}\t{}\t{BLOCK}\t{:.9}\t{:.9}\t{:.9}\t{:.9}\t{:.9}\t{:.9}\t{}",
        task.lags().len(),
        FIT_DOCS * BLOCK,
        irreducible_bits(task),
        c_joint / n,
        c_first / n,
        c_prior / n,
        c_v3 / n,
        c_v4 / n,
        joint.logical_bytes(),
    );
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 {
        return Err(
            "usage: sml-rtlr-v7-composed-bench TASK(0..3) SEED(11|23|37) UNIQUE_DIR".into(),
        );
    }
    run(args[1].parse()?, args[2].parse()?, Path::new(&args[3]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_causal_and_follow_the_declared_lags() {
        let source = data_range(0, HALO + 8, TRAIN_SEED);
        let noise = vec![255_u8; HALO + 8];
        let i = HALO + 3;
        assert_eq!(
            label(Task::Xor2, &source, &noise, i),
            feature(&source, i, 16) ^ feature(&source, i, 300)
        );
        let expected = if feature(&source, i, 1) {
            feature(&source, i, 16)
        } else {
            feature(&source, i, 300)
        };
        assert_eq!(label(Task::Context, &source, &noise, i), expected);
        // Labels read only strictly earlier bytes: max lag + 1 below i.
        assert!(i > Task::Context.lags().iter().max().unwrap() + 1);
    }

    #[test]
    fn noise_threshold_gives_the_declared_entropy_floor() {
        assert_eq!(irreducible_bits(Task::Xor2), 0.0);
        assert!((irreducible_bits(Task::Noisy) - 0.4733).abs() < 1e-3);
        let source = data_range(0, HALO + 4, TRAIN_SEED);
        let flip = vec![0_u8; HALO + 4];
        let keep = vec![255_u8; HALO + 4];
        let i = HALO + 1;
        assert_ne!(
            label(Task::Noisy, &source, &flip, i),
            label(Task::Noisy, &source, &keep, i)
        );
    }
}
