//! DEC-14: anchor-based question routing.
//!
//! DEC-13 showed that a router of hashed words and pairs memorises its
//! template wordings: position words absent from the templates ("final",
//! "back", "conclusion") did not reach their intent. DEC-14 adds lexical
//! anchors: each intent has a few seed words, fixed here, and a question word
//! hits an anchor when, in Open English WordNet, it shares a synset with a seed
//! or has a seed synset as a direct hypernym. Only the main clause is read for
//! anchors: in "X rather than / instead of / as opposed to Y", only X counts
//! (scope rule, fixed here).
//!
//! The `reserved` set (10 wordings per intent) was written after the seeds and
//! before any DEC-14 code or run, and is used only on TEST. The OEWN expansion
//! happens on Thor; its output was not seen when the set was written.

use crate::dec12_router::{router_features, ROUTER_DIM};
use crate::dec13_scope;
use crate::dec2_text::words;
use crate::dec4_instr::Intent;
use crate::dec5_meaning::{QuestionCoder, WordCodes};
use crate::dec7_polarity::negated;
use std::collections::{HashMap, HashSet};

/// Anchor order: Opening, Middle, End, Continues (near), NotContinues (away).
pub const ANCHORS: usize = 5;
/// Router dimension with the anchor block appended after the DEC-12 space.
pub const ROUTER14_DIM: usize = ROUTER_DIM + 32;

/// Seed words of each anchor, in `ANCHORS` order.
pub const SEEDS: [&[&str]; ANCHORS] = [
    &["beginning", "start", "first", "opening"],
    &["middle", "center", "centre", "midst"],
    &["end", "last", "finish", "ending"],
    &["follow", "next", "continue", "after"],
    &["elsewhere", "other", "separate", "gap"],
];

/// Contrast links: text from the first link on is out of scope.
pub const LINKS: [&str; 3] = [" rather than ", " instead of ", " as opposed to "];

/// Reserved wordings, used only in TEST `reserved`.
pub fn reserved(intent: Intent) -> &'static [&'static str] {
    match intent {
        Intent::Continues => &[
            "Does B come straight after A in the original text?",
            "Is B the passage that immediately succeeds A?",
            "Does the article carry on from A into B?",
            "Is B the next paragraph once A is done?",
            "If you kept reading after A, would you reach B at once?",
            "Is B a direct continuation of A?",
            "Does B resume the text right where A leaves off?",
            "Are A and B consecutive, with A coming first?",
            "Does B succeed A directly rather than appear later on?",
            "Is B the following passage instead of one from another place?",
        ],
        Intent::NotContinues => &[
            "Is B detached from what comes right after A?",
            "Does B originate from a separate portion of the paper?",
            "Is B drawn from some other location than just after A?",
            "Are A and B far apart in the paper?",
            "Is B unconnected to the text that directly follows A?",
            "Would B be out of sequence if read right after A?",
            "Is B not what comes immediately after A?",
            "Do other passages separate A from B?",
            "Is B from a different place rather than the direct continuation of A?",
            "Does B appear elsewhere instead of right after A?",
        ],
        Intent::Opening => &[
            "Is this from the beginning portion of the article?",
            "Does this text appear first in the paper?",
            "Is this one of the starting paragraphs?",
            "Is this the initial part of the document?",
            "Does the article start with this passage?",
            "Is this from the paper's first section?",
            "Would a reader see this before anything else?",
            "Is this from the introductory part of the text?",
            "Is this text at the head of the paper?",
            "Is this an opening paragraph rather than a closing one?",
        ],
        Intent::Middle => &[
            "Is this text from the central section of the article?",
            "Does this passage sit in the interior of the paper?",
            "Is this neither near the start nor near the finish?",
            "Is this from a mid-paper paragraph?",
            "Does this appear somewhere in between the opening and the ending?",
            "Is this from the main body rather than the edges of the document?",
            "Is this text from the heart of the article?",
            "Would a reader reach this partway through the paper?",
            "Is this an intermediate passage of the document?",
            "Is this located at the centre of the text?",
        ],
        Intent::End => &[
            "Is this from the ending of the article?",
            "Does this text appear last in the paper?",
            "Is this one of the concluding paragraphs?",
            "Is this the final part of the document?",
            "Does the article finish on this passage?",
            "Is this from the paper's last section?",
            "Would a reader see this after everything else?",
            "Is this from the closing part of the text?",
            "Is this text at the tail of the paper?",
            "Is this a closing paragraph rather than an opening one?",
        ],
    }
}

/// DEC-14 router training wordings: DEC-13 training plus the spent DEC-13
/// reserved set, deduplicated, minus the DEC-14 reserved set.
pub fn training_wordings(intent: Intent) -> Vec<String> {
    let mut out = dec13_scope::training_wordings(intent);
    out.extend(dec13_scope::reserved(intent).iter().map(|s| s.to_string()));
    let banned = reserved(intent);
    let mut kept: Vec<String> = Vec::new();
    for w in out {
        if !banned.contains(&w.as_str()) && !kept.contains(&w) {
            kept.push(w);
        }
    }
    kept
}

/// Main clause of a question under the scope rule.
pub fn main_clause(question: &str) -> &str {
    let cut = LINKS
        .iter()
        .filter_map(|l| question.find(l))
        .min()
        .unwrap_or(question.len());
    &question[..cut]
}

/// Word → anchor membership expanded from OEWN.
pub struct Anchors {
    hits: HashMap<String, [bool; ANCHORS]>,
}

const SUFFIXES: [&str; 6] = ["ing", "es", "ed", "ly", "s", "d"];

impl Anchors {
    /// Builds anchor membership from a DEC-6 lexicon TSV
    /// (`word synsets antonyms hypernyms`).
    pub fn from_lexicon(lexicon_tsv: &str) -> Anchors {
        fn split(s: &str) -> Vec<&str> {
            if s == "-" {
                Vec::new()
            } else {
                s.split(',').collect()
            }
        }
        let rows: Vec<(&str, Vec<&str>, Vec<&str>)> = lexicon_tsv
            .lines()
            .filter_map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                (f.len() == 4).then(|| (f[0], split(f[1]), split(f[3])))
            })
            .collect();
        let by_word: HashMap<&str, &Vec<&str>> = rows.iter().map(|r| (r.0, &r.1)).collect();
        let anchor_syn: Vec<HashSet<&str>> = SEEDS
            .iter()
            .map(|seeds| {
                seeds
                    .iter()
                    .filter_map(|s| by_word.get(s))
                    .flat_map(|v| v.iter().copied())
                    .collect()
            })
            .collect();
        let mut hits = HashMap::new();
        for (w, syn, hyp) in &rows {
            let mut h = [false; ANCHORS];
            for (a, set) in anchor_syn.iter().enumerate() {
                h[a] = syn.iter().chain(hyp.iter()).any(|s| set.contains(s));
            }
            if h.iter().any(|&b| b) {
                hits.insert(w.to_string(), h);
            }
        }
        for (a, seeds) in SEEDS.iter().enumerate() {
            for s in seeds.iter() {
                hits.entry(s.to_string()).or_insert([false; ANCHORS])[a] = true;
            }
        }
        Anchors { hits }
    }

    /// Anchor hits of one word, trying simple suffix stripping when unknown.
    pub fn word(&self, w: &str) -> [bool; ANCHORS] {
        if let Some(h) = self.hits.get(w) {
            return *h;
        }
        for suf in SUFFIXES {
            if let Some(stem) = w.strip_suffix(suf) {
                if stem.len() >= 3 {
                    if let Some(h) = self.hits.get(stem) {
                        return *h;
                    }
                }
            }
        }
        [false; ANCHORS]
    }

    /// Anchor hit counts over the main clause of a question.
    pub fn counts(&self, question: &str) -> [usize; ANCHORS] {
        let mut c = [0; ANCHORS];
        for w in words(main_clause(question)) {
            for (a, &h) in self.word(&w).iter().enumerate() {
                c[a] += usize::from(h);
            }
        }
        c
    }

    /// Number of words known to belong to at least one anchor.
    pub fn len(&self) -> usize {
        self.hits.len()
    }

    /// True when no word belongs to an anchor.
    pub fn is_empty(&self) -> bool {
        self.hits.is_empty()
    }
}

/// DEC-14 router features: DEC-12 features of the whole question plus the
/// anchor block (per anchor: count 0/1/2+ one-hot, and hit × main-clause
/// negation).
pub fn router14_features(
    lex: &WordCodes,
    coder: &QuestionCoder,
    cues: &HashSet<String>,
    anchors: &Anchors,
    question: &str,
) -> Vec<u32> {
    let mut out = router_features(lex, coder, cues, question);
    let c = anchors.counts(question);
    let neg = negated(cues, main_clause(question));
    for (a, &n) in c.iter().enumerate() {
        out.push((ROUTER_DIM + 3 * a + n.min(2)) as u32);
        if n > 0 {
            out.push((ROUTER_DIM + 15 + 2 * a + usize::from(neg)) as u32);
        }
    }
    out
}

/// Rule routing from anchors alone; `None` on a tie (no decision).
/// T1: near vs away hits, swapped when the main clause is negated.
/// T3: the unique largest of Opening/Middle/End hits.
pub fn anchor_rule(
    anchors: &Anchors,
    cues: &HashSet<String>,
    task: &str,
    q: &str,
) -> Option<Intent> {
    let c = anchors.counts(q);
    if task == "t1" {
        let (mut near, mut away) = (c[3], c[4]);
        if negated(cues, main_clause(q)) {
            core::mem::swap(&mut near, &mut away);
        }
        match near.cmp(&away) {
            core::cmp::Ordering::Greater => Some(Intent::Continues),
            core::cmp::Ordering::Less => Some(Intent::NotContinues),
            core::cmp::Ordering::Equal => None,
        }
    } else {
        let best = *c[..3].iter().max().unwrap_or(&0);
        let at: Vec<usize> = (0..3).filter(|&i| c[i] == best).collect();
        if best == 0 || at.len() != 1 {
            return None;
        }
        Some([Intent::Opening, Intent::Middle, Intent::End][at[0]])
    }
}
