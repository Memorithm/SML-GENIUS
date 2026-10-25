//! HYB H-2t: can recurring document templates be recognised before an
//! expert is chosen? Feasibility diagnostic, no verdict on any held-out
//! sample. Protocol: docs/HYB-H2T-TEMPLATE-FEASIBILITY.md.
//!
//! H-2 routed each byte on P1's own state and found nothing a single
//! H-1d head did not already see. A template (section heading, reference
//! list, citation brackets, LaTeX, table rows, boilerplate) is a property
//! of the *shape* of the recent text, which P1's byte contexts only see
//! locally. This module gives:
//!
//! * `ShapeState`: a causal shape signature of `SHAPE_WORDS` words built
//!   from the bytes already decoded (character classes, class bigrams over
//!   the last `WINDOW` bytes, line layout), so a decoder can recompute it;
//! * `cluster`: flat templates learned without labels, Hamming k-means
//!   with bitwise-majority centres;
//! * `agreement`: how well two clusterings agree (stability across seeds);
//! * `MatchModel`: an LZP-style match predictor, to size what the fixed
//!   text of templates (exact repeats) could add.
//!
//! Research only, NOT_PROMOTED.

use crate::hyb_h1_residual::splitmix;

/// Words of shape signature per byte.
pub const SHAPE_WORDS: usize = 2;
/// Bytes over which class bigrams are counted.
pub const WINDOW: usize = 64;
/// Character classes.
pub const CLASSES: usize = 8;

pub type Shape = [u64; SHAPE_WORDS];

/// Character class of a byte: lower, upper, digit, space or tab, newline,
/// bracket, math or markup symbol, anything else.
#[inline]
#[must_use]
pub fn class(b: u8) -> u8 {
    match b {
        b'a'..=b'z' => 0,
        b'A'..=b'Z' => 1,
        b'0'..=b'9' => 2,
        b' ' | b'\t' => 3,
        b'\n' | b'\r' => 4,
        b'(' | b')' | b'[' | b']' | b'{' | b'}' => 5,
        b'\\' | b'$' | b'^' | b'_' | b'=' | b'+' | b'<' | b'>' | b'|' | b'&' | b'%' => 6,
        _ => 7,
    }
}

/// Causal shape of the text decoded so far in one document.
#[derive(Debug, Clone)]
pub struct ShapeState {
    /// Classes of the last `WINDOW + 1` bytes, ring buffer.
    ring: [u8; WINDOW + 1],
    len: usize,
    bigrams: [u16; CLASSES * CLASSES],
    /// Bytes of each class among the last `WINDOW` bytes.
    counts: [u16; CLASSES],
    column: u32,
    line_first: Option<u8>,
    prev_line_len: u32,
    prev_line_dot: bool,
    last_byte: u8,
    open_brackets: i32,
    dollars: u32,
}

impl Default for ShapeState {
    fn default() -> Self {
        Self::new()
    }
}

impl ShapeState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            ring: [0; WINDOW + 1],
            len: 0,
            bigrams: [0; CLASSES * CLASSES],
            counts: [0; CLASSES],
            column: 0,
            line_first: None,
            prev_line_len: 0,
            prev_line_dot: false,
            last_byte: 0,
            open_brackets: 0,
            dollars: 0,
        }
    }

    fn class_back(&self, back: usize) -> u8 {
        self.ring[(self.len - 1 - back) % (WINDOW + 1)]
    }

    /// Signature of the next byte, from the bytes pushed so far.
    ///
    /// Word 0: which class bigrams occur among the last `WINDOW` bytes.
    /// Word 1: classes of the last 4 bytes (3 bits each, bits 0..12);
    /// per class, thermometer of its count among the last `WINDOW` bytes
    /// at 1, 4 and 16 (12..36); one-hot class of the first non-blank byte
    /// of the current line (36..44); column thermometer at 1, 4, 16, 64
    /// (44..48); previous line shorter than 40 bytes (48); previous line
    /// ended with a full stop (49); inside brackets (50); inside `$...$` on
    /// this line (51). Bits 52..64 stay zero.
    ///
    /// Positional detail is kept short on purpose: a template is the shape
    /// of a stretch of text, so its signature should persist across it.
    #[must_use]
    pub fn signature(&self) -> Shape {
        let mut w0 = 0u64;
        for (i, &n) in self.bigrams.iter().enumerate() {
            if n > 0 {
                w0 |= 1 << i;
            }
        }
        let mut w1 = 0u64;
        for back in 0..4.min(self.len) {
            w1 |= u64::from(self.class_back(back)) << (3 * back);
        }
        for (c, &n) in self.counts.iter().enumerate() {
            for (k, edge) in [1u16, 4, 16].iter().enumerate() {
                if n >= *edge {
                    w1 |= 1 << (12 + 3 * c + k);
                }
            }
        }
        if let Some(c) = self.line_first {
            w1 |= 1 << (36 + u32::from(c));
        }
        for (k, edge) in [1u32, 4, 16, 64].iter().enumerate() {
            if self.column >= *edge {
                w1 |= 1 << (44 + k);
            }
        }
        if self.prev_line_len < 40 {
            w1 |= 1 << 48;
        }
        if self.prev_line_dot {
            w1 |= 1 << 49;
        }
        if self.open_brackets > 0 {
            w1 |= 1 << 50;
        }
        if self.dollars % 2 == 1 {
            w1 |= 1 << 51;
        }
        [w0, w1]
    }

    /// Appends one decoded byte.
    pub fn push(&mut self, b: u8) {
        let c = class(b);
        if self.len > 0 {
            let prev = self.class_back(0);
            self.bigrams[usize::from(prev) * CLASSES + usize::from(c)] += 1;
        }
        // The bigram leaving the window of the last WINDOW bytes once this
        // byte is in: bytes (len - WINDOW, len - WINDOW + 1).
        if self.len >= WINDOW {
            let a = self.class_back(WINDOW - 1);
            let z = self.class_back(WINDOW - 2);
            self.bigrams[usize::from(a) * CLASSES + usize::from(z)] -= 1;
            self.counts[usize::from(a)] -= 1;
        }
        self.counts[usize::from(c)] += 1;
        self.ring[self.len % (WINDOW + 1)] = c;
        self.len += 1;
        match b {
            b'\n' => {
                self.prev_line_len = self.column;
                self.prev_line_dot = self.last_byte == b'.';
                self.column = 0;
                self.line_first = None;
                self.dollars = 0;
            }
            _ => {
                self.column += 1;
                if self.line_first.is_none() && c != 3 {
                    self.line_first = Some(c);
                }
                match b {
                    b'(' | b'[' | b'{' => self.open_brackets += 1,
                    b')' | b']' | b'}' => self.open_brackets = (self.open_brackets - 1).max(0),
                    b'$' => self.dollars += 1,
                    _ => {}
                }
            }
        }
        if b != b'\n' && b != b'\r' && b != b' ' {
            self.last_byte = b;
        }
    }
}

/// Index of the centre at the smallest Hamming distance (ties low).
#[inline]
#[must_use]
pub fn nearest(centres: &[Shape], s: &Shape) -> usize {
    let mut best = (u32::MAX, 0);
    for (k, c) in centres.iter().enumerate() {
        let d: u32 = c.iter().zip(s).map(|(a, b)| (a ^ b).count_ones()).sum();
        if d < best.0 {
            best = (d, k);
        }
    }
    best.1
}

/// Flat templates: Hamming k-means over `samples`, `rounds` rounds of
/// assignment then bitwise-majority centres (an empty cluster keeps its
/// centre). The start is a sample picked by `seed`, then farthest points.
#[must_use]
pub fn cluster(samples: &[Shape], k: usize, seed: u64, rounds: usize) -> Vec<Shape> {
    assert!(k > 0 && !samples.is_empty());
    // Farthest-point start: one sample picked by `seed`, then repeatedly
    // the sample farthest from every centre so far (ties low).
    let distance =
        |a: &Shape, b: &Shape| -> u32 { a.iter().zip(b).map(|(x, y)| (x ^ y).count_ones()).sum() };
    let mut centres: Vec<Shape> = vec![samples[(splitmix(seed) % samples.len() as u64) as usize]];
    let mut gap: Vec<u32> = samples.iter().map(|s| distance(s, &centres[0])).collect();
    while centres.len() < k {
        let mut far = 0;
        for (i, &g) in gap.iter().enumerate() {
            if g > gap[far] {
                far = i;
            }
        }
        let c = samples[far];
        for (g, s) in gap.iter_mut().zip(samples) {
            *g = (*g).min(distance(s, &c));
        }
        centres.push(c);
    }
    for _ in 0..rounds {
        let mut ones = vec![[0u32; 64 * SHAPE_WORDS]; k];
        let mut count = vec![0u32; k];
        for s in samples {
            let j = nearest(&centres, s);
            count[j] += 1;
            for (w, &word) in s.iter().enumerate() {
                let mut rest = word;
                while rest != 0 {
                    ones[j][w * 64 + rest.trailing_zeros() as usize] += 1;
                    rest &= rest - 1;
                }
            }
        }
        for j in 0..k {
            if count[j] == 0 {
                continue;
            }
            let mut c = [0u64; SHAPE_WORDS];
            for (i, &n) in ones[j].iter().enumerate() {
                if 2 * n > count[j] {
                    c[i / 64] |= 1 << (i % 64);
                }
            }
            centres[j] = c;
        }
    }
    centres
}

/// Share of positions on which two labelings agree under the best
/// one-to-one matching of their labels (maximum-weight assignment on the
/// confusion matrix), so it does not depend on how either side numbers
/// its labels.
#[must_use]
pub fn agreement(a: &[u8], b: &[u8], k: usize) -> f64 {
    assert_eq!(a.len(), b.len());
    if a.is_empty() {
        return 1.0;
    }
    let mut confusion = vec![vec![0i64; k]; k];
    for (&x, &y) in a.iter().zip(b) {
        confusion[usize::from(x)][usize::from(y)] += 1;
    }
    max_assignment(&confusion) as f64 / a.len() as f64
}

/// Largest total weight of a one-to-one assignment of rows to columns of
/// a square matrix (Hungarian algorithm on the negated weights, O(k^3)).
#[must_use]
pub fn max_assignment(w: &[Vec<i64>]) -> i64 {
    let n = w.len();
    if n == 0 {
        return 0;
    }
    let cost = |i: usize, j: usize| -w[i - 1][j - 1];
    // Potentials and matching, 1-based with column 0 as the virtual start.
    let (mut u, mut v) = (vec![0i64; n + 1], vec![0i64; n + 1]);
    let (mut p, mut way) = (vec![0usize; n + 1], vec![0usize; n + 1]);
    for i in 1..=n {
        p[0] = i;
        let mut j0 = 0;
        let mut minv = vec![i64::MAX; n + 1];
        let mut used = vec![false; n + 1];
        loop {
            used[j0] = true;
            let (i0, mut delta, mut j1) = (p[j0], i64::MAX, 0);
            for j in 1..=n {
                if !used[j] {
                    let cur = cost(i0, j) - u[i0] - v[j];
                    if cur < minv[j] {
                        minv[j] = cur;
                        way[j] = j0;
                    }
                    if minv[j] < delta {
                        delta = minv[j];
                        j1 = j;
                    }
                }
            }
            for j in 0..=n {
                if used[j] {
                    u[p[j]] += delta;
                    v[j] -= delta;
                } else {
                    minv[j] -= delta;
                }
            }
            j0 = j1;
            if p[j0] == 0 {
                break;
            }
        }
        loop {
            let j1 = way[j0];
            p[j0] = p[j1];
            j0 = j1;
            if j0 == 0 {
                break;
            }
        }
    }
    (1..=n).map(|j| w[p[j] - 1][j - 1]).sum()
}

/// Bytes hashed to find a match.
pub const MATCH_MIN: usize = 12;
/// Hash table size of a match model over a whole corpus.
pub const MATCH_TABLE_BITS: u32 = 22;

/// LZP match predictor over one stream: after `min` bytes (`MATCH_MIN`
/// by default) it looks
/// up where that context last occurred and predicts the byte that
/// followed; a correct prediction extends the match, a wrong one drops it.
#[derive(Debug, Clone)]
pub struct MatchModel {
    history: Vec<u8>,
    table: Vec<u32>,
    pointer: Option<usize>,
    length: usize,
    bits: u32,
    min: usize,
}

impl Default for MatchModel {
    fn default() -> Self {
        Self::new()
    }
}

impl MatchModel {
    #[must_use]
    pub fn new() -> Self {
        Self::with_bits(MATCH_TABLE_BITS)
    }

    /// A model with a `2^bits` entry hash table (`bits` in 8..=30).
    #[must_use]
    pub fn with_bits(bits: u32) -> Self {
        Self::with_min(bits, MATCH_MIN)
    }

    /// A model that hashes the last `min` bytes (`min` >= 1).
    #[must_use]
    pub fn with_min(bits: u32, min: usize) -> Self {
        assert!((8..=30).contains(&bits) && min >= 1);
        Self {
            history: Vec::new(),
            table: vec![0; 1 << bits],
            pointer: None,
            length: 0,
            bits,
            min,
        }
    }

    /// Predicted next byte and the current match length, if any.
    #[must_use]
    pub fn predict(&self) -> Option<(u8, usize)> {
        self.pointer
            .filter(|&p| p < self.history.len())
            .map(|p| (self.history[p], self.length))
    }

    fn hash(&self) -> usize {
        let tail = &self.history[self.history.len() - self.min..];
        let mut h = 0u64;
        for &b in tail {
            h = splitmix(h ^ u64::from(b));
        }
        (h >> (64 - self.bits)) as usize
    }

    pub fn push(&mut self, b: u8) {
        match self.predict() {
            Some((p, _)) if p == b => {
                self.pointer = self.pointer.map(|q| q + 1);
                self.length += 1;
            }
            _ => {
                self.pointer = None;
                self.length = 0;
            }
        }
        self.history.push(b);
        if self.history.len() >= self.min {
            let h = self.hash();
            if self.pointer.is_none() && self.table[h] != 0 {
                self.pointer = Some(self.table[h] as usize);
            }
            self.table[h] = u32::try_from(self.history.len()).unwrap_or(u32::MAX);
        }
    }
}

/// Match length bucket: 0 (no match), 1 (< 16), 2 (< 64), 3 (>= 64).
#[inline]
#[must_use]
pub fn length_bucket(prediction: Option<(u8, usize)>) -> usize {
    match prediction {
        None => 0,
        Some((_, n)) if n < 16 => 1,
        Some((_, n)) if n < 64 => 2,
        Some(_) => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shapes(text: &[u8]) -> Vec<Shape> {
        let mut st = ShapeState::new();
        text.iter()
            .map(|&b| {
                let s = st.signature();
                st.push(b);
                s
            })
            .collect()
    }

    #[test]
    fn classes_cover_every_byte() {
        let mut seen = [false; CLASSES];
        for b in 0..=255u8 {
            seen[usize::from(class(b))] = true;
        }
        assert!(seen.iter().all(|&s| s));
        assert_eq!(class(b'q'), 0);
        assert_eq!(class(b'Q'), 1);
        assert_eq!(class(b'7'), 2);
        assert_eq!(class(b'['), 5);
        assert_eq!(class(b'\\'), 6);
    }

    #[test]
    fn the_signature_is_causal_and_the_window_slides() {
        let text = b"Results are shown in [12].\nTable 3: 0.51 & 0.73 \\\\\n$x^2$ holds.";
        let a = shapes(text);
        // Changing a later byte never changes an earlier signature.
        let mut other = text.to_vec();
        other[40] = b'Z';
        let b = shapes(&other);
        assert_eq!(a[..=40], b[..=40]);
        assert_ne!(a[41..], b[41..]);
        // The bigram counts equal a recount of the last WINDOW bytes.
        let long: Vec<u8> = (0..500u32).map(|i| b"ab1 [x]\n$"[i as usize % 9]).collect();
        let mut st = ShapeState::new();
        for (i, &b) in long.iter().enumerate() {
            st.push(b);
            let start = (i + 1).saturating_sub(WINDOW);
            let mut want = [0u16; CLASSES * CLASSES];
            for w in long[start..=i].windows(2) {
                want[usize::from(class(w[0])) * CLASSES + usize::from(class(w[1]))] += 1;
            }
            assert_eq!(st.bigrams, want, "at {i}");
            let mut classes = [0u16; CLASSES];
            for &b in &long[(i + 1).saturating_sub(WINDOW)..=i] {
                classes[usize::from(class(b))] += 1;
            }
            assert_eq!(st.counts, classes, "at {i}");
        }
    }

    #[test]
    fn layout_bits_follow_the_text() {
        let mut st = ShapeState::new();
        for &b in b"Intro.\n  [3] ($a" {
            st.push(b);
        }
        let s = st.signature()[1];
        assert_ne!(s & (1 << 48), 0, "short previous line");
        assert_ne!(s & (1 << 49), 0, "previous line ended with a dot");
        assert_ne!(s & (1 << 50), 0, "inside brackets");
        assert_ne!(s & (1 << 51), 0, "inside $");
        assert_ne!(s & (1 << (36 + 5)), 0, "line starts with a bracket");
        assert_eq!(s >> 52, 0);
    }

    #[test]
    fn clustering_separates_two_templates_and_agreement_ignores_labels() {
        let prose = b"the model is trained on the corpus and the results are good. ".repeat(40);
        let refs = b"[12] A. Smith, J. Doe (2019) 4(2):11-19.\n".repeat(40);
        let mut samples = shapes(&prose);
        samples.extend(shapes(&refs));
        let centres = cluster(&samples, 2, 11, 6);
        let labels: Vec<u8> = samples.iter().map(|s| nearest(&centres, s) as u8).collect();
        let truth: Vec<u8> = (0..samples.len())
            .map(|i| u8::from(i >= prose.len()))
            .collect();
        let got = agreement(&labels, &truth, 2);
        assert!(got > 0.9, "agreement {got}");
        let swapped: Vec<u8> = labels.iter().map(|&l| 1 - l).collect();
        assert!((agreement(&labels, &swapped, 2) - 1.0).abs() < 1e-12);
        assert_eq!(cluster(&samples, 2, 11, 6), centres);
    }

    #[test]
    fn agreement_is_the_best_matching_whatever_the_label_numbers() {
        // Tied cells: a greedy match depends on label order, the best
        // matching does not (Codex review of #724).
        let rows = [
            [10u64, 10, 0, 0],
            [10, 0, 0, 0],
            [0, 0, 25, 0],
            [0, 0, 0, 25],
        ];
        let (mut a, mut b) = (Vec::new(), Vec::new());
        for (x, row) in rows.iter().enumerate() {
            for (y, &n) in row.iter().enumerate() {
                for _ in 0..n {
                    a.push(x as u8);
                    b.push(y as u8);
                }
            }
        }
        let best = agreement(&a, &b, 4);
        assert!((best - 70.0 / 80.0).abs() < 1e-12, "{best}");
        for perm in [[1u8, 0, 2, 3], [3, 2, 1, 0], [2, 0, 3, 1]] {
            let pa: Vec<u8> = a.iter().map(|&x| perm[usize::from(x)]).collect();
            let pb: Vec<u8> = b.iter().map(|&y| perm[usize::from(3 - y)]).collect();
            assert!((agreement(&pa, &b, 4) - best).abs() < 1e-12);
            assert!((agreement(&a, &pb, 4) - best).abs() < 1e-12);
        }
        // Against brute force on small random matrices.
        let mut z = 7u64;
        for _ in 0..200 {
            let w: Vec<Vec<i64>> = (0..4)
                .map(|_| {
                    (0..4)
                        .map(|_| {
                            z = splitmix(z);
                            (z % 9) as i64
                        })
                        .collect()
                })
                .collect();
            let mut brute = 0;
            for p in 0..24usize {
                let mut cols: Vec<usize> = (0..4).collect();
                let mut perm = Vec::new();
                let mut q = p;
                for left in (1..=4).rev() {
                    perm.push(cols.remove(q % left));
                    q /= left;
                }
                brute = brute.max((0..4).map(|i| w[i][perm[i]]).sum::<i64>());
            }
            assert_eq!(max_assignment(&w), brute);
        }
    }

    #[test]
    fn the_match_model_predicts_repeats_only() {
        let block = b"Licensed under CC-BY 4.0. All rights reserved by the authors.";
        let mut m = MatchModel::new();
        let mut hits = 0;
        let mut text = block.to_vec();
        text.extend_from_slice(b" xyz ");
        text.extend_from_slice(block);
        for (i, &b) in text.iter().enumerate() {
            if let Some((p, _)) = m.predict() {
                assert!(i > block.len(), "no prediction before a repeat");
                hits += usize::from(p == b);
            }
            m.push(b);
        }
        assert!(hits >= block.len() - MATCH_MIN - 1);
        assert_eq!(length_bucket(None), 0);
        assert_eq!(length_bucket(Some((0, 70))), 3);
    }
}
