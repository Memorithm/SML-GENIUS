//! DEC-9: many training wordings from a small grammar, and a new reserved set.
//!
//! DEC-8 showed that heads trained on 13 wordings do not reach wordings they
//! have not seen (0.559). DEC-9 trains the same heads on a few hundred
//! wordings: every frame of a small grammar crossed with every predicate,
//! plus every DEC-4 to DEC-8 wording (those sets are spent as tests). The
//! `reserved` wordings were written before the grammar and are never used in
//! TRAIN or DEV; any grammar wording equal to one of them is dropped.

use crate::dec4_instr::{held_out, seen, Intent};
use crate::dec8_fresh::fresh;

/// Reserved wordings, used only in TEST `reserved` (written before the grammar).
pub fn reserved(intent: Intent) -> &'static [&'static str] {
    match intent {
        Intent::Continues => &[
            "Is B the next bit of A?",
            "Does the text go on from A to B?",
            "Is B where A carries on?",
            "Would a reader see B right after A?",
        ],
        Intent::NotContinues => &[
            "Is B misplaced after A?",
            "Does B break the flow from A?",
            "Is B not the next passage after A?",
            "Would it be a mistake to put B after A?",
        ],
        Intent::Opening => &[
            "Is this passage near the top of the paper?",
            "Does the article begin with this text?",
        ],
        Intent::Middle => &[
            "Is this from the body of the article, away from both ends?",
            "Does this passage come midway through the paper?",
        ],
        Intent::End => &[
            "Is this near the bottom of the paper?",
            "Does the article finish with this text?",
        ],
    }
}

/// Base-form verb phrases meaning "B continues A" (object A follows).
const CONT: [&str; 12] = [
    "continue",
    "follow",
    "come after",
    "come right after",
    "come directly after",
    "come next after",
    "carry on from",
    "pick up from",
    "go on from",
    "succeed",
    "immediately follow",
    "directly follow",
];

/// Positive frames: `{p}` is a CONT phrase.
const POS: [&str; 6] = [
    "Does B {p} A?",
    "Can B {p} A?",
    "Would B {p} A?",
    "Should B {p} A?",
    "Is it true that B would {p} A?",
    "Is it correct that B should {p} A?",
];

/// Negative frames: `{p}` is a CONT phrase.
const NEG: [&str; 8] = [
    "Does B not {p} A?",
    "Would B not {p} A?",
    "Is it false that B would {p} A?",
    "Is it wrong that B should {p} A?",
    "Is it untrue that B can {p} A?",
    "Is it incorrect that B would {p} A?",
    "Does B fail to {p} A?",
    "Can B never {p} A?",
];

/// Adjective phrases meaning "B does not continue A" (object A follows).
const BREAK: [&str; 6] = [
    "out of place after",
    "unrelated to",
    "disconnected from",
    "misplaced after",
    "foreign to the text after",
    "out of order after",
];

/// Frames for BREAK phrases.
const BREAK_FRAMES: [&str; 3] = [
    "Is B {p} A?",
    "Would B be {p} A?",
    "Is it true that B is {p} A?",
];

/// Position nouns per level.
const OPENING: [&str; 7] = [
    "opening",
    "beginning",
    "start",
    "first part",
    "introduction",
    "first section",
    "early part",
];
const MIDDLE: [&str; 6] = [
    "middle",
    "central part",
    "midpoint",
    "middle section",
    "core",
    "inner part",
];
const END: [&str; 8] = [
    "end",
    "closing part",
    "final part",
    "last part",
    "conclusion",
    "final section",
    "ending",
    "late part",
];

/// Position frames: `{n}` is a position noun.
const PLACE: [&str; 6] = [
    "Is this passage from the {n} of the article?",
    "Does this text come from the {n} of the paper?",
    "Was this taken from the {n} of the article?",
    "Is this from the {n} of the paper?",
    "Does this passage belong to the {n} of the article?",
    "Is this text part of the {n} of the paper?",
];

fn cross(frames: &[&str], fill: &[&str], key: &str) -> Vec<String> {
    frames
        .iter()
        .flat_map(|f| fill.iter().map(move |p| f.replace(key, p)))
        .collect()
}

/// TRAIN/DEV wordings of `intent`: the grammar plus every spent DEC-4 to
/// DEC-8 wording, deduplicated, minus the reserved set, in a fixed order.
pub fn grammar(intent: Intent) -> Vec<String> {
    let mut out = match intent {
        Intent::Continues => cross(&POS, &CONT, "{p}"),
        Intent::NotContinues => {
            let mut v = cross(&NEG, &CONT, "{p}");
            v.extend(cross(&BREAK_FRAMES, &BREAK, "{p}"));
            v
        }
        Intent::Opening => cross(&PLACE, &OPENING, "{n}"),
        Intent::Middle => cross(&PLACE, &MIDDLE, "{n}"),
        Intent::End => cross(&PLACE, &END, "{n}"),
    };
    for set in [seen(intent), held_out(intent), fresh(intent)] {
        out.extend(set.iter().map(|s| s.to_string()));
    }
    let banned = reserved(intent);
    let mut kept: Vec<String> = Vec::new();
    for w in out {
        if !banned.contains(&w.as_str()) && !kept.contains(&w) {
            kept.push(w);
        }
    }
    kept
}
