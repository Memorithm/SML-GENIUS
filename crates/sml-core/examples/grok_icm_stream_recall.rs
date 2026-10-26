//! GROK-ICM-3: guarded shift memory vs naked Shift75 vs alternating Rule90.
//! See `docs/SML-GROK-ICM-3-STREAM-RECALL-PREREGISTRATION.md`.

#[path = "support/grok_icm_guard.rs"]
mod icm;

use icm::{fnv1a64, Check, Crc32Guard, PageGuard, SecdedGuard, SplitMix64};
use std::fmt::Write as _;

const N: u32 = 75;
const MASK: u128 = (1u128 << N) - 1;
const STEPS: usize = 4096;
const WARM: usize = 150;
const TEST: std::ops::Range<u64> = 2000..2004;

fn input_bit(r: &mut SplitMix64) -> u128 {
    u128::from((r.next_u64() >> 32) & 1)
}

fn rule90(s: u128) -> u128 {
    let left = ((s << 1) | (s >> (N - 1))) & MASK; // s_{i-1} at i
    let right = ((s >> 1) | ((s & 1) << (N - 1))) & MASK; // s_{i+1} at i
    left ^ right
}

/// Inverse of the 75x75 impulse matrix for phase p: rows give u_j = parity(row & s).
fn rule90_inverse(phase: usize) -> Vec<u128> {
    let mut cols = Vec::new();
    for j in 0..N as usize {
        let site = (phase + j) % 2;
        let mut c = 1u128 << site;
        for _ in 0..j {
            c = rule90(c);
        }
        cols.push(c);
    }
    // rows of M: row_i bit j = bit i of col_j
    let mut m: Vec<u128> = (0..N as usize)
        .map(|i| {
            cols.iter()
                .enumerate()
                .fold(0u128, |acc, (j, c)| acc | (((c >> i) & 1) << j))
        })
        .collect();
    let mut inv: Vec<u128> = (0..N as usize).map(|i| 1u128 << i).collect();
    for col in 0..N as usize {
        let piv = (col..N as usize)
            .find(|&r| (m[r] >> col) & 1 == 1)
            .expect("rank 75 (independent check of SKY-R3 claim)");
        m.swap(col, piv);
        inv.swap(col, piv);
        for r in 0..N as usize {
            if r != col && (m[r] >> col) & 1 == 1 {
                m[r] ^= m[col];
                inv[r] ^= inv[col];
            }
        }
    }
    // M^{-1} rows: u = M^{-1} s, so u_j = parity(inv_row_j & s) where inv is
    // expressed over original rows (bits of s).
    inv
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
struct T {
    correct: u64,
    erasure: u64,
    wrong: u64,
}

impl T {
    fn add(&mut self, o: Option<bool>) {
        match o {
            Some(true) => self.correct += 1,
            Some(false) => self.wrong += 1,
            None => self.erasure += 1,
        }
    }
}

fn words(s: u128) -> [u64; 2] {
    [s as u64, (s >> 64) as u64]
}

fn flip(r: &mut SplitMix64, k: u32) -> u128 {
    let mut m = 0u128;
    while m.count_ones() < k {
        m |= 1u128 << r.below(u64::from(N));
    }
    m
}

/// Returns tallies for [SHIFT75, SHIFT75+ICM, ALT_RULE90].
fn run(seed: u64, k: u32) -> [T; 3] {
    let inv = [rule90_inverse(0), rule90_inverse(1)];
    let mut rin = SplitMix64(seed);
    let mut rf = SplitMix64(seed ^ 0xFA17);
    let mut hist: Vec<u128> = Vec::with_capacity(STEPS);
    let (mut naked, mut guarded, mut r90) = (0u128, 0u128, 0u128);
    let mut valid: u32 = N;
    let mut sec = SecdedGuard::init(&words(0));
    let mut crc = Crc32Guard::init(&words(0));
    let mut out = [T::default(); 3];
    for t in 0..STEPS {
        // Scrub the guarded arm before the step.
        let w = words(guarded);
        let fixed = match sec.check(&w) {
            Check::Clean => Some(w.to_vec()),
            Check::Corrected(f) => Some(f),
            Check::Detected => None,
        };
        match fixed {
            Some(f) if crc.check(&f) == Check::Clean => {
                guarded = u128::from(f[0]) | (u128::from(f[1]) << 64);
            }
            _ => {
                guarded = 0;
                valid = 0;
            }
        }
        let x = input_bit(&mut rin);
        hist.push(x);
        naked = ((naked << 1) | x) & MASK;
        guarded = ((guarded << 1) | x) & MASK;
        valid = (valid + 1).min(N);
        r90 = rule90(r90) ^ (x << (t % 2));
        // guard recomputed after the shift (full recompute, see prereg)
        sec = SecdedGuard::init(&words(guarded));
        crc = Crc32Guard::init(&words(guarded));
        if t >= WARM {
            let p = t % 2;
            for d in 1..N as usize {
                let truth = hist[t - d];
                out[0].add(Some((naked >> d) & 1 == truth));
                out[1].add(if (d as u32) < valid {
                    Some((guarded >> d) & 1 == truth)
                } else {
                    None
                });
                let u = u128::from((inv[p][d] & r90).count_ones() & 1);
                out[2].add(Some(u == truth));
            }
        }
        // Faults at rest, after the guard was computed.
        if k > 0 && rf.below(32) == 0 {
            let m = flip(&mut rf, k);
            naked ^= m;
            guarded ^= m;
            r90 ^= m;
        }
    }
    out
}

fn panel(k: u32) -> [T; 3] {
    let mut acc = [T::default(); 3];
    for s in TEST {
        let r = run(s, k);
        for i in 0..3 {
            acc[i].correct += r[i].correct;
            acc[i].erasure += r[i].erasure;
            acc[i].wrong += r[i].wrong;
        }
    }
    acc
}

const NAMES: [&str; 3] = ["SHIFT75", "SHIFT75+ICM", "ALT_RULE90"];

fn promoted(p: &[T; 3]) -> bool {
    let g = p[1];
    let total = g.correct + g.erasure + g.wrong;
    g.wrong == 0 && g.erasure * 20 <= total && g.wrong < p[0].wrong && g.wrong < p[2].wrong
}

fn report() -> String {
    let mut out = String::from("GROK-ICM-3 report v1\n");
    let mut verdict = true;
    for k in [0, 1, 2] {
        let p = panel(k);
        for (n, t) in NAMES.iter().zip(p) {
            let _ = writeln!(
                out,
                "k{k} {n}: correct {} erasure {} wrong {}",
                t.correct, t.erasure, t.wrong
            );
        }
        if k > 0 {
            verdict &= promoted(&p);
        }
    }
    let _ = writeln!(
        out,
        "verdict SHIFT75+ICM {}",
        if verdict { "PROMOTED" } else { "NEGATIVE" }
    );
    out
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
    fn both_phases_full_rank_and_block_decoder_exact_from_zero() {
        let inv = [rule90_inverse(0), rule90_inverse(1)];
        let mut r = SplitMix64(7);
        let mut s = 0u128;
        let mut h = Vec::new();
        for t in 0..N as usize {
            let x = input_bit(&mut r);
            h.push(x);
            s = rule90(s) ^ (x << (t % 2));
        }
        let t = N as usize - 1;
        for d in 0..N as usize {
            let u = u128::from((inv[t % 2][d] & s).count_ones() & 1);
            assert_eq!(u, h[t - d], "age {d}");
        }
    }

    #[test]
    fn no_fault_shift_arms_perfect() {
        let p = panel(0);
        assert_eq!(p[0].wrong + p[0].erasure, 0);
        assert_eq!(p[1].wrong + p[1].erasure, 0);
    }

    #[test]
    fn guarded_shift_has_zero_silent_errors_under_faults() {
        for k in [1, 2] {
            assert_eq!(panel(k)[1].wrong, 0, "k={k}");
        }
    }

    #[test]
    fn report_replays() {
        assert_eq!(report(), report());
    }
}
