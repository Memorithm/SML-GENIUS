//! ENG-1c/VAL1: frozen independent real-text validation log-loss.
//!
//! Fixed depths 4/7/12/32 and model-owned exact TRAIN occurrence policies
//! 2/4/8 share one immutable depth32 Witten-Bell store. This program never
//! opens or scores TEST, does not fit on VAL, and does not actuate ElasticXxx.

use std::{error::Error, fs, path::Path, time::Instant};

use sml_core::{
    anf_char::{ChunkLayout, LANES},
    elastic_ngram_paged::{
        build_paged, training_sha256, EvidenceDepthReadout, PagedBuild, PagedLimits, PagedNGram,
        ENG1C_VAL_DEPTHS,
    },
    elastic_ngram_paged_sha::hex,
};

const POLICY_NAMES: [&str; 7] = [
    "FIXED_4",
    "FIXED_7",
    "FIXED_12",
    "FIXED_32",
    "MIN_COUNT_2",
    "MIN_COUNT_4",
    "MIN_COUNT_8",
];
const EVIDENCE_MIN_COUNTS: [u64; 3] = [2, 4, 8];
const BOOTSTRAP_REPETITIONS: usize = 2048;
const BOOTSTRAP_SEED: u64 = 0xE1C5_2026_1013_B001;

#[derive(Clone, Copy)]
struct Corpus {
    language: &'static str,
    train_bytes: usize,
    val_bytes: usize,
    val_events: u64,
    train_sha256: &'static str,
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
) -> Result<[PolicyStats; 7], Box<dyn Error>> {
    let layout = ChunkLayout::new(val.len())?;
    if layout.tokens() != expected_events {
        return Err("VAL 64-lane denominator drift".into());
    }
    let mut stats: [PolicyStats; 7] = std::array::from_fn(|_| PolicyStats::default());
    for lane in 0..LANES {
        let chunk = &val[lane * layout.chunk..(lane + 1) * layout.chunk];
        for t in 1..chunk.len() {
            // Exactly one traversal of each observed TRAIN count layer.
            let ladder = model.depth_ladder_readout(&chunk[..t])?;
            let choices = [
                0usize,
                1,
                2,
                3,
                evidence_choice(&ladder, EVIDENCE_MIN_COUNTS[0]),
                evidence_choice(&ladder, EVIDENCE_MIN_COUNTS[1]),
                evidence_choice(&ladder, EVIDENCE_MIN_COUNTS[2]),
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

fn best_index(stats: &[PolicyStats], indices: std::ops::Range<usize>) -> usize {
    let mut best = indices.start;
    for i in indices.skip(1) {
        // Strict < preserves earlier frozen policy in exact-tie cases.
        if stats[i].bpb() < stats[best].bpb() {
            best = i;
        }
    }
    best
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
        return Err("usage: sml-eng1c-val1 FR|EN <TRAIN> <VAL> <NEW-store> <NEW-scratch>".into());
    }
    let corpus = frozen(&args[1]).ok_or("unknown pinned language")?;
    let train = fs::read(&args[2])?;
    let val = fs::read(&args[3])?;
    if train.len() != corpus.train_bytes || val.len() != corpus.val_bytes {
        return Err("frozen 80/10 percent TRAIN/VAL split length drift".into());
    }
    let source = training_sha256(&train);
    if hex(&source) != corpus.train_sha256 {
        return Err("frozen full TRAIN source SHA256 mismatch".into());
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
    let fixed = best_index(&stats, 0..4);
    let gated = best_index(&stats, 4..7);
    let selected = best_index(&stats, 0..7);
    let adaptive_gain = stats[fixed].bpb() - stats[gated].bpb();
    let (gate_lo, gate_hi) = paired_lane_interval(&stats[fixed], &stats[gated], lane_tokens);
    let verdict = if adaptive_gain >= 0.01 && gate_lo > 0.0 {
        "PROVISIONAL_VAL_GAIN"
    } else {
        "VAL_STOP_NOT_PROVEN"
    };

    println!(
        "ENG1C_VAL1_META,language={},train_bytes={},train_sha256={},val_bytes={},val_tokens={},val_lanes=64,depth_model=32,counted_bytes={},sparse_bytes={},contexts={},pairs={},build_ms={},score_ms={},build_process_vmhwm_kib={},score_process_vmhwm_kib={},quality_source=VAL_ONLY,integrity=PASS",
        corpus.language, corpus.train_bytes, hex(&source), val.len(),
        corpus.val_events, metadata.count_store_bytes, metadata.sparse_index_bytes,
        metadata.contexts, metadata.follower_pairs, build_ms, val_ms,
        build_peak_kib, full_process_peak_kib
    );
    for (i, score) in stats.iter().enumerate() {
        let (lo, hi) = paired_lane_interval(&stats[0], score, lane_tokens);
        let improvement = stats[0].bpb() - score.bpb();
        println!(
            "ENG1C_VAL1_POLICY,language={},policy={},tokens={},total_bits={:.12},bpb={:.12},accuracy={:.12},delta_bpb_vs_FIXED_4={:.12},paired_boot_low={:.12},paired_boot_high={:.12},selected_depth4={},selected_depth7={},selected_depth12={},selected_depth32={},integrity=PASS",
            corpus.language, POLICY_NAMES[i], score.tokens, score.bits(), score.bpb(),
            score.accuracy(), improvement, lo, hi,
            score.selected_depths[0], score.selected_depths[1],
            score.selected_depths[2], score.selected_depths[3]
        );
    }
    println!(
        "ENG1C_VAL1_DECISION,language={},best_fixed={},best_gated={},best_overall={},best_fixed_bpb={:.12},best_gated_bpb={:.12},gated_gain_bpb={:.12},paired_gate_low={:.12},paired_gate_high={:.12},adaptive_science={},test=UNREAD,physical_RAM_savings=NOT_CLAIMED,integrity=PASS",
        corpus.language, POLICY_NAMES[fixed], POLICY_NAMES[gated], POLICY_NAMES[selected],
        stats[fixed].bpb(), stats[gated].bpb(), adaptive_gain, gate_lo, gate_hi, verdict
    );
    Ok(())
}

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if let Err(error) = run(&args) {
        eprintln!("ENG1C_VAL1_FAILURE,{error}");
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
    fn eng1c_val1_evidence_policy_uses_deepest_qualified_train_suffix_only() {
        let candidates = fabricated([99, 2, 4, 1]);
        assert_eq!(evidence_choice(&candidates, 2), 2);
        assert_eq!(evidence_choice(&candidates, 4), 2);
        assert_eq!(evidence_choice(&candidates, 8), 0);
        let deep = fabricated([1, 0, 0, 8]);
        assert_eq!(evidence_choice(&deep, 8), 3);
        let missing = fabricated([99, 0, 0, 0]);
        assert_eq!(evidence_choice(&missing, 2), 0);
    }

    #[test]
    fn eng1c_val1_ties_and_lane_bootstrap_are_stable() {
        let mut stats: [PolicyStats; 7] = std::array::from_fn(|_| PolicyStats::default());
        for (i, row) in stats.iter_mut().enumerate() {
            row.tokens = 64 * 39;
            row.lane_bits = std::array::from_fn(|lane| (lane + i) as f64);
        }
        assert_eq!(best_index(&stats, 0..4), 0);
        assert_eq!(best_index(&stats, 4..7), 4);
        assert_eq!(
            paired_lane_interval(&stats[0], &stats[1], 39),
            paired_lane_interval(&stats[0], &stats[1], 39)
        );
        assert_eq!(paired_lane_interval(&stats[0], &stats[0], 39), (0.0, 0.0));
        assert_eq!(frozen("FR").unwrap().val_events, 74_688);
        assert_eq!(frozen("EN").unwrap().val_events, 73_728);
    }
}
