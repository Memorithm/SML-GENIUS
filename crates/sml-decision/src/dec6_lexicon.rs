//! DEC-6: Boolean word codes from an open lexicon (Open English WordNet).
//!
//! DEC-5 context codes put opposites together ("beginning"/"end" 2 bits
//! apart). Here a word's 64-bit code comes from the lexicon:
//!
//!   V(w) = Σ_{s ∈ synsets(w)} r(s) + ½ Σ_{h ∈ hypernyms(w)} r(h) − Σ_{a ∈ antonyms(w)} r(a)
//!
//! with r(x) ∈ {±1}^64 fixed signs of a synset id. Bit k is on iff V(w)[k] > 0.
//! Synonyms share synsets, so their codes agree. A word and its antonym
//! carry each other's synsets with opposite signs, so they disagree on every
//! bit where those synsets' signs differ (about half): opposites are no closer
//! than unrelated words, unlike DEC-5 context codes.
//! Unknown inflected forms are looked up through `LEMMA_RULES`. The question
//! coder and features are DEC-5's.

use std::collections::HashMap;

use crate::dec4_instr::DIM4;
use crate::dec5_meaning::{signs, WordCodes};

/// Suffix rules tried in order for a word missing from the lexicon.
pub const LEMMA_RULES: [(&str, &str); 7] = [
    ("ies", "y"),
    ("es", ""),
    ("s", ""),
    ("ed", ""),
    ("ed", "e"),
    ("ing", ""),
    ("ing", "e"),
];

/// Lemma candidates of an inflected word, in rule order.
pub fn lemma_candidates(word: &str) -> Vec<String> {
    LEMMA_RULES
        .iter()
        .filter_map(|(suf, rep)| {
            let stem = word.strip_suffix(suf)?;
            (stem.len() >= 2).then(|| format!("{stem}{rep}"))
        })
        .collect()
}

fn add(v: &mut [i32; 64], id: &str, weight: i32) {
    let r = signs(&format!("syn:{id}"));
    for (k, x) in v.iter_mut().enumerate() {
        *x += if r >> k & 1 == 1 { weight } else { -weight };
    }
}

fn ids(col: &str) -> impl Iterator<Item = &str> {
    col.split(',').filter(|s| !s.is_empty() && *s != "-")
}

/// The 64-bit code of one lexicon row (synsets, antonym synsets, hypernyms).
pub fn row_code(synsets: &str, antonyms: &str, hypernyms: &str) -> u64 {
    // Weights doubled to keep the ½ hypernym weight integral.
    let mut v = [0i32; 64];
    ids(synsets).for_each(|s| add(&mut v, s, 2));
    ids(hypernyms).for_each(|s| add(&mut v, s, 1));
    ids(antonyms).for_each(|s| add(&mut v, s, -2));
    (0..64).filter(|&k| v[k] > 0).fold(0, |c, k| c | 1 << k)
}

/// Lexicon codes from the DEC-6 TSV (word, synsets, antonyms, hypernyms).
pub fn lexicon_codes(tsv: &str) -> Result<WordCodes, String> {
    let mut codes = HashMap::new();
    for (i, line) in tsv.lines().enumerate() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() != 4 {
            return Err(format!("lexicon line {} has {} fields", i + 1, f.len()));
        }
        codes.insert(f[0].to_string(), row_code(f[1], f[2], f[3]));
    }
    Ok(WordCodes::from_codes(codes, true))
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

fn idx(parts: &[&[u8]]) -> u32 {
    (fnv(parts) as usize & (DIM4 - 1)) as u32
}

/// DEC-6 features: DEC-4 content bits and kind tag, then for each tagged
/// question code: its bits alone, with the kind tag, and ANDed with every
/// content bit. Tags keep the lexicon and context codes apart.
pub fn features(task: &str, content: &[u32], codes: &[(&[u8], u64)]) -> Vec<u32> {
    let kind = task.as_bytes();
    let mut out = crate::dec4_instr::features(task, content, None);
    let cs: Vec<[u8; 4]> = content.iter().map(|c| c.to_le_bytes()).collect();
    for &(tag, bits) in codes {
        for k in (0..64u8).filter(|&k| bits >> k & 1 == 1) {
            let kb = [k];
            out.push(idx(&[b"l", tag, &kb]));
            out.push(idx(&[b"lk", tag, &kb, kind]));
            out.extend(cs.iter().map(|c| idx(&[b"lx", tag, &kb, kind, c])));
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}
