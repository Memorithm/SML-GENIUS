//! GROK-ICM-4 end-to-end bench: `sml_core::icm_guard` protecting real PVP6
//! gate coefficient pages. Requires `--features icm`.
//! See `docs/SML-GROK-ICM-4-FEATURE-INTEGRATION-PREREGISTRATION.md`.

use sml_core::icm_guard::{GuardedPage, ScrubOutcome};
use sml_core::pascal_vector_projection::{Pvp6PopulationProfile, Pvp6Workload};
use sml_core::pvp6_executor::{Pvp6Bank, Pvp6ExecutionPath, Pvp6Gate};
use std::fmt::Write as _;
use std::time::Instant;

const BITS: usize = 4096;
const GATES: usize = 32;
const DENSITY: usize = 8;
const TRIALS: u64 = 64;
const SEED0: u64 = 4000;

struct SplitMix64(u64);
impl SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
}

fn fnv1a64(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn random_gate(r: &mut SplitMix64) -> Pvp6Gate<BITS> {
    let mut m: Vec<u16> = Vec::new();
    while m.len() < DENSITY {
        let v = r.below(BITS as u64) as u16;
        if !m.contains(&v) {
            m.push(v);
        }
    }
    Pvp6Gate::from_monomials(&m).expect("gate")
}

fn monomials(page: &[u64]) -> Vec<u16> {
    let mut out = Vec::new();
    for (i, &w) in page.iter().enumerate() {
        for b in 0..64 {
            if (w >> b) & 1 == 1 {
                out.push((i * 64 + b) as u16);
            }
        }
    }
    out
}

/// Evaluate pages (None = refused) on all queries; returns per-gate outputs.
fn eval(pages: &[Option<Vec<u64>>]) -> Vec<Option<Vec<bool>>> {
    let queries: Vec<usize> = (0..BITS).collect();
    let mut idx = Vec::new();
    let mut gates: Vec<Pvp6Gate<BITS>> = Vec::new();
    let mut maxd = 1;
    for (g, p) in pages.iter().enumerate() {
        if let Some(p) = p {
            let m = monomials(p);
            if m.is_empty() {
                continue; // constant zero, handled below
            }
            maxd = maxd.max(m.len());
            gates.push(Pvp6Gate::from_monomials(&m).expect("gate"));
            idx.push(g);
        }
    }
    let mut out: Vec<Option<Vec<bool>>> = pages
        .iter()
        .map(|p| p.as_ref().map(|_| vec![false; BITS]))
        .collect();
    if gates.is_empty() {
        return out;
    }
    let bank = Pvp6Bank::new(gates).expect("bank");
    let res = bank
        .evaluate_bounded_on(
            Pvp6ExecutionPath::Selected,
            Pvp6Workload::new(
                maxd as u32,
                BITS as u32,
                1,
                Pvp6PopulationProfile::AffineUniform,
            ),
            &queries,
        )
        .expect("evaluate");
    for (bi, &g) in idx.iter().enumerate() {
        let v = out[g].as_mut().expect("accepted");
        for (q, slot) in v.iter_mut().enumerate() {
            *slot = res.value(bi, q).expect("value");
        }
    }
    out
}

#[derive(Default, Clone, Copy)]
struct Tally {
    correct: u64,
    erasure: u64,
    silent: u64,
    bit_loss: u64,
}

fn score(t: &mut Tally, got: &[Option<Vec<bool>>], want: &[Vec<bool>]) {
    for (g, w) in got.iter().zip(want) {
        match g {
            None => t.erasure += 1,
            Some(g) => {
                let wrong = g.iter().zip(w).filter(|(a, b)| a != b).count() as u64;
                if wrong == 0 {
                    t.correct += 1;
                } else {
                    t.silent += 1;
                    t.bit_loss += wrong;
                }
            }
        }
    }
}

/// Returns (NONE, ICM) tallies for flip count k.
fn panel(k: u32) -> (Tally, Tally) {
    let (mut none, mut icm) = (Tally::default(), Tally::default());
    for trial in 0..TRIALS {
        let mut r = SplitMix64(SEED0 + trial);
        let clean: Vec<Vec<u64>> = (0..GATES)
            .map(|_| random_gate(&mut r).coefficients().to_vec())
            .collect();
        let want: Vec<Vec<bool>> = eval(&clean.iter().cloned().map(Some).collect::<Vec<_>>())
            .into_iter()
            .map(|o| o.expect("clean"))
            .collect();
        let mut guarded: Vec<GuardedPage> = clean.iter().cloned().map(GuardedPage::new).collect();
        for g in &mut guarded {
            let mut flipped = Vec::new();
            while flipped.len() < k as usize {
                let b = r.below((BITS) as u64) as usize;
                if !flipped.contains(&b) {
                    flipped.push(b);
                }
            }
            for b in flipped {
                g.words_mut_unguarded()[b / 64] ^= 1 << (b % 64);
            }
        }
        let naked: Vec<Option<Vec<u64>>> =
            guarded.iter().map(|g| Some(g.words().to_vec())).collect();
        score(&mut none, &eval(&naked), &want);
        let scrubbed: Vec<Option<Vec<u64>>> = guarded
            .iter_mut()
            .map(|g| match g.scrub() {
                ScrubOutcome::Uncorrectable => None,
                _ => Some(g.words().to_vec()),
            })
            .collect();
        score(&mut icm, &eval(&scrubbed), &want);
    }
    (none, icm)
}

fn report() -> (String, bool) {
    let mut out = String::from("GROK-ICM-4 report v1\n");
    let mut pass = true;
    for k in 0..=3 {
        let (n, i) = panel(k);
        for (name, t) in [("NONE", n), ("ICM", i)] {
            let _ = writeln!(
                out,
                "k{k} {name}: correct {} erasure {} silent {} bit_loss {}",
                t.correct, t.erasure, t.silent, t.bit_loss
            );
        }
        let total = TRIALS * GATES as u64;
        if k == 0 {
            pass &= n.correct == total && i.correct == total;
        } else {
            pass &= i.silent == 0 && i.bit_loss == 0 && n.silent > 0;
            if k == 1 {
                pass &= i.correct == total;
            }
        }
    }
    let _ = writeln!(out, "verdict {}", if pass { "PASS" } else { "FAIL" });
    (out, pass)
}

fn median_ns<F: FnMut()>(mut f: F) -> u128 {
    let mut v: Vec<u128> = (0..31)
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed().as_nanos()
        })
        .collect();
    v.sort_unstable();
    v[15]
}

fn main() {
    let (r, _) = report();
    print!("{r}");
    println!("fnv1a64 {:016x}", fnv1a64(&r));
    let mut rng = SplitMix64(SEED0);
    let clean: Vec<Vec<u64>> = (0..GATES)
        .map(|_| random_gate(&mut rng).coefficients().to_vec())
        .collect();
    let pages: Vec<Option<Vec<u64>>> = clean.iter().cloned().map(Some).collect();
    let base = median_ns(|| {
        std::hint::black_box(eval(&pages));
    });
    let mut guarded: Vec<GuardedPage> = clean.iter().cloned().map(GuardedPage::new).collect();
    let with = median_ns(|| {
        let p: Vec<Option<Vec<u64>>> = guarded
            .iter_mut()
            .map(|g| match g.scrub() {
                ScrubOutcome::Uncorrectable => None,
                _ => Some(g.words().to_vec()),
            })
            .collect();
        std::hint::black_box(eval(&p));
    });
    let scrub_only = median_ns(|| {
        for g in &mut guarded {
            std::hint::black_box(g.scrub());
        }
    });
    println!(
        "latency_ns (host-dependent, not hashed): eval {base} scrub+eval {with} scrub_only {scrub_only} overhead {:.1}%",
        (with as f64 - base as f64) * 100.0 / base as f64
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_gates_pass_and_replay() {
        let (a, pass) = report();
        assert!(pass, "{a}");
        assert_eq!(a, report().0);
    }
}
