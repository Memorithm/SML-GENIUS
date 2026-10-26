//! GROK-ICM-2: ICM guards on real PVP6 gate bitplanes and Delta-KV record pages.
//! See `docs/SML-GROK-ICM-2-REAL-PAGES-PREREGISTRATION.md`.

#[path = "support/grok_icm_guard.rs"]
mod icm;

use icm::{fault, fnv1a64, Check, Crc32Guard, NoneGuard, PageGuard, SecdedGuard, SplitMix64};
use sml_core::delta_kv::DELTA_KV_ONE;
use sml_core::delta_kv_controls::{DKV_CONTROL_VALUE_BITS, DKV_CONTROL_VALUE_DIM};
use sml_core::pvp6_executor::Pvp6Gate;
use std::fmt::Write as _;

const WORDS: usize = 64;
const WRITES: usize = 2000;
const CAL: std::ops::Range<u64> = 0..4;
const TEST: std::ops::Range<u64> = 1000..1016;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Layout {
    Pvp6,
    Dkv,
}

fn pvp6_page(rng: &mut SplitMix64) -> Vec<u64> {
    let mut monos: Vec<u16> = Vec::new();
    while monos.len() < 64 {
        let m = rng.below(4096) as u16;
        if !monos.contains(&m) {
            monos.push(m);
        }
    }
    let gate = Pvp6Gate::<4096>::from_monomials(&monos).expect("valid PVP6 gate");
    gate.coefficients().to_vec()
}

fn dkv_record(rng: &mut SplitMix64) -> u64 {
    // Two records of DKV_CONTROL_VALUE_DIM Q12 i16 values = 64 bits.
    let one = i64::from(DELTA_KV_ONE);
    let mut w = 0u64;
    for slot in 0..(2 * DKV_CONTROL_VALUE_DIM) {
        let v = (rng.below((2 * one + 1) as u64) as i64 - one) as i16;
        w |= u64::from(v as u16) << (16 * slot);
    }
    w
}

fn initial_page(layout: Layout, rng: &mut SplitMix64) -> Vec<u64> {
    match layout {
        Layout::Pvp6 => pvp6_page(rng),
        Layout::Dkv => (0..WORDS).map(|_| dkv_record(rng)).collect(),
    }
}

fn intended(layout: Layout, rng: &mut SplitMix64, old: u64) -> u64 {
    match layout {
        Layout::Pvp6 => old ^ (1 << rng.below(64)),
        Layout::Dkv => dkv_record(rng),
    }
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
struct Stats {
    clean: u64,
    false_reject: u64,
    faulty: [u64; 3],
    detected: [u64; 3],
    silent: [u64; 3],
}

fn run<G: PageGuard>(layout: Layout, seed: u64, s: &mut Stats) {
    let mut rng = SplitMix64(seed.wrapping_mul(0x5851_f42d_4c95_7f2d) ^ layout as u64);
    let mut page = initial_page(layout, &mut rng);
    assert_eq!(page.len(), WORDS);
    let mut guard = G::init(&page);
    for _ in 0..WRITES {
        let idx = rng.below(WORDS as u64) as usize;
        let old = page[idx];
        let want = intended(layout, &mut rng, old);
        guard.apply_write(idx, old, want);
        let faulty = rng.below(8) == 0;
        let k = (rng.below(3) + 1) as u32;
        page[idx] = if faulty {
            fault(&mut rng, want, k)
        } else {
            want
        };
        let ok_data = page[idx] == want;
        let res = guard.check(&page);
        if !faulty {
            s.clean += 1;
            if res != Check::Clean {
                s.false_reject += 1;
            }
            continue;
        }
        let ki = (k - 1) as usize;
        s.faulty[ki] += 1;
        match res {
            Check::Clean => {
                debug_assert!(!ok_data);
                s.silent[ki] += 1;
            }
            Check::Detected => s.detected[ki] += 1,
            Check::Corrected(d) if d[idx] == want => s.detected[ki] += 1,
            Check::Corrected(_) => s.silent[ki] += 1,
        }
        // Repair to intended state before continuing (no fault accumulation).
        page[idx] = want;
        guard = G::init(&page);
    }
}

fn arm<G: PageGuard>(split: std::ops::Range<u64>, layout: Layout) -> Stats {
    let mut s = Stats::default();
    for seed in split {
        run::<G>(layout, seed, &mut s);
    }
    s
}

fn passes(s: &Stats) -> bool {
    s.detected[0] == s.faulty[0]
        && s.detected[1] == s.faulty[1]
        && s.false_reject * 1000 < s.clean.max(1)
}

fn line<G: PageGuard>(out: &mut String, name: &str, split: std::ops::Range<u64>, l: Layout) {
    let s = arm::<G>(split, l);
    let _ = writeln!(
        out,
        "{name} {:?} {}: clean {} false_reject {} | k1 {}/{} silent {} | k2 {}/{} silent {} | k3 {}/{} silent {} | gate {}",
        l,
        G::NAME,
        s.clean,
        s.false_reject,
        s.detected[0],
        s.faulty[0],
        s.silent[0],
        s.detected[1],
        s.faulty[1],
        s.silent[1],
        s.detected[2],
        s.faulty[2],
        s.silent[2],
        if passes(&s) { "PASS" } else { "FAIL" }
    );
}

fn report() -> String {
    assert_eq!(DKV_CONTROL_VALUE_BITS, 16 * DKV_CONTROL_VALUE_DIM);
    let mut out = String::from("GROK-ICM-2 report v1\n");
    for (name, split) in [("CAL", CAL), ("TEST", TEST)] {
        for l in [Layout::Pvp6, Layout::Dkv] {
            line::<NoneGuard>(&mut out, name, split.clone(), l);
            line::<SecdedGuard>(&mut out, name, split.clone(), l);
            line::<Crc32Guard>(&mut out, name, split.clone(), l);
        }
    }
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
    fn frozen_gates_on_sealed_test() {
        for l in [Layout::Pvp6, Layout::Dkv] {
            assert!(!passes(&arm::<NoneGuard>(TEST, l)), "NONE must fail");
            let sec = arm::<SecdedGuard>(TEST, l);
            assert!(passes(&sec), "SECDED {l:?} {sec:?}");
            let crc = arm::<Crc32Guard>(TEST, l);
            assert!(passes(&crc), "CRC32 {l:?} {crc:?}");
            assert_eq!(crc.silent, [0, 0, 0]);
        }
    }

    #[test]
    fn report_replays() {
        assert_eq!(report(), report());
    }
}
