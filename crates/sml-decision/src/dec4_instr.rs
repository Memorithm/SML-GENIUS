//! DEC-4: Boolean heads that must read the question.
//!
//! Each DEC-1 T1 (continuation) or T3 (position) example is asked as a yes/no
//! question whose wording changes from row to row. Every example appears
//! twice with two different intents and opposite answers, so a head that
//! ignores the question scores exactly one row of each pair: 0.5.
//!
//! Features (all hashed into `DIM4` bits): the DEC-2 content bits, a content
//! kind tag, the question's word bits, and the conjunction (AND) of every
//! question word with every content bit and the kind tag. The heads are the
//! DEC-2 heads, unchanged, over this larger space. TEST is asked twice: with
//! the wordings seen in training (`seen`) and with held-out wordings (`new`).

use crate::dec2_text::{pair_bits, text_bits, words};

/// Hashed feature space of DEC-4 (power of two).
pub const DIM4: usize = 1 << 20;

/// What a question asks; the answer depends on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Intent {
    /// T1: yes iff B continues A.
    Continues,
    /// T1: yes iff B does not continue A.
    NotContinues,
    /// T3: yes iff the passage is from the opening (level 0).
    Opening,
    /// T3: yes iff the passage is from the middle (level 1).
    Middle,
    /// T3: yes iff the passage is from the end (level 2).
    End,
}

pub const T3_INTENTS: [Intent; 3] = [Intent::Opening, Intent::Middle, Intent::End];

/// Wordings used in TRAIN, DEV and TEST `seen`.
pub fn seen(intent: Intent) -> &'static [&'static str] {
    match intent {
        Intent::Continues => &[
            "Does passage B continue passage A?",
            "Is B the next part of the text right after A?",
            "Does B follow A in the same article?",
        ],
        Intent::NotContinues => &[
            "Is B taken from somewhere other than right after A?",
            "Does B not continue passage A?",
            "Is B out of place after A?",
        ],
        Intent::Opening => &[
            "Is this passage from the opening of the article?",
            "Does this text come from the beginning of the paper?",
        ],
        Intent::Middle => &[
            "Is this passage from the middle of the article?",
            "Does this text come from the central part of the paper?",
        ],
        Intent::End => &[
            "Is this passage from the end of the article?",
            "Does this text come from the closing part of the paper?",
        ],
    }
}

/// Held-out wordings, used only in TEST `new`.
pub fn held_out(intent: Intent) -> &'static [&'static str] {
    match intent {
        Intent::Continues => &[
            "Would B come directly after A?",
            "Is B what the author wrote next after A?",
        ],
        Intent::NotContinues => &[
            "Would B be wrong as the next part after A?",
            "Is it false that B continues A?",
        ],
        Intent::Opening => &["Was this taken from the start of the article?"],
        Intent::Middle => &["Is this from somewhere in the middle part of the paper?"],
        Intent::End => &["Was this taken from the final part of the article?"],
    }
}

/// The yes/no answer of `intent` for a DEC-1 label (T1: 0/1, T3: level).
pub fn answer(intent: Intent, label: usize) -> usize {
    let yes = match intent {
        Intent::Continues => label == 1,
        Intent::NotContinues => label == 0,
        Intent::Opening => label == 0,
        Intent::Middle => label == 1,
        Intent::End => label == 2,
    };
    usize::from(yes)
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

/// The two intents asked of one example: T1 both polarities; T3 the true
/// level and one other level, chosen by the example id.
pub fn intents(task: &str, id: &str, label: usize) -> Vec<Intent> {
    match task {
        "t1" => vec![Intent::Continues, Intent::NotContinues],
        "t3" => {
            let other = (label + 1 + (fnv(&[b"other", id.as_bytes()]) % 2) as usize) % 3;
            vec![T3_INTENTS[label % 3], T3_INTENTS[other]]
        }
        _ => Vec::new(),
    }
}

/// Picks a wording of `intent` from `set` for one row, by split, id and intent.
pub fn wording(
    set: &'static [&'static str],
    split: &str,
    id: &str,
    intent: Intent,
) -> &'static str {
    let tag = format!("{intent:?}");
    set[(fnv(&[split.as_bytes(), id.as_bytes(), tag.as_bytes()]) % set.len() as u64) as usize]
}

/// DEC-2 content bits: T1 pair bits of (A, B), T3 text bits of the passage.
pub fn content_bits(task: &str, texts: &[&str]) -> Vec<u32> {
    match (task, texts) {
        ("t1", [a, b]) => pair_bits(&text_bits(a), &text_bits(b)),
        ("t3", [p]) => text_bits(p),
        _ => Vec::new(),
    }
}

fn idx(parts: &[&[u8]]) -> u32 {
    (fnv(parts) as usize & (DIM4 - 1)) as u32
}

/// Hashed features of one row. With `question = None` (the BLIND arm) only
/// the content bits and the kind tag are set.
pub fn features(task: &str, content: &[u32], question: Option<&str>) -> Vec<u32> {
    let kind = task.as_bytes();
    let cs: Vec<[u8; 4]> = content.iter().map(|c| c.to_le_bytes()).collect();
    let mut out: Vec<u32> = cs.iter().map(|c| idx(&[b"c", kind, c])).collect();
    out.push(idx(&[b"k", kind]));
    if let Some(q) = question {
        let mut qw = words(q);
        qw.sort_unstable();
        qw.dedup();
        for w in &qw {
            let w = w.as_bytes();
            out.push(idx(&[b"q", w]));
            out.push(idx(&[b"qk", w, kind]));
            out.extend(cs.iter().map(|c| idx(&[b"x", w, kind, c])));
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}
