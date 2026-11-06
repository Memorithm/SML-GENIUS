//! DEC-10: a richer wording grammar (T3 verbs and places) and a new reserved set.
//!
//! DEC-9 trained on 333 grammar wordings and reached 0.599 on its reserved
//! set: T1 0.645, but T3 only 0.554, because its T3 frames varied only the
//! position noun. DEC-10 adds T3 frames built on verbs ("does the article end
//! with ...") and places ("near the top of the paper"), and T1 noun frames
//! ("is B the continuation of A"). It also adds the spent DEC-9 reserved
//! wordings. The `reserved` set below was written before these additions.
//! Grammar wordings one word away from a reserved wording were left out
//! (the passage verbs "start" and "wrap up").

use crate::dec4_instr::Intent;
use crate::dec9_grammar;

/// Reserved wordings, used only in TEST `reserved` (written before the grammar).
pub fn reserved(intent: Intent) -> &'static [&'static str] {
    match intent {
        Intent::Continues => &[
            "Does B pick up right where A stops?",
            "Is B the following part of A's text?",
            "Would B be next in line after A?",
            "Does B come straight on from A?",
        ],
        Intent::NotContinues => &[
            "Is B from a different place than right after A?",
            "Does B not belong right after A?",
            "Would B be an odd thing to read after A?",
            "Is it a stretch to say B comes after A?",
        ],
        Intent::Opening => &[
            "Is this one of the first things the article says?",
            "Does this passage start the paper off?",
        ],
        Intent::Middle => &[
            "Is this passage somewhere in the middle stretch of the article?",
            "Does this text fall between the start and the end of the paper?",
        ],
        Intent::End => &[
            "Is this one of the last things the article says?",
            "Does this passage wrap up the paper?",
        ],
    }
}

const NOUNS: [&str; 6] = [
    "continuation",
    "next part",
    "next passage",
    "follow-up",
    "sequel",
    "next section",
];
const NOUN_POS: [&str; 2] = ["Is B the {n} of A?", "Is it true that B is the {n} of A?"];
const NOUN_NEG: [&str; 2] = [
    "Is B not the {n} of A?",
    "Is it false that B is the {n} of A?",
];

const OPEN_DOC_VERBS: [&str; 4] = ["begin with", "open with", "start with", "lead with"];
const END_DOC_VERBS: [&str; 4] = ["end with", "finish with", "close with", "conclude with"];
const DOC_FRAMES: [&str; 2] = [
    "Does the article {v} this text?",
    "Does the paper {v} this passage?",
];

const OPEN_TEXT_VERBS: [&str; 3] = ["open", "begin", "introduce"];
const END_TEXT_VERBS: [&str; 4] = ["end", "close", "conclude", "finish"];
const TEXT_FRAMES: [&str; 2] = [
    "Does this passage {v} the article?",
    "Does this text {v} the paper?",
];

const OPEN_PLACES: [&str; 6] = [
    "near the top of the paper",
    "at the start of the article",
    "early in the article",
    "near the beginning of the paper",
    "at the very start of the article",
    "in the opening lines of the paper",
];
const MIDDLE_PLACES: [&str; 6] = [
    "halfway through the article",
    "midway through the paper",
    "in the middle of the article",
    "in the body of the paper",
    "in the central part of the article",
    "deep inside the paper",
];
const END_PLACES: [&str; 6] = [
    "near the bottom of the paper",
    "at the end of the article",
    "late in the article",
    "near the end of the paper",
    "at the very end of the article",
    "in the closing lines of the paper",
];
const PLACE_FRAMES: [&str; 3] = [
    "Is this {s}?",
    "Does this passage appear {s}?",
    "Was this text written {s}?",
];

fn cross(frames: &[&str], fill: &[&str], key: &str) -> Vec<String> {
    frames
        .iter()
        .flat_map(|f| fill.iter().map(move |p| f.replace(key, p)))
        .collect()
}

/// TRAIN/DEV wordings of `intent`: the DEC-9 grammar, the spent DEC-9
/// reserved wordings and the DEC-10 additions, deduplicated, minus the
/// DEC-10 reserved set, in a fixed order.
pub fn grammar(intent: Intent) -> Vec<String> {
    let mut out = dec9_grammar::grammar(intent);
    out.extend(dec9_grammar::reserved(intent).iter().map(|s| s.to_string()));
    out.extend(match intent {
        Intent::Continues => cross(&NOUN_POS, &NOUNS, "{n}"),
        Intent::NotContinues => cross(&NOUN_NEG, &NOUNS, "{n}"),
        Intent::Opening => {
            let mut v = cross(&DOC_FRAMES, &OPEN_DOC_VERBS, "{v}");
            v.extend(cross(&TEXT_FRAMES, &OPEN_TEXT_VERBS, "{v}"));
            v.extend(cross(&PLACE_FRAMES, &OPEN_PLACES, "{s}"));
            v
        }
        Intent::Middle => cross(&PLACE_FRAMES, &MIDDLE_PLACES, "{s}"),
        Intent::End => {
            let mut v = cross(&DOC_FRAMES, &END_DOC_VERBS, "{v}");
            v.extend(cross(&TEXT_FRAMES, &END_TEXT_VERBS, "{v}"));
            v.extend(cross(&PLACE_FRAMES, &END_PLACES, "{s}"));
            v
        }
    });
    let banned = reserved(intent);
    let mut kept: Vec<String> = Vec::new();
    for w in out {
        if !banned.contains(&w.as_str()) && !kept.contains(&w) {
            kept.push(w);
        }
    }
    kept
}
