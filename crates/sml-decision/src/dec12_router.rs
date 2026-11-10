//! DEC-12: a two-stage instruction reader.
//!
//! Stage 1 (router) reads only the question and maps it to an intent: for a
//! T1 row (two passages) Continues or NotContinues, for a T3 row (one passage)
//! Opening, Middle or End. The row's shape tells the task, as in DEC-4. The
//! router is a DEC-2 averaged perceptron over hashed question words, word
//! pairs, the 64-bit lexicon question code and the DEC-7 polarity bit. It is
//! trained on question wordings alone, with no passage.
//! Stage 2 answers with the DEC-2 content heads (B continues A; level of the
//! passage), which never see the question.
//!
//! The `reserved` set below was written before any DEC-12 run and is used only
//! on TEST.

use crate::dec10_grammar;
use crate::dec11_capacity;
use crate::dec2_text::words;
use crate::dec4_instr::Intent;
use crate::dec5_meaning::{QuestionCoder, WordCodes};
use crate::dec7_polarity::negated;
use std::collections::HashSet;

/// Router feature space (power of two).
pub const ROUTER_DIM: usize = 1 << 16;
/// Router training epochs (fixed, no DEV selection: the router has no DEV).
pub const ROUTER_EPOCHS: usize = 16;
/// Router weight levels after quantization (±1..±7, no pruning).
pub const ROUTER_LEVELS: i64 = 7;

/// Reserved wordings, used only in TEST `reserved`.
pub fn reserved(intent: Intent) -> &'static [&'static str] {
    match intent {
        Intent::Continues => &[
            "Is B the passage the author put directly after A?",
            "Does reading A then B follow the original order?",
            "Was B placed immediately after A in the paper?",
            "Is the text A followed by B?",
        ],
        Intent::NotContinues => &[
            "Is B taken out of order relative to A?",
            "Would the author not have written B right after A?",
            "Is B somewhere else in the text, not just after A?",
            "Does B skip ahead or come from elsewhere instead of following A?",
        ],
        Intent::Opening => &[
            "Is this part of the article's first lines?",
            "Does this text come at the outset of the paper?",
        ],
        Intent::Middle => &[
            "Is this passage from the middle portion of the paper?",
            "Is this text located mid-article?",
        ],
        Intent::End => &[
            "Is this part of the article's last lines?",
            "Does this text come at the close of the paper?",
        ],
    }
}

/// Router training wordings of `intent`: the DEC-10 grammar plus the spent
/// DEC-10 and DEC-11 reserved sets, deduplicated, minus the DEC-12 reserved set.
pub fn training_wordings(intent: Intent) -> Vec<String> {
    let mut out = dec10_grammar::grammar(intent);
    out.extend(
        dec10_grammar::reserved(intent)
            .iter()
            .map(|s| s.to_string()),
    );
    out.extend(
        dec11_capacity::reserved(intent)
            .iter()
            .map(|s| s.to_string()),
    );
    let banned = reserved(intent);
    let mut kept: Vec<String> = Vec::new();
    for w in out {
        if !banned.contains(&w.as_str()) && !kept.contains(&w) {
            kept.push(w);
        }
    }
    kept
}

fn fnv(parts: &[&[u8]]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for p in parts {
        for &b in p.iter().chain(core::iter::once(&0xff)) {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    h
}

/// Reserved slots at the top of the router space: 2 per code bit, 2 polarity.
const TAIL: usize = 130;

/// Router features of one question.
pub fn router_features(
    lex: &WordCodes,
    coder: &QuestionCoder,
    cues: &HashSet<String>,
    question: &str,
) -> Vec<u32> {
    let hashed = (ROUTER_DIM - TAIL) as u64;
    let ws = words(question);
    let mut out: Vec<u32> = Vec::new();
    for w in &ws {
        out.push((fnv(&[b"w", w.as_bytes()]) % hashed) as u32);
    }
    for p in ws.windows(2) {
        out.push((fnv(&[b"p", p[0].as_bytes(), p[1].as_bytes()]) % hashed) as u32);
    }
    let base = ROUTER_DIM - TAIL;
    let code = coder.bits(lex, question);
    for k in 0..64 {
        out.push((base + 2 * k + (code >> k & 1) as usize) as u32);
    }
    out.push((base + 128 + usize::from(negated(cues, question))) as u32);
    out.sort_unstable();
    out.dedup();
    out
}

/// Router class of an intent within its task (T1: 1 = Continues; T3: level).
pub fn class_of(intent: Intent) -> usize {
    match intent {
        Intent::Continues => 1,
        Intent::NotContinues => 0,
        Intent::Opening => 0,
        Intent::Middle => 1,
        Intent::End => 2,
    }
}

/// Intent of a router class for a task.
pub fn intent_of(task: &str, class: usize) -> Intent {
    match (task, class) {
        ("t1", 1) => Intent::Continues,
        ("t1", _) => Intent::NotContinues,
        (_, 0) => Intent::Opening,
        (_, 1) => Intent::Middle,
        _ => Intent::End,
    }
}
