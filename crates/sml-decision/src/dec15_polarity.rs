//! DEC-15: anchor routing with antonym polarity and a protected anchor block.
//!
//! Changes from DEC-14 (`dec14_anchor`), fixed before the reserved set below:
//! - seeds extended after the DEC-14 misses (declared: tuned on a spent set):
//!   Middle += intermediate, halfway, central; Near += consecutive,
//!   successive, adjacent, connected; Away += distant, remote;
//! - antonym polarity: a word whose OEWN antonym synsets meet an anchor's seed
//!   synsets is an "anti" hit; for T1, anti-Near counts as Away and anti-Away as
//!   Near (T3 ignores anti hits);
//! - each anchor feature fires `REP` slots so that the anchor block keeps its
//!   weight after the ±7 quantization.
//!
//! The `reserved` set (10 per intent) was written after these choices and
//! before any DEC-15 code or run, and is used only on TEST.

use crate::dec12_router::{router_features, ROUTER_DIM};
use crate::dec14_anchor;
use crate::dec2_text::words;
use crate::dec4_instr::Intent;
use crate::dec5_meaning::{QuestionCoder, WordCodes};
use crate::dec7_polarity::negated;
use std::collections::{HashMap, HashSet};

/// Anchor order: Opening, Middle, End, Continues (near), NotContinues (away).
pub const ANCHORS: usize = 5;
/// Slots per anchor feature.
pub const REP: usize = 4;
/// Router dimension with the replicated anchor block after the DEC-12 space.
pub const ROUTER15_DIM: usize = ROUTER_DIM + 25 * REP;

/// Seed words of each anchor, in `ANCHORS` order.
pub const SEEDS: [&[&str]; ANCHORS] = [
    &["beginning", "start", "first", "opening"],
    &[
        "middle",
        "center",
        "centre",
        "midst",
        "intermediate",
        "halfway",
        "central",
    ],
    &["end", "last", "finish", "ending"],
    &[
        "follow",
        "next",
        "continue",
        "after",
        "consecutive",
        "successive",
        "adjacent",
        "connected",
    ],
    &["elsewhere", "other", "separate", "gap", "distant", "remote"],
];

/// Contrast links: text from the first link on is out of scope.
pub const LINKS: [&str; 3] = [" rather than ", " instead of ", " as opposed to "];

/// Reserved wordings, used only in TEST `reserved`.
pub fn reserved(intent: Intent) -> &'static [&'static str] {
    match intent {
        Intent::Continues => &[
            "Does B come right on the heels of A?",
            "Is B the passage printed directly below A?",
            "Does the text run from A straight into B?",
            "Is B the immediate successor of A?",
            "Would B be read just after A in the original order?",
            "Is B joined to the end of A in the paper?",
            "Does B extend A without anything in between?",
            "Are A and B neighbours, with B second?",
            "Does B directly continue A rather than start a new topic elsewhere?",
            "Is B the subsequent passage instead of an unrelated one?",
        ],
        Intent::NotContinues => &[
            "Is B cut off from the end of A?",
            "Does B sit somewhere distant from A in the paper?",
            "Is there material missing between A and B?",
            "Is B misplaced as a follow-up to A?",
            "Does B come from a remote part of the article?",
            "Is B disconnected from the passage A?",
            "Does B break the flow that A starts?",
            "Is B not adjacent to A?",
            "Is B a fragment from a separate section instead of the next part after A?",
            "Does B belong elsewhere rather than directly after A?",
        ],
        Intent::Opening => &[
            "Is this from the earliest part of the paper?",
            "Does the document lead off with this passage?",
            "Is this one of the leading paragraphs of the article?",
            "Is this near where the paper commences?",
            "Is this the paper's preamble?",
            "Does this passage come at the very top?",
            "Is this from the first pages of the text?",
            "Would this be read at the outset?",
            "Is this from the onset of the article?",
            "Is this from the start of the paper as opposed to its middle?",
        ],
        Intent::Middle => &[
            "Does this passage come from around the midpoint of the article?",
            "Does this passage fall in the middle third of the article?",
            "Is this from somewhere midway through the text?",
            "Is this an inner section of the document?",
            "Does this text lie between the introduction and the conclusion?",
            "Is this from the paper's mid part?",
            "Is this passage centrally placed in the article?",
            "Would a reader find this about halfway in?",
            "Is this from the bulk of the paper, not its edges?",
            "Is this from the centre of the text rather than its start?",
        ],
        Intent::End => &[
            "Is this from the latest part of the paper?",
            "Does the document wrap up with this passage?",
            "Is this one of the closing paragraphs of the article?",
            "Is this near where the paper concludes?",
            "Is this the paper's epilogue?",
            "Does this passage come at the very bottom?",
            "Is this from the last pages of the text?",
            "Would this be read at the finish?",
            "Is this from the tail end of the article?",
            "Is this from the end of the paper as opposed to its beginning?",
        ],
    }
}

/// DEC-15 router training wordings: DEC-14 training plus the spent DEC-14
/// reserved set, deduplicated, minus the DEC-15 reserved set.
pub fn training_wordings(intent: Intent) -> Vec<String> {
    let mut out = dec14_anchor::training_wordings(intent);
    out.extend(dec14_anchor::reserved(intent).iter().map(|s| s.to_string()));
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
    anti: HashMap<String, [bool; ANCHORS]>,
}

const SUFFIXES: [&str; 6] = ["ing", "es", "ed", "ly", "s", "d"];

fn lookup(map: &HashMap<String, [bool; ANCHORS]>, w: &str) -> [bool; ANCHORS] {
    if let Some(h) = map.get(w) {
        return *h;
    }
    for suf in SUFFIXES {
        if let Some(stem) = w.strip_suffix(suf) {
            if stem.len() >= 3 {
                if let Some(h) = map.get(stem) {
                    return *h;
                }
            }
        }
    }
    [false; ANCHORS]
}

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
        type Row<'t> = (&'t str, Vec<&'t str>, Vec<&'t str>, Vec<&'t str>);
        let rows: Vec<Row> = lexicon_tsv
            .lines()
            .filter_map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                (f.len() == 4).then(|| (f[0], split(f[1]), split(f[2]), split(f[3])))
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
        let mut anti = HashMap::new();
        for (w, syn, ant, hyp) in &rows {
            let mut h = [false; ANCHORS];
            let mut n = [false; ANCHORS];
            for (a, set) in anchor_syn.iter().enumerate() {
                h[a] = syn.iter().chain(hyp.iter()).any(|s| set.contains(s));
                n[a] = ant.iter().any(|s| set.contains(s));
            }
            if h.iter().any(|&b| b) {
                hits.insert(w.to_string(), h);
            }
            if n.iter().any(|&b| b) {
                anti.insert(w.to_string(), n);
            }
        }
        for (a, seeds) in SEEDS.iter().enumerate() {
            for s in seeds.iter() {
                hits.entry(s.to_string()).or_insert([false; ANCHORS])[a] = true;
            }
        }
        Anchors { hits, anti }
    }

    /// Anchor hits of one word, trying simple suffix stripping when unknown.
    pub fn word(&self, w: &str) -> [bool; ANCHORS] {
        lookup(&self.hits, w)
    }

    /// Antonym ("anti") hits of one word, same suffix stripping.
    pub fn anti_word(&self, w: &str) -> [bool; ANCHORS] {
        lookup(&self.anti, w)
    }

    /// Anchor hit counts over the main clause of a question. For the T1
    /// anchors, anti-Near counts as Away and anti-Away as Near.
    pub fn counts(&self, question: &str) -> [usize; ANCHORS] {
        let mut c = [0; ANCHORS];
        for w in words(main_clause(question)) {
            for (a, &h) in self.word(&w).iter().enumerate() {
                c[a] += usize::from(h);
            }
            let n = self.anti_word(&w);
            c[3] += usize::from(n[4]);
            c[4] += usize::from(n[3]);
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

/// DEC-15 router features: DEC-12 features of the whole question plus the
/// anchor block (per anchor: count 0/1/2+ one-hot, and hit × main-clause
/// negation), each anchor feature firing `REP` slots.
pub fn router15_features(
    lex: &WordCodes,
    coder: &QuestionCoder,
    cues: &HashSet<String>,
    anchors: &Anchors,
    question: &str,
) -> Vec<u32> {
    let mut out = router_features(lex, coder, cues, question);
    let c = anchors.counts(question);
    let neg = negated(cues, main_clause(question));
    let mut slot = |k: usize| {
        for r in 0..REP {
            out.push((ROUTER_DIM + REP * k + r) as u32);
        }
    };
    for (a, &n) in c.iter().enumerate() {
        slot(3 * a + n.min(2));
        if n > 0 {
            slot(15 + 2 * a + usize::from(neg));
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
