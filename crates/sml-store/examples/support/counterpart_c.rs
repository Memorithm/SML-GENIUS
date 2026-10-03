//! Port of the supplied C state generator, not a learned semantic encoder.
#![allow(dead_code)]
use sml_core::semantic::SemanticState;
pub const HAMMING: [usize; 6] = [1, 2, 4, 8, 16, 32];
pub const FACTS: usize = 12;
struct Stream(u64);
impl Stream {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}
fn base(seed: u64) -> u64 {
    seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xA5A5_A5A5_A5A5_A5A5
}
pub fn random_state(seed: u64, index: u64) -> SemanticState<64> {
    let mut stream =
        Stream::new(base(seed) ^ index.wrapping_add(1).wrapping_mul(0xD6E8_FEB8_6659_FD93));
    let mut s = SemanticState::<64>::zeroed().unwrap();
    for field in [&mut s.value, &mut s.meaning_mask, &mut s.routing] {
        for bit in 0..64 {
            field.set(bit, stream.next() & 1 == 1).unwrap();
        }
    }
    s
}
pub fn near_miss(seed: u64, fact: u64, h: usize) -> Result<SemanticState<64>, &'static str> {
    if h > 192 {
        return Err("Hamming distance exceeds populated width");
    }
    let mut s = random_state(seed, fact);
    let mut rng = Stream::new(
        base(seed)
            ^ fact.wrapping_add(1).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
            ^ (h as u64).wrapping_mul(0x1656_67B1_9E37_79F9),
    );
    let mut positions: Vec<usize> = (0..192).collect();
    for k in 0..h {
        let j = k + (rng.next() % (192 - k) as u64) as usize;
        positions.swap(k, j);
    }
    for p in &positions[..h] {
        let f = match p / 64 {
            0 => &mut s.value,
            1 => &mut s.meaning_mask,
            _ => &mut s.routing,
        };
        let b = f.get(p % 64).unwrap();
        f.set(p % 64, !b).unwrap();
    }
    Ok(s)
}
/// Legacy C diagnostic surrogate; deliberately omits A. Not an authority token.
pub fn legacy_record_id(s: &SemanticState<64>) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for f in [&s.value, &s.meaning_mask, &s.routing] {
        for bit in 0..64 {
            hash ^= u64::from(f.get(bit).unwrap());
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}
pub fn key_fingerprint(key: &[i16]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325_u64;
    for v in key {
        for b in v.to_le_bytes() {
            h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    h
}
pub fn queries(seed: u64, fact: u64) -> Vec<(String, SemanticState<64>)> {
    let mut out = vec![("written".into(), random_state(seed, fact))];
    for h in HAMMING {
        out.push((format!("near_{h}"), near_miss(seed, fact, h).unwrap()));
    }
    out.push(("independent".into(), random_state(seed, 1000 + fact)));
    out
}
