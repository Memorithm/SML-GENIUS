//! GROK-ICM guarded pages (feature `icm`, default off).
//!
//! Each 64-bit word carries a SECDED(72,64) extended-Hamming check byte and
//! the whole page carries a CRC-32/MPEG-2. [`GuardedPage::scrub`] repairs one
//! flipped bit per word and refuses the page when the repaired content does
//! not match the CRC, so SECDED miscorrections are never accepted silently.
//! Evidence: `docs/SML-GROK-ICM-2-REAL-PAGES-PREREGISTRATION.md` and
//! `docs/SML-GROK-ICM-4-FEATURE-INTEGRATION-PREREGISTRATION.md`.

const POLY: u32 = 0x04C1_1DB7;

fn crc32(words: &[u64]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;
    for w in words {
        for byte in w.to_be_bytes() {
            crc ^= u32::from(byte) << 24;
            for _ in 0..8 {
                crc = if crc & 0x8000_0000 != 0 {
                    (crc << 1) ^ POLY
                } else {
                    crc << 1
                };
            }
        }
    }
    crc
}

const fn data_positions() -> [u8; 64] {
    let mut out = [0u8; 64];
    let mut k = 0;
    let mut p = 1u8;
    while p <= 71 {
        if !p.is_power_of_two() {
            out[k] = p;
            k += 1;
        }
        p += 1;
    }
    out
}

const POSITIONS: [u8; 64] = data_positions();

fn secded_encode(w: u64) -> u8 {
    let mut s = 0u8;
    for (i, &p) in POSITIONS.iter().enumerate() {
        if (w >> i) & 1 == 1 {
            s ^= p;
        }
    }
    let overall = (w.count_ones() + u32::from(s).count_ones()) & 1;
    s | ((overall as u8) << 7)
}

/// `None` = uncorrectable; `Some(word)` = clean or corrected data.
fn secded_decode(w: u64, c: u8) -> Option<u64> {
    let syn = (secded_encode(w) ^ c) & 0x7F;
    let odd = ((w.count_ones() + u32::from(c).count_ones()) & 1) == 1;
    match (syn, odd) {
        (0, _) => Some(w),
        (_, false) => None,
        (s, true) if s.is_power_of_two() => Some(w),
        (s, true) => POSITIONS.iter().position(|&p| p == s).map(|i| w ^ (1 << i)),
    }
}

/// Result of scrubbing a [`GuardedPage`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrubOutcome {
    /// Page matched its guard.
    Clean,
    /// This many words were repaired and the CRC confirmed the repair.
    Corrected(usize),
    /// Damage could not be repaired safely; the page was left untouched.
    Uncorrectable,
}

/// A page of 64-bit words protected by per-word SECDED plus a page CRC-32.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardedPage {
    words: Vec<u64>,
    checks: Vec<u8>,
    crc: u32,
}

impl GuardedPage {
    /// Guard `words` as they are now.
    #[must_use]
    pub fn new(words: Vec<u64>) -> Self {
        let checks = words.iter().map(|&w| secded_encode(w)).collect();
        let crc = crc32(&words);
        Self { words, checks, crc }
    }

    /// Current (possibly damaged) words.
    #[must_use]
    pub fn words(&self) -> &[u64] {
        &self.words
    }

    /// Mutable access to the raw words, for fault injection in tests and
    /// benches. Writes through this do not update the guard.
    pub fn words_mut_unguarded(&mut self) -> &mut [u64] {
        &mut self.words
    }

    /// Verify and, when safe, repair the page in place.
    pub fn scrub(&mut self) -> ScrubOutcome {
        let mut fixed = self.words.clone();
        let mut repaired = 0;
        for (w, &c) in fixed.iter_mut().zip(&self.checks) {
            if secded_encode(*w) == c {
                continue;
            }
            match secded_decode(*w, c) {
                None => return ScrubOutcome::Uncorrectable,
                Some(v) => {
                    if v != *w {
                        repaired += 1;
                    }
                    *w = v;
                }
            }
        }
        if crc32(&fixed) != self.crc {
            return ScrubOutcome::Uncorrectable;
        }
        if repaired == 0 && fixed == self.words {
            return ScrubOutcome::Clean;
        }
        self.words = fixed;
        ScrubOutcome::Corrected(repaired)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> Vec<u64> {
        (0..64_u64)
            .map(|i| i.wrapping_mul(0x9e37_79b9_7f4a_7c15))
            .collect()
    }

    #[test]
    fn any_change_to_page_changes_crc_for_small_flips() {
        let clean = page();
        let base = crc32(&clean);
        for bit in 0..64 * 64 {
            let mut p = clean.clone();
            p[bit / 64] ^= 1 << (bit % 64);
            assert_ne!(crc32(&p), base);
        }
    }

    #[test]
    fn clean_page_is_clean() {
        assert_eq!(GuardedPage::new(page()).scrub(), ScrubOutcome::Clean);
    }

    #[test]
    fn every_single_bit_flip_is_repaired() {
        let clean = page();
        for bit in 0..64 * 64 {
            let mut g = GuardedPage::new(clean.clone());
            g.words_mut_unguarded()[bit / 64] ^= 1 << (bit % 64);
            assert_eq!(g.scrub(), ScrubOutcome::Corrected(1));
            assert_eq!(g.words(), &clean[..]);
        }
    }

    #[test]
    fn double_flip_in_one_word_is_refused_untouched() {
        let mut g = GuardedPage::new(page());
        g.words_mut_unguarded()[3] ^= 0b101;
        let damaged = g.words().to_vec();
        assert_eq!(g.scrub(), ScrubOutcome::Uncorrectable);
        assert_eq!(g.words(), &damaged[..]);
    }
}
