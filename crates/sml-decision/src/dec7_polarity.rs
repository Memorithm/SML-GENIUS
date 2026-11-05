//! DEC-7: negation as its own Boolean feature.
//!
//! DEC-6 lexicon codes separate opposites but cannot read negation: WordNet
//! has no link from "not" to "false" or "wrong". DEC-7 adds one polarity bit
//! per question: on iff the question holds an odd number of negation cues.
//! The cue set is fixed mechanically, before any run:
//!
//! * the closed list `SYNTACTIC` of English negators, and
//! * every lexicon word with an antonym synset of a `TRUTH_SEEDS` word
//!   (words meaning "not true / not right / not correct").
//!
//! The polarity bit enters the DEC-6 features as one more tagged code (alone,
//! with the kind tag, and ANDed with every content bit).

use std::collections::{HashMap, HashSet};

use crate::dec2_text::words;
use crate::dec6_lexicon::lemma_candidates;

/// Closed-class English negators.
pub const SYNTACTIC: [&str; 10] = [
    "no", "not", "never", "none", "nor", "neither", "without", "cannot", "nothing", "nobody",
];

/// Words whose antonyms are negation cues.
pub const TRUTH_SEEDS: [&str; 3] = ["true", "right", "correct"];

/// The negation cue set from the DEC-6 lexicon TSV.
pub fn negators(lexicon_tsv: &str) -> HashSet<String> {
    let rows: HashMap<&str, (&str, &str)> = lexicon_tsv
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            (f.len() == 4).then(|| (f[0], (f[1], f[2])))
        })
        .collect();
    let seeds: HashSet<&str> = TRUTH_SEEDS
        .iter()
        .filter_map(|w| rows.get(w))
        .flat_map(|(syn, _)| syn.split(','))
        .collect();
    let mut out: HashSet<String> = SYNTACTIC.iter().map(|s| s.to_string()).collect();
    for (w, (_, ants)) in &rows {
        if ants.split(',').any(|a| seeds.contains(a)) {
            out.insert((*w).to_string());
        }
    }
    out
}

fn is_cue(cues: &HashSet<String>, w: &str) -> bool {
    cues.contains(w) || lemma_candidates(w).iter().any(|l| cues.contains(l))
}

/// The question's negation cues, in order.
pub fn cues_in(cues: &HashSet<String>, question: &str) -> Vec<String> {
    words(question)
        .into_iter()
        .filter(|w| is_cue(cues, w))
        .collect()
}

/// Polarity bit: on iff the question holds an odd number of cues.
pub fn negated(cues: &HashSet<String>, question: &str) -> bool {
    cues_in(cues, question).len() % 2 == 1
}
