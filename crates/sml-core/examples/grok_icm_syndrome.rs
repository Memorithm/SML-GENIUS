//! GROK-ICM-0: incremental syndrome page guard versus full-scan oracle.
//!
//! Research reference only. See `docs/SML-GROK-ICM-0-PREREGISTRATION.md`.
//! No runtime integration, no topological or physical claim.

use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq, Eq)]
enum GuardError {
    IndexOutOfRange,
    LocalCheckMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Check {
    Clean,
    Detected,
    Corrected(Vec<u64>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Recovered,
    Corrected,
    Erasure,
    Silent,
}

trait PageGuard: Clone {
    const NAME: &'static str;
    fn init(page: &[u64]) -> Self;
    /// Incremental update; must not mutate `self` on error.
    fn apply_write(&mut self, idx: usize, old: u64, new: u64) -> Result<(), GuardError>;
    fn check(&self, page: &[u64]) -> Check;
    fn verify_full(&self, page: &[u64]) -> bool {
        self.check(page) == Check::Clean
    }
    fn syndrome(&self) -> Vec<u8>;
    fn guard_bits(&self) -> usize;
    fn flip_guard_bit(&mut self, bit: usize);
}

// ---------------------------------------------------------------- Parity
#[derive(Clone, Debug, PartialEq, Eq)]
struct ParityGuard {
    words: usize,
    parity: u8,
}

impl PageGuard for ParityGuard {
    const NAME: &'static str = "PARITY";
    fn init(page: &[u64]) -> Self {
        let ones: u32 = page.iter().map(|w| w.count_ones()).sum();
        Self {
            words: page.len(),
            parity: (ones & 1) as u8,
        }
    }
    fn apply_write(&mut self, idx: usize, old: u64, new: u64) -> Result<(), GuardError> {
        if idx >= self.words {
            return Err(GuardError::IndexOutOfRange);
        }
        self.parity ^= ((old ^ new).count_ones() & 1) as u8;
        Ok(())
    }
    fn check(&self, page: &[u64]) -> Check {
        if Self::init(page).parity == self.parity {
            Check::Clean
        } else {
            Check::Detected
        }
    }
    fn syndrome(&self) -> Vec<u8> {
        vec![self.parity]
    }
    fn guard_bits(&self) -> usize {
        1
    }
    fn flip_guard_bit(&mut self, _bit: usize) {
        self.parity ^= 1;
    }
}

// ---------------------------------------------------------------- CRC-32/MPEG-2
const POLY: u32 = 0x04C1_1DB7;

fn crc_byte(mut crc: u32, byte: u8) -> u32 {
    crc ^= u32::from(byte) << 24;
    for _ in 0..8 {
        crc = if crc & 0x8000_0000 != 0 {
            (crc << 1) ^ POLY
        } else {
            crc << 1
        };
    }
    crc
}

fn crc_bytes(init: u32, bytes: &[u8]) -> u32 {
    bytes.iter().fold(init, |c, &b| crc_byte(c, b))
}

/// Linear part (init 0): M(x) * x^32 mod P for one 64-bit word.
fn crc_lin_word(w: u64) -> u32 {
    crc_bytes(0, &w.to_be_bytes())
}

/// a(x) * b(x) mod P(x), P = x^32 + POLY.
fn mulmod(a: u32, b: u32) -> u32 {
    let mut prod: u64 = 0;
    for i in 0..32 {
        if (b >> i) & 1 == 1 {
            prod ^= u64::from(a) << i;
        }
    }
    let full: u64 = (1u64 << 32) | u64::from(POLY);
    for i in (32..64).rev() {
        if (prod >> i) & 1 == 1 {
            prod ^= full << (i - 32);
        }
    }
    prod as u32
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Crc32Guard {
    crc: u32,
    /// shift[idx] = x^(64*(n-1-idx)) mod P, built once in O(n).
    shift: Vec<u32>,
}

fn crc_full(page: &[u64]) -> u32 {
    page.iter()
        .fold(0xFFFF_FFFF, |c, w| crc_bytes(c, &w.to_be_bytes()))
}

impl PageGuard for Crc32Guard {
    const NAME: &'static str = "CRC32";
    fn init(page: &[u64]) -> Self {
        let n = page.len();
        let mut x64 = 1u32;
        for _ in 0..64 {
            // multiply by x
            x64 = if x64 & 0x8000_0000 != 0 {
                (x64 << 1) ^ POLY
            } else {
                x64 << 1
            };
        }
        let mut shift = vec![1u32; n];
        for i in (0..n.saturating_sub(1)).rev() {
            shift[i] = mulmod(shift[i + 1], x64);
        }
        Self {
            crc: crc_full(page),
            shift,
        }
    }
    fn apply_write(&mut self, idx: usize, old: u64, new: u64) -> Result<(), GuardError> {
        let s = *self.shift.get(idx).ok_or(GuardError::IndexOutOfRange)?;
        self.crc ^= mulmod(crc_lin_word(old ^ new), s);
        Ok(())
    }
    fn check(&self, page: &[u64]) -> Check {
        if page.len() == self.shift.len() && crc_full(page) == self.crc {
            Check::Clean
        } else {
            Check::Detected
        }
    }
    fn syndrome(&self) -> Vec<u8> {
        self.crc.to_be_bytes().to_vec()
    }
    fn guard_bits(&self) -> usize {
        32
    }
    fn flip_guard_bit(&mut self, bit: usize) {
        self.crc ^= 1 << bit;
    }
}

// ---------------------------------------------------------------- SECDED (72,64)
/// Hamming positions 1..=71 that are not powers of two, in increasing order.
fn data_positions() -> [u8; 64] {
    let mut out = [0u8; 64];
    let mut k = 0;
    for p in 1u8..=71 {
        if !p.is_power_of_two() {
            out[k] = p;
            k += 1;
        }
    }
    assert_eq!(k, 64);
    out
}

/// Check byte: bits 0..7 Hamming checks, bit 7 overall parity of data + checks.
fn secded_encode(w: u64) -> u8 {
    let pos = data_positions();
    let mut s = 0u8;
    for (i, &p) in pos.iter().enumerate() {
        if (w >> i) & 1 == 1 {
            s ^= p;
        }
    }
    let overall = (w.count_ones() + u32::from(s).count_ones()) & 1;
    s | ((overall as u8) << 7)
}

enum WordDecode {
    Ok,
    Fixed(u64),
    CheckBitFixed,
    Double,
}

fn secded_decode(w: u64, c: u8) -> WordDecode {
    let e = secded_encode(w);
    let syn = (e ^ c) & 0x7F;
    let overall_bad = ((w.count_ones() + u32::from(c).count_ones()) & 1) == 1;
    match (syn, overall_bad) {
        (0, false) => WordDecode::Ok,
        (0, true) => WordDecode::CheckBitFixed, // overall parity bit itself
        (_, false) => WordDecode::Double,
        (s, true) => {
            if s.is_power_of_two() {
                WordDecode::CheckBitFixed
            } else if let Some(i) = data_positions().iter().position(|&p| p == s) {
                WordDecode::Fixed(w ^ (1u64 << i))
            } else {
                WordDecode::Double // syndrome > 71: uncorrectable
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Secded72_64Guard {
    checks: Vec<u8>,
}

impl PageGuard for Secded72_64Guard {
    const NAME: &'static str = "SECDED72_64";
    fn init(page: &[u64]) -> Self {
        Self {
            checks: page.iter().map(|&w| secded_encode(w)).collect(),
        }
    }
    fn apply_write(&mut self, idx: usize, old: u64, new: u64) -> Result<(), GuardError> {
        let c = self
            .checks
            .get_mut(idx)
            .ok_or(GuardError::IndexOutOfRange)?;
        if secded_encode(old) != *c {
            return Err(GuardError::LocalCheckMismatch);
        }
        *c = secded_encode(new);
        Ok(())
    }
    fn check(&self, page: &[u64]) -> Check {
        if page.len() != self.checks.len() {
            return Check::Detected;
        }
        let mut fixed = page.to_vec();
        let mut any = false;
        for (i, (&w, &c)) in page.iter().zip(&self.checks).enumerate() {
            match secded_decode(w, c) {
                WordDecode::Ok => {}
                WordDecode::Fixed(v) => {
                    fixed[i] = v;
                    any = true;
                }
                WordDecode::CheckBitFixed => any = true,
                WordDecode::Double => return Check::Detected,
            }
        }
        if any {
            Check::Corrected(fixed)
        } else {
            Check::Clean
        }
    }
    fn syndrome(&self) -> Vec<u8> {
        self.checks.clone()
    }
    fn guard_bits(&self) -> usize {
        8 * self.checks.len()
    }
    fn flip_guard_bit(&mut self, bit: usize) {
        self.checks[bit / 8] ^= 1 << (bit % 8);
    }
}

// ---------------------------------------------------------------- guarded page
#[derive(Clone, Debug, PartialEq, Eq)]
struct GuardedPage<G: PageGuard> {
    page: Vec<u64>,
    guard: G,
}

impl<G: PageGuard> GuardedPage<G> {
    fn new(page: Vec<u64>) -> Self {
        let guard = G::init(&page);
        Self { page, guard }
    }
    /// Atomic: on error neither page nor guard changes.
    fn write(&mut self, idx: usize, new: u64) -> Result<(), GuardError> {
        let old = *self.page.get(idx).ok_or(GuardError::IndexOutOfRange)?;
        self.guard.apply_write(idx, old, new)?;
        self.page[idx] = new;
        Ok(())
    }
}

// ---------------------------------------------------------------- experiment
struct SplitMix64(u64);
impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

fn random_page(seed: u64, words: usize) -> Vec<u64> {
    let mut r = SplitMix64(seed);
    (0..words).map(|_| r.next()).collect()
}

/// Returns number of writes whose incremental syndrome matched the full scan.
fn delta_panel<G: PageGuard>(words: usize, writes: usize, seed: u64) -> usize {
    let mut r = SplitMix64(seed);
    let mut gp = GuardedPage::<G>::new(random_page(seed ^ 0xA5, words));
    let mut ok = 0;
    for _ in 0..writes {
        let idx = (r.next() % words as u64) as usize;
        let val = r.next();
        gp.write(idx, val)
            .expect("in-range write on consistent page");
        if gp.guard.syndrome() == G::init(&gp.page).syndrome() {
            ok += 1;
        }
    }
    ok
}

fn flip(page: &mut [u64], guard: &mut impl PageGuard, bit: usize) {
    let data_bits = page.len() * 64;
    if bit < data_bits {
        page[bit / 64] ^= 1 << (bit % 64);
    } else {
        guard.flip_guard_bit(bit - data_bits);
    }
}

fn classify<G: PageGuard>(orig: &[u64], page: &[u64], guard: &G) -> Outcome {
    match guard.check(page) {
        Check::Clean if page == orig => Outcome::Recovered,
        Check::Clean => Outcome::Silent,
        Check::Detected => Outcome::Erasure,
        Check::Corrected(d) if d == orig => Outcome::Corrected,
        Check::Corrected(_) => Outcome::Silent,
    }
}

#[derive(Default, Debug, PartialEq, Eq, Clone, Copy)]
struct Tally {
    trials: u64,
    recovered: u64,
    corrected: u64,
    erasure: u64,
    silent: u64,
}

impl Tally {
    fn add(&mut self, o: Outcome) {
        self.trials += 1;
        match o {
            Outcome::Recovered => self.recovered += 1,
            Outcome::Corrected => self.corrected += 1,
            Outcome::Erasure => self.erasure += 1,
            Outcome::Silent => self.silent += 1,
        }
    }
}

fn fault_panel<G: PageGuard>(orig: &[u64], two_bit: bool) -> Tally {
    let g0 = G::init(orig);
    let total = orig.len() * 64 + g0.guard_bits();
    let mut t = Tally::default();
    for a in 0..total {
        if two_bit {
            for b in (a + 1)..total {
                let (mut p, mut g) = (orig.to_vec(), g0.clone());
                flip(&mut p, &mut g, a);
                flip(&mut p, &mut g, b);
                t.add(classify(orig, &p, &g));
            }
        } else {
            let (mut p, mut g) = (orig.to_vec(), g0.clone());
            flip(&mut p, &mut g, a);
            t.add(classify(orig, &p, &g));
        }
    }
    t
}

const SMALL_PAGE: [u64; 1] = [0x0123_4567_89AB_CDEF];
const WRITES: usize = 10_000;

fn arm_report<G: PageGuard>(out: &mut String) {
    let big = random_page(0x1C41, 64);
    let d64 = delta_panel::<G>(1, WRITES, 0x1C40);
    let d4096 = delta_panel::<G>(64, WRITES, 0x1C40);
    let f1s = fault_panel::<G>(&SMALL_PAGE, false);
    let f2s = fault_panel::<G>(&SMALL_PAGE, true);
    let f1b = fault_panel::<G>(&big, false);
    let clean = G::init(&big).verify_full(&big);
    let _ = writeln!(
        out,
        "{}: delta64 {d64}/{WRITES} delta4096 {d4096}/{WRITES} verify_full_clean {clean}",
        G::NAME
    );
    for (name, t) in [("1bit@64", f1s), ("2bit@64", f2s), ("1bit@4096", f1b)] {
        let _ = writeln!(
            out,
            "  {name}: trials {} recovered {} corrected {} erasure {} silent {}",
            t.trials, t.recovered, t.corrected, t.erasure, t.silent
        );
    }
}

fn report() -> String {
    let mut s = String::from("GROK-ICM-0 report v1\n");
    arm_report::<ParityGuard>(&mut s);
    arm_report::<Crc32Guard>(&mut s);
    arm_report::<Secded72_64Guard>(&mut s);
    s
}

fn fnv1a64(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn main() {
    let r = report();
    print!("{r}");
    println!("fnv1a64 {:016x}", fnv1a64(&r));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_mpeg2_check_value() {
        assert_eq!(crc_bytes(0xFFFF_FFFF, b"123456789"), 0x0376_E6E7);
    }

    fn delta_exact<G: PageGuard>() {
        assert_eq!(
            delta_panel::<G>(1, WRITES, 0x1C40),
            WRITES,
            "{} 64-bit",
            G::NAME
        );
        assert_eq!(
            delta_panel::<G>(64, WRITES, 0x1C40),
            WRITES,
            "{} 4096-bit",
            G::NAME
        );
    }

    #[test]
    fn incremental_equals_full_scan_all_arms() {
        delta_exact::<ParityGuard>();
        delta_exact::<Crc32Guard>();
        delta_exact::<Secded72_64Guard>();
    }

    #[test]
    fn parity_fault_classes() {
        let t1 = fault_panel::<ParityGuard>(&SMALL_PAGE, false);
        assert_eq!((t1.trials, t1.erasure), (65, 65));
        let t2 = fault_panel::<ParityGuard>(&SMALL_PAGE, true);
        assert_eq!((t2.trials, t2.silent), (65 * 64 / 2, 65 * 64 / 2));
    }

    #[test]
    fn crc32_detects_all_1_and_2_bit_faults() {
        let t1 = fault_panel::<Crc32Guard>(&SMALL_PAGE, false);
        assert_eq!((t1.trials, t1.erasure), (96, 96));
        let t2 = fault_panel::<Crc32Guard>(&SMALL_PAGE, true);
        assert_eq!((t2.trials, t2.erasure), (96 * 95 / 2, 96 * 95 / 2));
        let tb = fault_panel::<Crc32Guard>(&random_page(0x1C41, 64), false);
        assert_eq!((tb.trials, tb.erasure), (4128, 4128));
    }

    #[test]
    fn secded_corrects_1_detects_2() {
        let t1 = fault_panel::<Secded72_64Guard>(&SMALL_PAGE, false);
        assert_eq!((t1.trials, t1.corrected), (72, 72));
        let t2 = fault_panel::<Secded72_64Guard>(&SMALL_PAGE, true);
        assert_eq!((t2.trials, t2.erasure), (72 * 71 / 2, 72 * 71 / 2));
        let tb = fault_panel::<Secded72_64Guard>(&random_page(0x1C41, 64), false);
        assert_eq!((tb.trials, tb.corrected), (4608, 4608));
    }

    fn atomic_oob<G: PageGuard + std::fmt::Debug + PartialEq>() {
        let mut gp = GuardedPage::<G>::new(random_page(7, 64));
        let before = gp.clone();
        assert_eq!(gp.write(64, 1), Err(GuardError::IndexOutOfRange));
        assert_eq!(gp, before);
        assert_eq!(
            gp.guard.apply_write(64, 0, 1),
            Err(GuardError::IndexOutOfRange)
        );
        assert_eq!(gp, before);
    }

    #[test]
    fn rejects_are_atomic() {
        atomic_oob::<ParityGuard>();
        atomic_oob::<Crc32Guard>();
        atomic_oob::<Secded72_64Guard>();
        let mut gp = GuardedPage::<Secded72_64Guard>::new(random_page(7, 64));
        gp.page[3] ^= 1 << 5; // corrupt data behind the guard
        let before = gp.clone();
        assert_eq!(gp.write(3, 42), Err(GuardError::LocalCheckMismatch));
        assert_eq!(gp, before);
    }

    #[test]
    fn verify_full_accepts_clean_pages() {
        let p = random_page(9, 64);
        assert!(ParityGuard::init(&p).verify_full(&p));
        assert!(Crc32Guard::init(&p).verify_full(&p));
        assert!(Secded72_64Guard::init(&p).verify_full(&p));
    }

    #[test]
    fn report_is_byte_identical_on_replay() {
        assert_eq!(report(), report());
    }
}
