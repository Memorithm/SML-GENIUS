//! ENG-1c/VAL16: requested POST-HOC threshold 16 on previously EXPOSED VAL.
//!
//! Separate from immutable VAL1. No TEST read, no VAL training; results
//! are exploratory and not independent confirmation.

use std::{error::Error, fs, path::Path, time::Instant};

use sml_core::{
    anf_char::{ChunkLayout, LANES},
    elastic_ngram_paged::{
        build_paged, training_sha256, EvidenceDepthReadout, PagedBuild, PagedLimits, PagedNGram,
        ENG1C_VAL_DEPTHS,
    },
    elastic_ngram_paged_sha::hex,
};

const POLICY_NAMES: [&str; 3] = ["FIXED_4", "MIN_COUNT_8", "MIN_COUNT_16"];
const EVIDENCE_MIN_COUNTS: [u64; 2] = [8, 16];
const BOOTSTRAP_REPETITIONS: usize = 2048;
const BOOTSTRAP_SEED: u64 = 0xE1C5_2026_1013_B001;

#[derive(Clone, Copy)]
struct Corpus {
    language: &'static str,
    train_bytes: usize,
    val_bytes: usize,
    val_events: u64,
    train_sha256: &'static str,
    val_sha256: &'static str,
    original_fixed4_bpb: f64,
    original_fixed4_accuracy: f64,
    original_count8_bpb: f64,
    original_count8_selections: [u64; 4],
    contexts: u64,
    follower_pairs: u64,
    counted_bytes: u64,
    index_bytes: u64,
}

fn frozen(language: &str) -> Option<Corpus> {
    match language {
        "FR" => Some(Corpus {
            language: "FR",
            train_bytes: 598_284,
            val_bytes: 74_785,
            val_events: 74_688,
            train_sha256: "ec538157302ad0d39668288c8109779b70492a93041d432f7f97b2dc96e535cd",
            val_sha256: "d3ddafca85d8bfe057503bdb88c450f8110644b51fabbc2600cdb634b2933593",
            original_fixed4_bpb: 2.063036593417,
            original_fixed4_accuracy: 0.565633033419,
            original_count8_bpb: 2.089344543258,
            original_count8_selections: [47_520, 25_301, 1_867, 0],
            contexts: 14_281_533,
            follower_pairs: 14_878_280,
            counted_bytes: 565_374_640,
            index_bytes: 2_440_998,
        }),
        "EN" => Some(Corpus {
            language: "EN",
            train_bytes: 590_360,
            val_bytes: 73_795,
            val_events: 73_728,
            train_sha256: "43553238be6ec4a88e01a679a53381cd89aca8374d815e021e92317f71cb47d0",
            val_sha256: "f1a278d07c7d98a15f976d9ad7a673029a50d038b07d9d2508ffa1a4d41f882b",
            original_fixed4_bpb: 1.876564702039,
            original_fixed4_accuracy: 0.604180230035,
            original_count8_bpb: 1.913076323341,
            original_count8_selections: [43_834, 27_449, 2_422, 23],
            contexts: 13_932_625,
            follower_pairs: 14_519_708,
            counted_bytes: 551_748_904,
            index_bytes: 2_382_156,
        }),
        _ => None,
    }
}

fn vmhwm_kib() -> Result<u64, Box<dyn Error>> {
    let s = fs::read_to_string("/proc/self/status")?;
    let line = s
        .lines()
        .find(|l| l.starts_with("VmHWM:"))
        .ok_or("missing Linux VmHWM")?;
    let mut fields = line.split_whitespace();
    if fields.next() != Some("VmHWM:") {
        return Err("malformed VmHWM".into());
    }
    let amount = fields.next().ok_or("empty Linux VmHWM")?.parse::<u64>()?;
    if amount == 0 || fields.next() != Some("kB") || fields.next().is_some() {
        return Err("invalid Linux process VmHWM".into());
    }
    Ok(amount)
}

/// No target label or VAL count is an input: this gate reads exclusively
/// the TRAIN suffix event total attached to a frozen-depth distribution.
fn evidence_choice(ladder: &[EvidenceDepthReadout; 4], min_train_count: u64) -> usize {
    for candidate in (1..4).rev() {
        if ladder[candidate].context_events >= min_train_count {
            return candidate;
        }
    }
    0
}

#[derive(Clone, Debug)]
struct PolicyStats {
    lane_bits: [f64; LANES],
    tokens: u64,
    correct: u64,
    selected_depths: [u64; 4],
}

impl Default for PolicyStats {
    fn default() -> Self {
        Self {
            lane_bits: [0.0; LANES],
            tokens: 0,
            correct: 0,
            selected_depths: [0; 4],
        }
    }
}

impl PolicyStats {
    fn bits(&self) -> f64 {
        self.lane_bits.iter().sum()
    }

    fn bpb(&self) -> f64 {
        self.bits() / self.tokens as f64
    }

    fn accuracy(&self) -> f64 {
        self.correct as f64 / self.tokens as f64
    }

    fn record(&mut self, lane: usize, label: u8, readout: &EvidenceDepthReadout) {
        let probs = &readout.probabilities;
        let target = usize::from(label);
        self.lane_bits[lane] -= probs[target].log2();
        let best = (0..256).fold(0, |previous, candidate| {
            if probs[candidate] > probs[previous] {
                candidate
            } else {
                previous
            }
        });
        self.correct += u64::from(best == target);
        self.tokens += 1;
        let slot = ENG1C_VAL_DEPTHS
            .iter()
            .position(|&depth| depth == readout.depth)
            .expect("frozen depth");
        self.selected_depths[slot] += 1;
    }
}

fn evaluate_val(
    model: &mut PagedNGram,
    val: &[u8],
    expected_events: u64,
) -> Result<[PolicyStats; 3], Box<dyn Error>> {
    let layout = ChunkLayout::new(val.len())?;
    if layout.tokens() != expected_events {
        return Err("VAL 64-lane denominator drift".into());
    }
    let mut stats: [PolicyStats; 3] = std::array::from_fn(|_| PolicyStats::default());
    for lane in 0..LANES {
        let chunk = &val[lane * layout.chunk..(lane + 1) * layout.chunk];
        for t in 1..chunk.len() {
            // Exactly one traversal of each observed TRAIN count layer.
            let ladder = model.depth_ladder_readout(&chunk[..t])?;
            let choices = [
                0usize,
                evidence_choice(&ladder, EVIDENCE_MIN_COUNTS[0]),
                evidence_choice(&ladder, EVIDENCE_MIN_COUNTS[1]),
            ];
            let true_next_byte = chunk[t];
            for (policy, &depth_index) in choices.iter().enumerate() {
                stats[policy].record(lane, true_next_byte, &ladder[depth_index]);
            }
        }
    }

    for result in &stats {
        if result.tokens != expected_events
            || result.selected_depths.iter().sum::<u64>() != expected_events
            || !result.bpb().is_finite()
            || result.bpb() < 0.0
            || !result.accuracy().is_finite()
        {
            return Err("incomplete/nonfinite VAL prediction score".into());
        }
    }
    Ok(stats)
}

struct XorShift64(u64);
impl XorShift64 {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn paired_lane_interval(
    baseline: &PolicyStats,
    candidate: &PolicyStats,
    lane_tokens: u64,
) -> (f64, f64) {
    let differences: [f64; LANES] = std::array::from_fn(|i| {
        (baseline.lane_bits[i] - candidate.lane_bits[i]) / lane_tokens as f64
    });
    let mut rng = XorShift64(BOOTSTRAP_SEED);
    let mut samples = Vec::with_capacity(BOOTSTRAP_REPETITIONS);
    for _ in 0..BOOTSTRAP_REPETITIONS {
        let mut sample_difference = 0.0;
        for _ in 0..LANES {
            sample_difference += differences[(rng.next() % LANES as u64) as usize];
        }
        samples.push(sample_difference / LANES as f64);
    }
    samples.sort_by(f64::total_cmp);
    (samples[51], samples[1996])
}

fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    if args.len() != 6 {
        return Err("usage: sml-eng1c-val16 FR|EN <TRAIN> <VAL> <NEW-store> <NEW-scratch>".into());
    }
    let corpus = frozen(&args[1]).ok_or("unknown pinned language")?;
    let train = fs::read(&args[2])?;
    let val = fs::read(&args[3])?;
    if train.len() != corpus.train_bytes || val.len() != corpus.val_bytes {
        return Err("frozen 80/10 percent TRAIN/VAL split length drift".into());
    }
    let source = training_sha256(&train);
    if hex(&source) != corpus.train_sha256
        || hex(&training_sha256(&val)) != corpus.val_sha256
    {
        return Err("frozen full TRAIN or EXPOSED VAL SHA256 mismatch".into());
    }
    let layout = ChunkLayout::new(val.len())?;
    if layout.tokens() != corpus.val_events {
        return Err("frozen validation denominator mismatch".into());
    }

    let built_at = Instant::now();
    let build = build_paged(
        &train,
        ENG1C_VAL_DEPTHS[3],
        Path::new(&args[4]),
        Path::new(&args[5]),
        PagedLimits::default(),
    )?;
    let build_ms = built_at.elapsed().as_millis();
    let PagedBuild::Published(metadata) = build else {
        return Err("CAP2b full-depth model unexpectedly hit storage quota".into());
    };
    if metadata.contexts != corpus.contexts
        || metadata.follower_pairs != corpus.follower_pairs
        || metadata.count_store_bytes != corpus.counted_bytes
        || metadata.sparse_index_bytes != corpus.index_bytes
        || metadata.train_bytes != corpus.train_bytes as u64
        || metadata.source_sha256 != source
    {
        return Err("CAP2b exact frozen TRAIN count-store identity drift".into());
    }
    let build_peak_kib = vmhwm_kib()?;
    // Opening validates the counted file, sparse index and source SHA256.
    let mut model = PagedNGram::open(Path::new(&args[4]), source)?;
    let val_at = Instant::now();
    let stats = evaluate_val(&mut model, &val, corpus.val_events)?;
    let val_ms = val_at.elapsed().as_millis();
    let full_process_peak_kib = vmhwm_kib()?;
    let lane_tokens = layout.chunk as u64 - 1;
    // Reference results were fixed in merged VAL1 BEFORE this candidate.
    if (stats[0].bpb() - corpus.original_fixed4_bpb).abs() > 5e-10
        || (stats[0].accuracy() - corpus.original_fixed4_accuracy).abs() > 5e-10
        || (stats[1].bpb() - corpus.original_count8_bpb).abs() > 5e-10
        || stats[1].selected_depths != corpus.original_count8_selections
    {
        return Err("original VAL1 FIXED_4 or MIN_COUNT_8 positive control drift".into());
    }

    let delta_16_minus_8 = stats[2].bpb() - stats[1].bpb();
    let delta_16_minus_4 = stats[2].bpb() - stats[0].bpb();
    let (vs4_lo, vs4_hi) = paired_lane_interval(&stats[0], &stats[2], lane_tokens);
    let (vs8_lo, vs8_hi) = paired_lane_interval(&stats[1], &stats[2], lane_tokens);
    let verdict = if delta_16_minus_4 < 0.0 {
        "EXPLORATORY_BETTER_THAN_FIXED4"
    } else {
        "EXPLORATORY_NOT_BETTER_THAN_FIXED4"
    };
    println!(
        "ENG1C_VAL16_META,language={},train_bytes={},train_sha256={},val_bytes={},val_tokens={},val_lanes=64,depth_model=32,counted_bytes={},sparse_bytes={},contexts={},pairs={},build_ms={},score_ms={},build_process_vmhwm_kib={},score_process_vmhwm_kib={},quality_source=EXPOSED_VAL_POSTHOC,integrity=PASS",
        corpus.language, corpus.train_bytes, hex(&source), val.len(),
        corpus.val_events, metadata.count_store_bytes, metadata.sparse_index_bytes,
        metadata.contexts, metadata.follower_pairs, build_ms, val_ms,
        build_peak_kib, full_process_peak_kib
    );
    for (i, score) in stats.iter().enumerate() {
        let (lo, hi) = paired_lane_interval(&stats[0], score, lane_tokens);
        let improvement = stats[0].bpb() - score.bpb();
        println!(
            "ENG1C_VAL16_POLICY,language={},policy={},tokens={},total_bits={:.12},bpb={:.12},accuracy={:.12},delta_bpb_vs_FIXED_4={:.12},paired_boot_low={:.12},paired_boot_high={:.12},selected_depth4={},selected_depth7={},selected_depth12={},selected_depth32={},study=POST_HOC_EXPLORATORY,integrity=PASS",
            corpus.language, POLICY_NAMES[i], score.tokens, score.bits(), score.bpb(),
            score.accuracy(), improvement, lo, hi,
            score.selected_depths[0], score.selected_depths[1],
            score.selected_depths[2], score.selected_depths[3]
        );
    }
    println!(
        "ENG1C_VAL16_DECISION,language={},fixed4_bpb={:.12},count8_bpb={:.12},count16_bpb={:.12},delta_16_minus_8={:.12},delta_16_minus_fixed4={:.12},paired_vs4_low={:.12},paired_vs4_high={:.12},paired_vs8_low={:.12},paired_vs8_high={:.12},exploratory_verdict={},original_VAL1=UNCHANGED,test=UNREAD,independent_confirmation=NOT_PERFORMED,physical_RAM_savings=NOT_CLAIMED,integrity=PASS",
        corpus.language, stats[0].bpb(), stats[1].bpb(), stats[2].bpb(),
        delta_16_minus_8, delta_16_minus_4,
        vs4_lo, vs4_hi, vs8_lo, vs8_hi, verdict
    );
    Ok(())
}

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if let Err(error) = run(&args) {
        eprintln!("ENG1C_VAL16_FAILURE,{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fabricated(evidence: [u64; 4]) -> [EvidenceDepthReadout; 4] {
        std::array::from_fn(|i| EvidenceDepthReadout {
            depth: ENG1C_VAL_DEPTHS[i],
            probabilities: [1.0 / 256.0; 256],
            context_events: evidence[i],
        })
    }

    #[test]
    fn eng1c_val16_only_train_count_picks_deepest_qualified_suffix() {
        let counts = fabricated([99, 16, 16, 15]);
        assert_eq!(evidence_choice(&counts, 8), 3);
        assert_eq!(evidence_choice(&counts, 16), 2);
        let deep = fabricated([1, 0, 0, 16]);
        assert_eq!(evidence_choice(&deep, 16), 3);
        let missing = fabricated([99, 0, 0, 0]);
        assert_eq!(evidence_choice(&missing, 16), 0);
        assert_eq!(evidence_choice(&fabricated([100, 15, 15, 15]), 16), 0);
    }

    #[test]
    fn eng1c_val16_deterministic_paired_lanes_and_original_controls() {
        let mut control = PolicyStats::default();
        let mut trial = PolicyStats::default();
        control.tokens = (LANES * 39) as u64;
        trial.tokens = control.tokens;
        control.lane_bits = std::array::from_fn(|i| i as f64);
        trial.lane_bits = std::array::from_fn(|i| i as f64 + 1.0);
        assert_eq!(
            paired_lane_interval(&control, &trial, 39),
            paired_lane_interval(&control, &trial, 39)
        );
        assert_eq!(paired_lane_interval(&control, &control, 39), (0.0, 0.0));
        assert_eq!(POLICY_NAMES, ["FIXED_4", "MIN_COUNT_8", "MIN_COUNT_16"]);
        assert_eq!(frozen("FR").unwrap().val_events, 74_688);
        assert_eq!(frozen("EN").unwrap().val_events, 73_728);
        assert_eq!(frozen("FR").unwrap().original_count8_selections, [47520, 25301, 1867, 0]);
    }
}
