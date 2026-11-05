//! DEC-5: Boolean word-meaning codes so that new question wordings can be read.
//!
//! DEC-4 heads read question words as identities, so a new word ("false",
//! "start") carries nothing. DEC-5 gives each word a 64-bit Boolean code
//! learned from the TRAIN texts alone:
//!
//! * every context word v has a fixed pseudo-random sign vector r(v) in
//!   {−1, +1}^64 (the bits of a 64-bit hash);
//! * S_w = Σ r(v) over the words v within `WINDOW` of each occurrence of w;
//! * bit k of code(w) is on iff S_w[k] / n_w > M[k], where M is the mean of
//!   r over all contexts (so frequent context words do not dominate).
//!
//! Words with fewer than `MIN_CONTEXTS` contexts get no code. A question is
//! coded as one 64-bit word too: bit k is on iff the sum over its coded words
//! of ±1 (bit k of their codes) is above the mean of that sum over the TRAIN
//! questions. Similar questions then share bits whether or not their words
//! were seen in a training question.

use std::collections::HashMap;

use crate::dec2_text::words;
use crate::dec4_instr::DIM4;

/// Context window (words on each side).
pub const WINDOW: usize = 2;
/// Minimum contexts for a word to get a code.
pub const MIN_CONTEXTS: u64 = 20;

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

/// The fixed sign vector of a context word, as 64 bits (1 = +1).
pub fn signs(word: &str) -> u64 {
    // splitmix64 finalizer spreads the FNV hash over all 64 bits.
    let mut z = fnv(&[b"r", word.as_bytes()]);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Learned 64-bit codes of the vocabulary.
pub struct WordCodes {
    codes: HashMap<String, u64>,
}

impl WordCodes {
    /// Learns codes from texts, in the given order.
    pub fn learn<'a>(texts: impl IntoIterator<Item = &'a str>) -> Self {
        let mut acc: HashMap<String, ([i64; 64], u64)> = HashMap::new();
        let mut total = [0i64; 64];
        let mut contexts = 0u64;
        for t in texts {
            let w = words(t);
            let r: Vec<u64> = w.iter().map(|x| signs(x)).collect();
            for i in 0..w.len() {
                let lo = i.saturating_sub(WINDOW);
                let hi = (i + WINDOW + 1).min(w.len());
                let e = acc.entry(w[i].clone()).or_insert(([0; 64], 0));
                for (j, &rv) in r.iter().enumerate().take(hi).skip(lo) {
                    if j == i {
                        continue;
                    }
                    for (k, s) in e.0.iter_mut().enumerate() {
                        let v = if rv >> k & 1 == 1 { 1 } else { -1 };
                        *s += v;
                        total[k] += v;
                    }
                    e.1 += 1;
                    contexts += 1;
                }
            }
        }
        let mean: Vec<f64> = total
            .iter()
            .map(|&t| t as f64 / contexts.max(1) as f64)
            .collect();
        let codes = acc
            .into_iter()
            .filter(|(_, (_, n))| *n >= MIN_CONTEXTS)
            .map(|(w, (s, n))| {
                let mut c = 0u64;
                for k in 0..64 {
                    if s[k] as f64 > mean[k] * n as f64 {
                        c |= 1 << k;
                    }
                }
                (w, c)
            })
            .collect();
        Self { codes }
    }

    pub fn code(&self, word: &str) -> Option<u64> {
        self.codes.get(word).copied()
    }

    pub fn len(&self) -> usize {
        self.codes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.codes.is_empty()
    }

    /// Order-independent fingerprint of the codes.
    pub fn fingerprint(&self) -> u64 {
        let mut v: Vec<(&String, &u64)> = self.codes.iter().collect();
        v.sort_unstable();
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for (w, c) in v {
            h ^= fnv(&[w.as_bytes(), &c.to_le_bytes()]);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        h
    }

    /// Per-bit sum of ±1 over the coded words of a question.
    pub fn question_sum(&self, question: &str) -> [i32; 64] {
        let mut s = [0i32; 64];
        for w in words(question) {
            if let Some(c) = self.code(&w) {
                for (k, v) in s.iter_mut().enumerate() {
                    *v += if c >> k & 1 == 1 { 1 } else { -1 };
                }
            }
        }
        s
    }
}

/// Codes a question as 64 bits, centred on the mean TRAIN question.
pub struct QuestionCoder {
    mean: [f64; 64],
}

impl QuestionCoder {
    pub fn fit<'a>(codes: &WordCodes, questions: impl IntoIterator<Item = &'a str>) -> Self {
        let mut sum = [0f64; 64];
        let mut n = 0usize;
        for q in questions {
            for (a, b) in sum.iter_mut().zip(codes.question_sum(q)) {
                *a += f64::from(b);
            }
            n += 1;
        }
        Self {
            mean: sum.map(|s| s / n.max(1) as f64),
        }
    }

    pub fn bits(&self, codes: &WordCodes, question: &str) -> u64 {
        let s = codes.question_sum(question);
        (0..64)
            .filter(|&k| f64::from(s[k]) > self.mean[k])
            .fold(0u64, |c, k| c | 1 << k)
    }
}

fn idx(parts: &[&[u8]]) -> u32 {
    (fnv(parts) as usize & (DIM4 - 1)) as u32
}

/// DEC-5 features: content bits, kind tag, and the question-code bits alone,
/// with the kind tag and ANDed with every content bit. With `ident`, the
/// DEC-4 question-word features are added (`dec4_instr::features`).
pub fn features(task: &str, content: &[u32], qbits: u64, ident: Option<&str>) -> Vec<u32> {
    let kind = task.as_bytes();
    let mut out = crate::dec4_instr::features(task, content, ident);
    let cs: Vec<[u8; 4]> = content.iter().map(|c| c.to_le_bytes()).collect();
    for k in (0..64u8).filter(|&k| qbits >> k & 1 == 1) {
        let kb = [k];
        out.push(idx(&[b"m", &kb]));
        out.push(idx(&[b"mk", &kb, kind]));
        out.extend(cs.iter().map(|c| idx(&[b"mx", &kb, kind, c])));
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Hamming distance between two word codes, if both exist.
pub fn hamming(codes: &WordCodes, a: &str, b: &str) -> Option<u32> {
    Some((codes.code(a)? ^ codes.code(b)?).count_ones())
}
