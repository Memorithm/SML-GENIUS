//! GROK-ICM-1: local parity codes vs SECDED/CRC/REP3 at equal redundancy.
//! See `docs/SML-GROK-ICM-1-EQUAL-REDUNDANCY-PREREGISTRATION.md`.

use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Dec {
    Clean,
    Detected,
    Corrected(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Code {
    Secded,
    Crc8,
    Interleave8,
    Product8x8,
    Crc16,
    Rep3,
}

const ALL: [Code; 6] = [
    Code::Secded,
    Code::Crc8,
    Code::Interleave8,
    Code::Product8x8,
    Code::Crc16,
    Code::Rep3,
];

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

fn secded_encode(w: u64) -> u8 {
    let mut s = 0u8;
    for (i, &p) in data_positions().iter().enumerate() {
        if (w >> i) & 1 == 1 {
            s ^= p;
        }
    }
    let overall = (w.count_ones() + u32::from(s).count_ones()) & 1;
    s | ((overall as u8) << 7)
}

fn crc_generic(w: u64, poly: u32, width: u32) -> u32 {
    let top = 1u32 << (width - 1);
    let mask = if width == 32 {
        u32::MAX
    } else {
        (1 << width) - 1
    };
    let mut crc = 0u32;
    for b in w.to_be_bytes() {
        crc ^= u32::from(b) << (width - 8);
        for _ in 0..8 {
            crc = if crc & top != 0 {
                (crc << 1) ^ poly
            } else {
                crc << 1
            } & mask;
        }
    }
    crc
}

fn lane_parity(w: u64) -> u8 {
    let mut c = 0u8;
    for i in 0..64 {
        if (w >> i) & 1 == 1 {
            c ^= 1 << (i % 8);
        }
    }
    c
}

/// rows in low byte (row r = data byte r), columns in high byte.
fn product(w: u64) -> u16 {
    let mut rows = 0u16;
    let mut cols = 0u16;
    for i in 0..64 {
        if (w >> i) & 1 == 1 {
            rows ^= 1 << (i / 8);
            cols ^= 1 << (i % 8);
        }
    }
    rows | (cols << 8)
}

impl Code {
    fn name(self) -> &'static str {
        match self {
            Code::Secded => "SECDED72_64",
            Code::Crc8 => "CRC8",
            Code::Interleave8 => "INTERLEAVE8",
            Code::Product8x8 => "PRODUCT8X8",
            Code::Crc16 => "CRC16",
            Code::Rep3 => "REP3",
        }
    }
    fn check_bits(self) -> usize {
        match self {
            Code::Secded | Code::Crc8 | Code::Interleave8 => 8,
            Code::Product8x8 | Code::Crc16 => 16,
            Code::Rep3 => 128,
        }
    }
    fn encode(self, w: u64) -> u128 {
        match self {
            Code::Secded => u128::from(secded_encode(w)),
            Code::Crc8 => u128::from(crc_generic(w, 0x07, 8)),
            Code::Interleave8 => u128::from(lane_parity(w)),
            Code::Product8x8 => u128::from(product(w)),
            Code::Crc16 => u128::from(crc_generic(w, 0x1021, 16)),
            Code::Rep3 => u128::from(w) | (u128::from(w) << 64),
        }
    }
    fn decode(self, w: u64, c: u128) -> Dec {
        let e = self.encode(w);
        if e == c {
            return Dec::Clean;
        }
        match self {
            Code::Crc8 | Code::Crc16 | Code::Interleave8 => Dec::Detected,
            Code::Secded => {
                let c = c as u8;
                let syn = (e as u8 ^ c) & 0x7F;
                let odd = ((w.count_ones() + u32::from(c).count_ones()) & 1) == 1;
                match (syn, odd) {
                    (0, true) => Dec::Corrected(w),
                    (_, false) => Dec::Detected,
                    (s, true) if s.is_power_of_two() => Dec::Corrected(w),
                    (s, true) => match data_positions().iter().position(|&p| p == s) {
                        Some(i) => Dec::Corrected(w ^ (1 << i)),
                        None => Dec::Detected,
                    },
                }
            }
            Code::Product8x8 => {
                let d = (e ^ c) as u16;
                let (r, k) = (d & 0xFF, d >> 8);
                match (r.count_ones(), k.count_ones()) {
                    (1, 1) => {
                        let i = r.trailing_zeros() * 8 + k.trailing_zeros();
                        Dec::Corrected(w ^ (1 << i))
                    }
                    (1, 0) | (0, 1) => Dec::Corrected(w),
                    _ => Dec::Detected,
                }
            }
            Code::Rep3 => {
                let (a, b) = (c as u64, (c >> 64) as u64);
                Dec::Corrected((w & a) | (w & b) | (a & b))
            }
        }
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
struct Tally {
    trials: u64,
    recovered: u64,
    corrected: u64,
    erasure: u64,
    silent: u64,
}

fn outcome(code: Code, orig: u64, flips: &[usize], t: &mut Tally) {
    let (mut w, mut c) = (orig, code.encode(orig));
    for &b in flips {
        if b < 64 {
            w ^= 1 << b;
        } else {
            c ^= 1 << (b - 64);
        }
    }
    t.trials += 1;
    match code.decode(w, c) {
        Dec::Clean if w == orig => t.recovered += 1,
        Dec::Clean => t.silent += 1,
        Dec::Detected => t.erasure += 1,
        Dec::Corrected(d) if d == orig => t.corrected += 1,
        Dec::Corrected(_) => t.silent += 1,
    }
}

const WORDS: [u64; 2] = [0x0123_4567_89AB_CDEF, 0];

fn panel(code: Code, kind: &str) -> Tally {
    let n = 64 + code.check_bits();
    let mut t = Tally::default();
    for &w in &WORDS {
        match kind {
            "1bit" => (0..n).for_each(|a| outcome(code, w, &[a], &mut t)),
            "2bit" => {
                for a in 0..n {
                    for b in a + 1..n {
                        outcome(code, w, &[a, b], &mut t);
                    }
                }
            }
            "3bit" => {
                for a in 0..n {
                    for b in a + 1..n {
                        for c in b + 1..n {
                            outcome(code, w, &[a, b, c], &mut t);
                        }
                    }
                }
            }
            _ => {
                for len in 2..=8 {
                    for s in 0..=(n - len) {
                        let f: Vec<usize> = (s..s + len).collect();
                        outcome(code, w, &f, &mut t);
                    }
                }
            }
        }
    }
    t
}

const KINDS: [&str; 4] = ["1bit", "2bit", "3bit", "burst2-8"];

fn verdict(tier: &[Code], arm: Code) -> &'static str {
    let s = |c: Code, k| panel(c, k).silent;
    if s(arm, "1bit") + s(arm, "2bit") + s(arm, "burst2-8") != 0 {
        return "NEGATIVE";
    }
    let best = tier
        .iter()
        .filter(|&&c| c != Code::Interleave8 && c != Code::Product8x8)
        .map(|&c| s(c, "3bit"))
        .min()
        .unwrap_or(u64::MAX);
    if s(arm, "3bit") <= best {
        "PROMOTED"
    } else {
        "NEGATIVE"
    }
}

fn report() -> String {
    let mut out = String::from("GROK-ICM-1 report v1\n");
    for code in ALL {
        let _ = writeln!(out, "{} check_bits {}", code.name(), code.check_bits());
        for k in KINDS {
            let t = panel(code, k);
            let _ = writeln!(
                out,
                "  {k}: trials {} recovered {} corrected {} erasure {} silent {}",
                t.trials, t.recovered, t.corrected, t.erasure, t.silent
            );
        }
    }
    let a = [Code::Secded, Code::Crc8, Code::Interleave8];
    let b = [Code::Product8x8, Code::Crc16];
    let _ = writeln!(
        out,
        "verdict INTERLEAVE8 {}",
        verdict(&a, Code::Interleave8)
    );
    let _ = writeln!(out, "verdict PRODUCT8X8 {}", verdict(&b, Code::Product8x8));
    out
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
    fn clean_words_decode_clean() {
        for c in ALL {
            for w in WORDS {
                assert_eq!(c.decode(w, c.encode(w)), Dec::Clean, "{}", c.name());
            }
        }
    }

    #[test]
    fn crc_check_values() {
        // CRC-16/XMODEM of 8 zero bytes is 0; non-trivial word must differ.
        assert_eq!(crc_generic(0, 0x1021, 16), 0);
        assert_ne!(crc_generic(WORDS[0], 0x1021, 16), 0);
    }

    #[test]
    fn frozen_mathematical_expectations() {
        assert_eq!(panel(Code::Secded, "1bit").silent, 0);
        assert_eq!(panel(Code::Secded, "2bit").silent, 0);
        assert!(panel(Code::Secded, "3bit").silent > 0);
        assert!(panel(Code::Interleave8, "2bit").silent > 0);
        assert_eq!(panel(Code::Product8x8, "1bit").silent, 0);
        // Frozen expectation FALSIFIED by execution (see doc): row-check + column-check
        // pairs are miscorrected into a data flip. Assert the observed negative.
        assert_eq!(panel(Code::Product8x8, "2bit").silent, 384);
        assert_eq!(
            panel(Code::Rep3, "1bit").corrected,
            panel(Code::Rep3, "1bit").trials
        );
    }

    #[test]
    fn report_replays() {
        assert_eq!(report(), report());
    }
}
