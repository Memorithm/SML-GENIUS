//! DEC-13: a large reserved wording set and negation scope for the DEC-12
//! router.
//!
//! The `reserved` set (10 wordings per intent, 50 in total) was written before
//! the scope grammar and before any DEC-13 run, and is used only on TEST. It
//! includes contrastive wordings ("X rather than Y", "X instead of Y",
//! "X as opposed to Y") whose true intent is the first member. The author knew
//! the DEC-12 router miss ("instead of") when writing it: declared limit.
//!
//! The DEC-13 router keeps the DEC-12 features and training procedure; its
//! training wordings add a small scope grammar to the DEC-12 training set and
//! the spent DEC-12 reserved set.

use crate::dec12_router;
use crate::dec4_instr::Intent;

/// Reserved wordings, used only in TEST `reserved`.
pub fn reserved(intent: Intent) -> &'static [&'static str] {
    match intent {
        Intent::Continues => &[
            "Does B pick up exactly where A stops?",
            "Is B the very next stretch of text after A?",
            "Do A and B appear back to back, A first?",
            "Would a reader meet B right after finishing A?",
            "Is B what comes next once A ends?",
            "Does the paper go from A straight on to B?",
            "Is B adjacent to A and positioned after it?",
            "Does B follow on from A rather than come from another section?",
            "Is B the direct sequel to A instead of a passage from elsewhere?",
            "Does B continue A, as opposed to jumping somewhere else?",
        ],
        Intent::NotContinues => &[
            "Is there a gap between A and B in the paper?",
            "Was B lifted from a different place than right after A?",
            "Does B belong to another part of the paper, away from A?",
            "Is B unrelated in position to the end of A?",
            "Does something else come between A and B?",
            "Does A lead somewhere other than into B?",
            "Is it false that B comes right after A?",
            "Does B come from another section rather than follow A?",
            "Is B a passage from elsewhere instead of the sequel to A?",
            "Does B jump somewhere else, as opposed to continuing A?",
        ],
        Intent::Opening => &[
            "Is this passage from the very start of the paper?",
            "Does this text belong to the paper's opening section?",
            "Is this among the first few paragraphs?",
            "Would this appear near the top of the article?",
            "Is this how the paper begins?",
            "Is this an early passage of the document?",
            "Does this come before most of the paper?",
            "Is this from the introduction rather than the conclusion?",
            "Is this text from the front of the article?",
            "Does the paper open with this part?",
        ],
        Intent::Middle => &[
            "Is this passage from the central part of the paper?",
            "Does this text sit between the start and the finish?",
            "Is this from neither the beginning nor the end?",
            "Is this somewhere in the body of the article?",
            "Does this come from the paper's midsection?",
            "Is this halfway through the document?",
            "Is this passage far from both the opening and the close?",
            "Is this from the core of the paper rather than its edges?",
            "Would this be found in the middle stretch of the text?",
            "Is this from inside the paper, away from its first and last parts?",
        ],
        Intent::End => &[
            "Is this passage from the very end of the paper?",
            "Does this text belong to the paper's closing section?",
            "Is this among the final few paragraphs?",
            "Would this appear near the bottom of the article?",
            "Is this how the paper ends?",
            "Is this a late passage of the document?",
            "Does this come after most of the paper?",
            "Is this from the conclusion rather than the introduction?",
            "Is this text from the back of the article?",
            "Does the paper finish with this part?",
        ],
    }
}

const LINKS: [&str; 3] = ["rather than", "instead of", "as opposed to"];
const NEAR_V: [&str; 4] = [
    "follow A",
    "come right after A",
    "continue A",
    "come directly after A",
];
const NEAR_ING: [&str; 4] = [
    "following A",
    "coming right after A",
    "continuing A",
    "coming directly after A",
];
const AWAY_V: [&str; 4] = [
    "come from elsewhere",
    "skip ahead",
    "come from a distant part",
    "jump to a later part",
];
const AWAY_ING: [&str; 4] = [
    "coming from elsewhere",
    "skipping ahead",
    "coming from a distant part",
    "jumping to a later part",
];

fn place_nouns(intent: Intent) -> &'static [&'static str] {
    match intent {
        Intent::Opening => &["the start", "the opening", "the beginning"],
        Intent::Middle => &["the middle", "the body", "the centre"],
        Intent::End => &["the end", "the close", "the final part"],
        _ => &[],
    }
}

/// Scope grammar: "X <link> Y" wordings whose intent is X.
pub fn scope_grammar(intent: Intent) -> Vec<String> {
    let mut out = Vec::new();
    match intent {
        Intent::Continues => {
            for v in NEAR_V {
                for l in LINKS {
                    for a in AWAY_ING {
                        out.push(format!("Does B {v} {l} {a}?"));
                    }
                }
            }
        }
        Intent::NotContinues => {
            for v in AWAY_V {
                for l in LINKS {
                    for a in NEAR_ING {
                        out.push(format!("Does B {v} {l} {a}?"));
                    }
                }
            }
        }
        _ => {
            let others: Vec<&str> = [Intent::Opening, Intent::Middle, Intent::End]
                .into_iter()
                .filter(|&o| o != intent)
                .flat_map(place_nouns)
                .copied()
                .collect();
            for p in place_nouns(intent) {
                for l in LINKS {
                    for o in &others {
                        out.push(format!("Is this from {p} {l} {o}?"));
                    }
                }
            }
        }
    }
    out
}

/// DEC-13 router training wordings of `intent`: DEC-12 training wordings, the
/// spent DEC-12 reserved set and the scope grammar, deduplicated, minus the
/// DEC-13 reserved set.
pub fn training_wordings(intent: Intent) -> Vec<String> {
    let mut out = dec12_router::training_wordings(intent);
    out.extend(dec12_router::reserved(intent).iter().map(|s| s.to_string()));
    out.extend(scope_grammar(intent));
    let banned = reserved(intent);
    let mut kept: Vec<String> = Vec::new();
    for w in out {
        if !banned.contains(&w.as_str()) && !kept.contains(&w) {
            kept.push(w);
        }
    }
    kept
}
