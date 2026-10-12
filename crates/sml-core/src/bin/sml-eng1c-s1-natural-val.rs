//! ENG-1c/S1: preregistered capacity-bounded natural-text VAL-only pilot.
//!
//! Exactly 112 bytes at the center of each 64 original TRAIN lanes are
//! selected without consulting model loss. Both frozen SML-owned Witten-Bell
//! implementations train on the same subset and score only the original
//! full VAL split. TEST is never passed to a predictor.
//!
//! The language utility conclusion is exploratory and remains STOP unless
//! BOTH languages pass the externally checked predeclared inequality.

use std::{error::Error, fs};

use sml_core::{
    anf_char::{ChunkLayout, LANES},
    char_baselines::{ByteScore, NGramModel},
    elastic_ngram_long::{ElasticNGramLong, LongNGramLimits},
    elastic_ngram_trie::{ElasticNGramTrie, TrieLimits},
};

const WINDOW_BYTES: usize = 112;
const TRAIN_EVENTS: usize = LANES * (WINDOW_BYTES - 1);
const DEPTHS: [usize; 7] = [0, 4, 7, 12, 16, 24, 32];
const MAX_DEPTH: usize = 32;
const MIN_DEV_GAIN_BPB: f64 = 0.02;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Language {
    French,
    English,
}

impl Language {
    fn parse(value: &str) -> Result<Self, Box<dyn Error>> {
        match value {
            "FR" => Ok(Self::French),
            "EN" => Ok(Self::English),
            _ => Err("language must be FR or EN".into()),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::French => "FR",
            Self::English => "EN",
        }
    }

    fn expected_body(self) -> usize {
        match self {
            Self::French => 747_855,
            Self::English => 737_950,
        }
    }

    fn expected_val_events(self) -> u64 {
        match self {
            Self::French => 74_688,
            Self::English => 73_728,
        }
    }
}

/// The sample is fixed by length and original 64-lane positions, not labels.
/// For full TRAIN lane size m >=112: take the exact 112 central bytes.
fn select_train_windows(train: &[u8]) -> Result<(Vec<u8>, usize), Box<dyn Error>> {
    let layout = ChunkLayout::new(train.len())?;
    if layout.chunk < WINDOW_BYTES {
        return Err("TRAIN lane too short for frozen 112-byte extraction".into());
    }
    let start = (layout.chunk - WINDOW_BYTES) / 2;
    let mut out = Vec::with_capacity(LANES * WINDOW_BYTES);
    for lane in 0..LANES {
        let base = lane * layout.chunk + start;
        out.extend_from_slice(&train[base..base + WINDOW_BYTES]);
    }
    if out.len() != LANES * WINDOW_BYTES {
        return Err("incomplete frozen 64-lane TRAIN selection".into());
    }
    Ok((out, start))
}

fn compare_score(
    depth: usize,
    original: ByteScore,
    trie: ByteScore,
    long: ByteScore,
    expected: u64,
) -> Result<(), Box<dyn Error>> {
    for score in [original, trie, long] {
        if score.tokens != expected
            || !score.bits_per_byte.is_finite()
            || !score.accuracy.is_finite()
        {
            return Err(format!("incomplete/nonfinite VAL for depth {depth}").into());
        }
    }
    if original.bits_per_byte.to_bits() != trie.bits_per_byte.to_bits()
        || original.accuracy.to_bits() != trie.accuracy.to_bits()
        || long.bits_per_byte.to_bits() != trie.bits_per_byte.to_bits()
        || long.accuracy.to_bits() != trie.accuracy.to_bits()
    {
        return Err(format!("independent full-score parity failure depth {depth}").into());
    }
    Ok(())
}

fn check_full_probabilities(
    val: &[u8],
    trie: &ElasticNGramTrie,
    long: &ElasticNGramLong,
    references: &[(usize, NGramModel)],
) -> Result<usize, Box<dyn Error>> {
    let layout = ChunkLayout::new(val.len())?;
    let mut histories = 0usize;
    for lane in 0..LANES {
        let chunk = &val[lane * layout.chunk..(lane + 1) * layout.chunk];
        if chunk.len() <= 25 {
            return Err("VAL lane too short for preregistered probability grid".into());
        }
        // Exactly the first 24 and last 24 positive history lengths.
        for t in (1..=24).chain((chunk.len() - 24)..chunk.len()) {
            let history = &chunk[..t];
            for &depth in &DEPTHS {
                let a = trie.distribution(history, depth)?;
                let b = long.distribution(history, depth)?;
                let mut total = 0.0f64;
                for y in 0..256 {
                    if a[y].to_bits() != b[y].to_bits()
                        || !a[y].is_finite()
                        || a[y] <= 0.0
                    {
                        return Err(format!(
                            "256-class trie/long parity failure at depth {depth}, lane {lane}, t {t}, class {y}"
                        ).into());
                    }
                    total += a[y];
                }
                if (total - 1.0).abs() > 1.0e-10 {
                    return Err("unnormalized VAL 256-way distribution".into());
                }
                if let Some((_, ref_model)) = references.iter().find(|(k, _)| *k == depth) {
                    let mut reference = [0.0f64; 256];
                    ref_model.distribution(history, &mut reference);
                    for y in 0..256 {
                        if reference[y].to_bits() != a[y].to_bits() {
                            return Err(format!("legacy NGramModel parity failure at depth {depth}").into());
                        }
                    }
                }
            }
            histories += 1;
        }
    }
    Ok(histories)
}

fn run(language: Language, body: &[u8]) -> Result<(), Box<dyn Error>> {
    if body.len() != language.expected_body() {
        return Err("body byte count mismatches frozen corpus".into());
    }
    let a = body.len() * 8 / 10;
    let b = body.len() * 9 / 10;
    // Critical: do NOT refer to body[b..], do NOT inspect TEST labels or
    // compute any TEST score. Only TRAIN and VAL are passed to the model.
    let full_train = &body[..a];
    let val = &body[a..b];
    let (training, lane_start) = select_train_windows(full_train)?;

    let trie = ElasticNGramTrie::train(
        &training,
        TrieLimits {
            max_depth: MAX_DEPTH,
            ..TrieLimits::default()
        },
    )?;
    let long = ElasticNGramLong::train(
        &training,
        LongNGramLimits {
            max_depth: MAX_DEPTH,
            ..LongNGramLimits::default()
        },
    )?;
    if trie.accounting().training_events != TRAIN_EVENTS
        || long.accounting().training_events != TRAIN_EVENTS
        || trie.accounting().nodes != long.accounting().contexts
        || trie.accounting().follower_pairs != long.accounting().follower_pairs
        || trie.contexts_by_depth() != long.contexts_by_depth()
    {
        return Err("unmatched TRAIN count tables or incorrect budget population".into());
    }

    println!(
        "ENG1C_S1_TRAIN,lang={},body_bytes={},full_train_bytes={},train_subset_bytes={},train_events={},full_train_lane_bytes={},window_start={},trained_max_depth=32,trie_contexts={},long_contexts={},follower_pairs={},trie_logical_counter_bytes={},long_logical_counter_bytes={},no_TEST_evaluation=true,integrity=PASS",
        language.name(),
        body.len(),
        full_train.len(),
        training.len(),
        TRAIN_EVENTS,
        full_train.len() / LANES,
        lane_start,
        trie.accounting().nodes,
        long.accounting().contexts,
        trie.accounting().follower_pairs,
        trie.accounting().logical_counter_bytes,
        long.accounting().counter_value_bytes
    );

    let references = [0, 4, 7]
        .into_iter()
        .map(|depth| (depth, NGramModel::train(&training, depth)))
        .collect::<Vec<_>>();
    let probability_histories = check_full_probabilities(val, &trie, &long, &references)?;
    if probability_histories != LANES * 48 {
        return Err("incomplete preregistered VAL history oracle grid".into());
    }
    println!(
        "ENG1C_S1_ORACLE,lang={},histories={},depths={},classes=256,bitwise_model_parity=PASS,no_TEST_evaluation=true",
        language.name(),
        probability_histories,
        DEPTHS.len()
    );

    let mut losses = [0.0f64; DEPTHS.len()];
    let mut best_index = 0usize;
    let mut best_bits = f64::INFINITY;
    for (index, &depth) in DEPTHS.iter().enumerate() {
        let one = trie.score(val, depth)?;
        let two = long.score(val, depth)?;
        if one.tokens != language.expected_val_events()
            || two.tokens != language.expected_val_events()
            || one.bits_per_byte.to_bits() != two.bits_per_byte.to_bits()
            || one.accuracy.to_bits() != two.accuracy.to_bits()
            || !one.bits_per_byte.is_finite()
        {
            return Err(format!("unmatched 256-byte VAL score depth {depth}").into());
        }
        if let Some((_, model)) = references.iter().find(|(k, _)| *k == depth) {
            let baseline = model.score(val);
            compare_score(depth, baseline, one, two, language.expected_val_events())?;
        }
        losses[index] = one.bits_per_byte;
        println!(
            "ENG1C_S1_VAL,lang={},depth={},events={},bits_per_byte={:.8},accuracy={:.8},parity=PASS,split=VAL",
            language.name(),
            depth,
            one.tokens,
            one.bits_per_byte,
            one.accuracy
        );
        if one.bits_per_byte + 1.0e-12 < best_bits {
            best_bits = one.bits_per_byte;
            best_index = index;
        }
    }

    // Fixed decision: k32 must beat BOTH k4 and k7 by at least 0.02 bpb.
    // This is development-only. A negative result must NOT fail correctness CI.
    let pass = losses[6] <= losses[1].min(losses[2]) - MIN_DEV_GAIN_BPB;
    let verdict = if pass { "LONG_DEPTH_DEV_GAIN" } else { "DEPTH_UTIL_STOP" };
    println!(
        "ENG1C_S1_RESULT,lang={},VAL_only=true,train_events={},VAL_events={},k4_bpb={:.8},k7_bpb={:.8},k32_bpb={:.8},best_val_depth={},best_val_bpb={:.8},decision={},TEST_scored=false,quality_promotion=NOT_AUTHORIZED",
        language.name(),
        TRAIN_EVENTS,
        language.expected_val_events(),
        losses[1],
        losses[2],
        losses[6],
        DEPTHS[best_index],
        best_bits,
        verdict,
    );
    Ok(())
}

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 3 {
        eprintln!("usage: sml-eng1c-s1-natural-val FR|EN <SHA256-verified-normalized-Gutenberg-body>");
        std::process::exit(2);
    }
    let result = Language::parse(&args[1])
        .and_then(|language| Ok((language, fs::read(&args[2])?)))
        .and_then(|(language, body)| run(language, &body));
    if let Err(error) = result {
        eprintln!("ENG1C_S1_FAILURE,{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_middle_window_has_64_complete_lanes_no_cross_boundary() {
        let synthetic = (0..LANES)
            .flat_map(|lane| {
                (0..1000usize).map(move |t| (lane * 7 + t) as u8)
            })
            .collect::<Vec<_>>();
        let (subset, start) = select_train_windows(&synthetic).unwrap();
        assert_eq!(start, 444);
        assert_eq!(subset.len(), 7168);
        assert_eq!(LANES * (WINDOW_BYTES - 1), 7104);
        assert_eq!(1 + TRAIN_EVENTS * 32, 227_329);
        assert_eq!(TRAIN_EVENTS * 33, 234_432);
        assert_eq!(TRAIN_EVENTS * 528, 3_750_912);
        for lane in 0..LANES {
            let source_start = lane * 1000 + start;
            assert_eq!(subset[lane * WINDOW_BYTES], synthetic[source_start]);
            assert_eq!(
                subset[lane * WINDOW_BYTES + WINDOW_BYTES - 1],
                synthetic[source_start + WINDOW_BYTES - 1]
            );
        }
    }

    #[test]
    fn language_and_frozen_denominators_are_fixed() {
        assert_eq!(Language::French.expected_body(), 747_855);
        assert_eq!(Language::English.expected_body(), 737_950);
        assert_eq!(Language::French.expected_val_events(), 74_688);
        assert_eq!(Language::English.expected_val_events(), 73_728);
        assert!(Language::parse("TEST").is_err());
    }
}
