//! Shared GROK-ICM guards (same algorithms as ICM-0). Included with `#[path]`;
//! not a Cargo example target.
#![allow(dead_code)]

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    Clean,
    Detected,
    Corrected(Vec<u64>),
}

pub trait PageGuard: Clone {
    const NAME: &'static str;
    fn init(page: &[u64]) -> Self;
    fn apply_write(&mut self, idx: usize, old: u64, new: u64);
    fn check(&self, page: &[u64]) -> Check;
}

#[derive(Clone, Debug)]
pub struct NoneGuard;
impl PageGuard for NoneGuard {
    const NAME: &'static str = "NONE";
    fn init(_: &[u64]) -> Self {
        NoneGuard
    }
    fn apply_write(&mut self, _: usize, _: u64, _: u64) {}
    fn check(&self, _: &[u64]) -> Check {
        Check::Clean
    }
}

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
fn crc_word(init: u32, w: u64) -> u32 {
    w.to_be_bytes().iter().fold(init, |c, &b| crc_byte(c, b))
}
fn mulmod(a: u32, b: u32) -> u32 {
    let mut p: u64 = 0;
    for i in 0..32 {
        if (b >> i) & 1 == 1 {
            p ^= u64::from(a) << i;
        }
    }
    let full = (1u64 << 32) | u64::from(POLY);
    for i in (32..64).rev() {
        if (p >> i) & 1 == 1 {
            p ^= full << (i - 32);
        }
    }
    p as u32
}
pub fn crc_full(page: &[u64]) -> u32 {
    page.iter().fold(0xFFFF_FFFF, |c, &w| crc_word(c, w))
}

#[derive(Clone, Debug)]
pub struct Crc32Guard {
    crc: u32,
    shift: Vec<u32>,
}
impl PageGuard for Crc32Guard {
    const NAME: &'static str = "CRC32";
    fn init(page: &[u64]) -> Self {
        let mut x64 = 1u32;
        for _ in 0..64 {
            x64 = if x64 & 0x8000_0000 != 0 {
                (x64 << 1) ^ POLY
            } else {
                x64 << 1
            };
        }
        let n = page.len();
        let mut shift = vec![1u32; n];
        for i in (0..n.saturating_sub(1)).rev() {
            shift[i] = mulmod(shift[i + 1], x64);
        }
        Self {
            crc: crc_full(page),
            shift,
        }
    }
    fn apply_write(&mut self, idx: usize, old: u64, new: u64) {
        self.crc ^= mulmod(crc_word(0, old ^ new), self.shift[idx]);
    }
    fn check(&self, page: &[u64]) -> Check {
        if crc_full(page) == self.crc {
            Check::Clean
        } else {
            Check::Detected
        }
    }
}

fn data_positions() -> [u8; 64] {
    let mut out = [0u8; 64];
    let mut k = 0;
    for p in 1u8..=71 {
        if !p.is_power_of_two() {
            out[k] = p;
            k += 1;
        }
    }
    out
}
pub fn secded_encode(w: u64) -> u8 {
    let mut s = 0u8;
    for (i, &p) in data_positions().iter().enumerate() {
        if (w >> i) & 1 == 1 {
            s ^= p;
        }
    }
    let overall = (w.count_ones() + u32::from(s).count_ones()) & 1;
    s | ((overall as u8) << 7)
}
/// None = double/uncorrectable; Some(word) = clean or corrected data.
pub fn secded_decode(w: u64, c: u8) -> Option<u64> {
    let syn = (secded_encode(w) ^ c) & 0x7F;
    let odd = ((w.count_ones() + u32::from(c).count_ones()) & 1) == 1;
    match (syn, odd) {
        (0, _) => Some(w),
        (_, false) => None,
        (s, true) if s.is_power_of_two() => Some(w),
        (s, true) => data_positions()
            .iter()
            .position(|&p| p == s)
            .map(|i| w ^ (1 << i)),
    }
}

#[derive(Clone, Debug)]
pub struct SecdedGuard {
    pub checks: Vec<u8>,
}
impl PageGuard for SecdedGuard {
    const NAME: &'static str = "SECDED72_64";
    fn init(page: &[u64]) -> Self {
        Self {
            checks: page.iter().map(|&w| secded_encode(w)).collect(),
        }
    }
    fn apply_write(&mut self, idx: usize, _old: u64, new: u64) {
        self.checks[idx] = secded_encode(new);
    }
    fn check(&self, page: &[u64]) -> Check {
        let mut fixed = page.to_vec();
        let mut any = false;
        for (i, (&w, &c)) in page.iter().zip(&self.checks).enumerate() {
            if secded_encode(w) == c {
                continue;
            }
            match secded_decode(w, c) {
                None => return Check::Detected,
                Some(v) => {
                    fixed[i] = v;
                    any = true;
                }
            }
        }
        if any {
            Check::Corrected(fixed)
        } else {
            Check::Clean
        }
    }
}

pub struct SplitMix64(pub u64);
impl SplitMix64 {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
}

pub fn fnv1a64(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Flip `k` distinct random bits of `w`.
pub fn fault(rng: &mut SplitMix64, w: u64, k: u32) -> u64 {
    let mut m = 0u64;
    while m.count_ones() < k {
        m |= 1 << rng.below(64);
    }
    w ^ m
}
