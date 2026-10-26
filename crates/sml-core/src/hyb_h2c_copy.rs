//! HYB H-2c: a copy expert mixed after the H-1d head.
//!
//! A per-document LZP match model (`MatchModel`) predicts the byte that
//! followed the last occurrence of the current context. While the bits
//! already coded of the current byte agree with that prediction, the
//! prediction gives an expected bit for the next decision, and the mixer
//! moves the head's stretch towards (or away from) it by a learned shift.
//! The shift depends on the match length bucket and the decision depth
//! only, never on the head, and it is cut to the room the head stretch
//! leaves before the coder's limit, the same towards either bit. The code
//! length is convex over that range, so a shift towards a bit drawn at
//! random costs bits on average: a null signal cannot recalibrate the head.
//! Everything it reads is known to a decoder before the decision.
//! Research only. NOT_PROMOTED.

use crate::{
    gb11_p1_stream::STRETCH_LIMIT,
    hyb_h1_residual::{decision_cost, splitmix, DEPTHS},
};

/// Match length buckets: 0, 1..=3, 4..=15, 16..=63, 64..=255, 256+.
pub const COPY_BUCKETS: usize = 6;
/// One shift per bucket and depth.
pub const COPY_CELLS: usize = COPY_BUCKETS * DEPTHS;
/// Aligned stretches lie in the coder's range.
const SPAN: i32 = STRETCH_LIMIT;
const BINS: usize = 2 * SPAN as usize + 1;
/// Candidate shifts, in stretch units.
pub const SHIFT_MIN: i32 = -1024;
pub const SHIFT_MAX: i32 = STRETCH_LIMIT;
pub const SHIFT_STEP: i32 = 8;
/// A cell seen fewer times than this keeps a zero shift.
pub const MIN_CELL_DECISIONS: u64 = 1000;

/// Bucket of a match that has been confirmed `length` times.
#[inline]
#[must_use]
pub fn copy_bucket(length: usize) -> usize {
    match length {
        0 => 0,
        1..=3 => 1,
        4..=15 => 2,
        16..=63 => 3,
        64..=255 => 4,
        _ => 5,
    }
}

/// Expected bit of decision `depth` when the copy prediction `predicted`
/// still agrees with the bits of `target` above it, else `None`.
#[inline]
#[must_use]
pub fn expected_bit(predicted: u8, target: u8, depth: usize) -> Option<bool> {
    let above = 8 - depth as u32;
    (u32::from(predicted) >> above == u32::from(target) >> above)
        .then_some((predicted >> (7 - depth)) & 1 != 0)
}

#[inline]
fn cell(bucket: usize, depth: usize) -> usize {
    bucket * DEPTHS + depth
}

/// The head stretch seen from the copy prediction, clamped to the coder's
/// range: positive when the head leans towards the expected bit.
#[inline]
fn aligned(stretch: i32, expected: bool) -> i32 {
    let s = stretch.clamp(-STRETCH_LIMIT, STRETCH_LIMIT);
    if expected {
        s
    } else {
        -s
    }
}

/// Shift `w` cut to the room left on either side of aligned stretch `a`,
/// so that neither direction reaches the clamp.
#[inline]
fn room(a: i32, w: i32) -> i32 {
    let room = STRETCH_LIMIT - a.abs();
    w.clamp(-room, room)
}

/// Learned shifts of the copy mixer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyMixer {
    pub shifts: [i32; COPY_CELLS],
}

impl CopyMixer {
    #[must_use]
    pub fn zero() -> Self {
        Self {
            shifts: [0; COPY_CELLS],
        }
    }

    /// Stretch of decision `depth` after the copy expert.
    #[inline]
    #[must_use]
    pub fn stretch(&self, stretch: i32, expected: bool, bucket: usize, depth: usize) -> i32 {
        let a = aligned(stretch, expected);
        let w = room(a, self.shifts[cell(bucket, depth)]);
        if expected {
            a + w
        } else {
            -(a + w)
        }
    }

    #[must_use]
    pub fn text(&self) -> String {
        self.shifts
            .iter()
            .map(i32::to_string)
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// Histograms of aligned stretch and outcome per cell, from which the
/// best shift of each cell is found exactly.
#[derive(Debug, Clone)]
pub struct CopyFit {
    hist: Vec<Vec<u64>>,
    /// Decisions seen and decisions where the expected bit was right.
    pub seen: [[u64; 2]; COPY_BUCKETS],
}

impl Default for CopyFit {
    fn default() -> Self {
        Self::new()
    }
}

impl CopyFit {
    #[must_use]
    pub fn new() -> Self {
        Self {
            hist: vec![Vec::new(); COPY_CELLS],
            seen: [[0; 2]; COPY_BUCKETS],
        }
    }

    /// Records one decision with an expected bit.
    pub fn add(&mut self, stretch: i32, expected: bool, bit: bool, bucket: usize, depth: usize) {
        let a = aligned(stretch, expected);
        let h = &mut self.hist[cell(bucket, depth)];
        if h.is_empty() {
            h.resize(2 * BINS, 0);
        }
        let right = bit == expected;
        h[usize::from(right) * BINS + (a.clamp(-SPAN, SPAN) + SPAN) as usize] += 1;
        self.seen[bucket][0] += 1;
        self.seen[bucket][1] += u64::from(right);
    }

    /// Per cell, the candidate shift with the lowest code length on the
    /// recorded decisions; ties go to the shift closest to zero, and a
    /// cell with fewer than `MIN_CELL_DECISIONS` decisions keeps zero.
    #[must_use]
    pub fn fit(&self) -> CopyMixer {
        // decision_cost clamps its stretch, so a table over the clamped
        // range gives the same code lengths.
        let table: Vec<[f64; 2]> = (-STRETCH_LIMIT..=STRETCH_LIMIT)
            .map(|s| [decision_cost(s, false), decision_cost(s, true)])
            .collect();
        let lookup = |s: i32, right: bool| {
            table[(s.clamp(-STRETCH_LIMIT, STRETCH_LIMIT) + STRETCH_LIMIT) as usize]
                [usize::from(right)]
        };
        let mut shifts = [0; COPY_CELLS];
        for (c, h) in self.hist.iter().enumerate() {
            let filled: Vec<(i32, bool, f64)> = h
                .iter()
                .enumerate()
                .filter(|(_, &n)| n > 0)
                .map(|(i, &n)| ((i % BINS) as i32 - SPAN, i >= BINS, n as f64))
                .collect();
            if filled.iter().map(|&(_, _, n)| n).sum::<f64>() < MIN_CELL_DECISIONS as f64 {
                continue;
            }
            let cost = |w: i32| -> f64 {
                filled
                    .iter()
                    .map(|&(a, right, n)| n * lookup(a + room(a, w), right))
                    .sum()
            };
            let mut best = (cost(0), 0i32);
            let mut w = SHIFT_MIN;
            while w <= SHIFT_MAX {
                let bits = cost(w);
                if bits < best.0 || (bits == best.0 && w.abs() < best.1.abs()) {
                    best = (bits, w);
                }
                w += SHIFT_STEP;
            }
            shifts[c] = best.1;
        }
        CopyMixer { shifts }
    }
}

/// Control: an LZP match model whose matches are looked up in another
/// document (`donor`), never in the document being coded. Its predictions
/// look like real copies (real words, real match lengths) but carry no
/// repetition of the current document.
#[derive(Debug, Clone)]
pub struct DonorModel {
    donor: Vec<u8>,
    table: Vec<u32>,
    bits: u32,
    min: usize,
    context: Vec<u8>,
    pointer: Option<usize>,
    length: usize,
}

fn context_hash(tail: &[u8], bits: u32) -> usize {
    let mut h = 0u64;
    for &b in tail {
        h = splitmix(h ^ u64::from(b));
    }
    (h >> (64 - bits)) as usize
}

impl DonorModel {
    /// Indexes every `min`-byte context of `donor` (latest occurrence wins).
    #[must_use]
    pub fn new(donor: Vec<u8>, bits: u32, min: usize) -> Self {
        assert!((8..=30).contains(&bits) && min >= 1);
        let mut table = vec![0u32; 1 << bits];
        for end in min..=donor.len() {
            table[context_hash(&donor[end - min..end], bits)] =
                u32::try_from(end).unwrap_or(u32::MAX);
        }
        Self {
            donor,
            table,
            bits,
            min,
            context: Vec::new(),
            pointer: None,
            length: 0,
        }
    }

    /// Predicted next byte and the current match length, if any.
    #[must_use]
    pub fn predict(&self) -> Option<(u8, usize)> {
        self.pointer
            .filter(|&p| p < self.donor.len())
            .map(|p| (self.donor[p], self.length))
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
        self.context.push(b);
        if self.context.len() > self.min {
            self.context.remove(0);
        }
        if self.pointer.is_none() && self.context.len() == self.min {
            let at = self.table[context_hash(&self.context, self.bits)];
            if at != 0 {
                self.pointer = Some(at as usize);
            }
        }
    }
}

impl CopyFit {
    /// One shift for every cell: the candidate with the lowest code length
    /// over all recorded decisions (control for the 48 learned shifts).
    #[must_use]
    pub fn fit_single(&self) -> CopyMixer {
        let mut merged = Self::new();
        merged.hist[0] = vec![0; 2 * BINS];
        for h in self.hist.iter().filter(|h| !h.is_empty()) {
            for (m, &n) in merged.hist[0].iter_mut().zip(h) {
                *m += n;
            }
        }
        let w = merged.fit().shifts[0];
        CopyMixer {
            shifts: [w; COPY_CELLS],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hyb_h2t_templates::MatchModel;

    #[test]
    fn buckets_and_expected_bits() {
        assert_eq!(
            [0, 1, 3, 4, 15, 16, 63, 64, 255, 256].map(copy_bucket),
            [0, 1, 1, 2, 2, 3, 3, 4, 4, 5]
        );
        // 0b1010_0000 predicted, target 0b1011_0000: alive down to depth 3.
        let (q, t) = (0b1010_0000, 0b1011_0000);
        let got: Vec<_> = (0..DEPTHS).map(|d| expected_bit(q, t, d)).collect();
        assert_eq!(
            got,
            [
                Some(true),
                Some(false),
                Some(true),
                Some(false),
                None,
                None,
                None,
                None
            ]
        );
        for b in 0..=255u8 {
            for d in 0..DEPTHS {
                assert_eq!(expected_bit(b, b, d), Some((b >> (7 - d)) & 1 != 0));
            }
        }
    }

    /// A reliable copy signal learns a large positive shift, a random one
    /// learns none, and the mixer is symmetric in the expected bit.
    #[test]
    fn the_fit_trusts_reliable_copies_only() {
        let mut fit = CopyFit::new();
        let mut state = 7u64;
        for i in 0..20_000u64 {
            state = crate::hyb_h1_residual::splitmix(state ^ i);
            let expected = state & 1 != 0;
            // Bucket 5: right 99 %; bucket 0: right 50 %.
            fit.add(0, expected, expected ^ (state % 100 == 0), 5, 3);
            fit.add(0, expected, (state >> 7) & 1 != 0, 0, 3);
        }
        let mixer = fit.fit();
        let strong = mixer.shifts[cell(5, 3)];
        assert!(strong > 1000, "{strong}");
        assert!(mixer.shifts[cell(0, 3)].abs() <= 16);
        // The shift does not depend on which way the head leans, and it
        // is cut to the room left before the clamp, alike on both sides.
        let cut = strong.min(STRETCH_LIMIT - 100);
        assert_eq!(mixer.stretch(-100, true, 5, 3), -100 + cut);
        assert_eq!(mixer.stretch(100, true, 5, 3), 100 + cut);
        assert_eq!(mixer.stretch(100, false, 5, 3), 100 - cut);
        let near = STRETCH_LIMIT - 40;
        assert_eq!(mixer.stretch(near, true, 5, 3), STRETCH_LIMIT);
        assert_eq!(mixer.stretch(near, false, 5, 3), near - 40);
        assert_eq!(mixer.stretch(-near, true, 5, 3), -near + 40);
        assert!(fit.seen[5][1] > 19_700 && fit.seen[0][0] == 20_000);
        assert_eq!(CopyMixer::zero().stretch(5, true, 2, 1), 5);
    }

    /// A random expected bit learns no shift, even when the head is badly
    /// overconfident: without a cell per head direction, the mixer cannot
    /// recalibrate the head.
    #[test]
    fn a_random_signal_cannot_recalibrate_an_overconfident_head() {
        // The head is right 70 % of the time at stretch ±1500.
        let draw = |state: u64| {
            let bit = state & 1 != 0;
            let right = (state >> 8 & 0xffff) % 10 < 7;
            let stretch = if bit == right { 1500 } else { -1500 };
            (stretch, state >> 63 != 0, bit)
        };
        let mut fit = CopyFit::new();
        for i in 0..200_000u64 {
            let (stretch, expected, bit) = draw(splitmix(11 ^ i));
            fit.add(stretch, expected, bit, 2, 4);
        }
        let mixer = fit.fit();
        // Fitted on noise, the shift is small, and on fresh decisions it
        // gains nothing beyond sampling noise (about 0.001 bit a decision
        // here), where recalibrating this head would gain about 1.6.
        assert!(
            mixer.shifts[cell(2, 4)].abs() <= 160,
            "{}",
            mixer.shifts[cell(2, 4)]
        );
        let (mut plain, mut mixed) = (0.0, 0.0);
        for i in 0..200_000u64 {
            let (stretch, expected, bit) = draw(splitmix(0xfeed ^ i));
            plain += decision_cost(stretch, bit);
            mixed += decision_cost(mixer.stretch(stretch, expected, 2, 4), bit);
        }
        assert!(plain - mixed < 0.003 * 200_000.0, "{}", plain - mixed);
    }

    #[test]
    fn the_donor_predicts_from_the_other_document_only() {
        let donor = b"alpha beta gamma delta epsilon zeta eta theta".to_vec();
        let mut model = DonorModel::new(donor, 12, 4);
        let mut hits = 0;
        // Text that repeats itself but shares only `gamma delta` with the
        // donor: only that part can be predicted.
        for &b in b"xyz gamma delta qqq xyz gamma qqq xyz" {
            if model.predict().is_some_and(|(q, _)| q == b) {
                hits += 1;
            }
            model.push(b);
        }
        assert!((3..=12).contains(&hits), "{hits}");
        let mut empty = DonorModel::new(Vec::new(), 8, 4);
        for &b in b"abcdabcdabcd" {
            assert_eq!(empty.predict(), None);
            empty.push(b);
        }
    }

    #[test]
    fn a_single_shift_is_shared_by_every_cell() {
        let mut fit = CopyFit::new();
        for i in 0..5000u64 {
            let e = i % 2 == 0;
            fit.add(0, e, e ^ (i % 10 == 0), (i % 6) as usize, (i % 8) as usize);
        }
        let single = fit.fit_single();
        assert!(single.shifts.iter().all(|&w| w == single.shifts[0]));
        assert!(single.shifts[0] > 400, "{}", single.shifts[0]);
    }

    #[test]
    fn a_copied_paragraph_is_predicted_with_long_matches() {
        let text = b"The quick brown fox jumps over the lazy dog near the river bank. ";
        let mut doc = Vec::new();
        doc.extend_from_slice(text);
        doc.extend_from_slice(b"Something else entirely. ");
        doc.extend_from_slice(text);
        let mut model = MatchModel::with_min(16, 6);
        let mut hits = 0;
        let mut last = None;
        for &b in &doc {
            if let Some((q, n)) = model.predict() {
                hits += usize::from(q == b);
                last = Some(n);
            }
            model.push(b);
        }
        assert!(hits >= text.len() - 8, "{hits}");
        assert!(last.unwrap() >= 50);
    }
}
