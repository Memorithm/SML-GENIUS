//! BPB-3/v0: experimental Boolean bitplanes with a shared posit-inspired scale.
//! This is NOT a Posit Standard (2022) numeric type.

const N: usize = 64;
const ENCODED_BYTES: usize = 26;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bpb3 {
    scale: u16,
    negative: u64,
    lo: u64,
    hi: u64,
}

pub fn encode_scale(k: i8) -> Option<u16> {
    if !(-24..=27).contains(&k) {
        return None;
    }
    let regime = i16::from(k).div_euclid(4);
    let exp = i16::from(k).rem_euclid(4) as u16;
    let run = (if regime >= 0 { regime + 1 } else { -regime }) as usize;
    let mut word = 0u16;
    let mut pos = 0usize;
    for _ in 0..run {
        if regime >= 0 {
            word |= 1 << (15 - pos);
        }
        pos += 1;
    }
    if regime < 0 {
        word |= 1 << (15 - pos);
    }
    pos += 1;
    if exp & 2 != 0 {
        word |= 1 << (15 - pos);
    }
    pos += 1;
    if exp & 1 != 0 {
        word |= 1 << (15 - pos);
    }
    Some(word)
}

pub fn decode_scale(word: u16) -> Option<i8> {
    let leading = (word >> 15) & 1;
    let mut run = 0usize;
    while run < 16 && ((word >> (15 - run)) & 1) == leading {
        run += 1;
    }
    if run == 0 || run + 2 >= 16 {
        return None;
    }
    let e = (((word >> (14 - run)) & 1) << 1) | ((word >> (13 - run)) & 1);
    let r = if leading == 1 {
        run as i16 - 1
    } else {
        -(run as i16)
    };
    let k = i8::try_from(4 * r + i16::from(e)).ok()?;
    (encode_scale(k) == Some(word)).then_some(k)
}

impl Bpb3 {
    pub fn pack(weights: &[i8; N], exponent: i8) -> Result<Self, &'static str> {
        let scale = encode_scale(exponent).ok_or("out-of-range scale")?;
        let (mut negative, mut lo, mut hi) = (0u64, 0u64, 0u64);
        for (i, &q) in weights.iter().enumerate() {
            if !(-3..=3).contains(&q) {
                return Err("weight outside [-3,3]");
            }
            let bit = 1u64 << i;
            if q < 0 {
                negative |= bit;
            }
            if q.unsigned_abs() & 1 != 0 {
                lo |= bit;
            }
            if q.unsigned_abs() & 2 != 0 {
                hi |= bit;
            }
        }
        Ok(Self {
            scale,
            negative,
            lo,
            hi,
        })
    }

    pub fn unpack(&self) -> [i8; N] {
        let mut weights = [0; N];
        for (i, q) in weights.iter_mut().enumerate() {
            let mag = ((self.lo >> i) & 1) as i8 + 2 * ((self.hi >> i) & 1) as i8;
            *q = if (self.negative >> i) & 1 == 1 {
                -mag
            } else {
                mag
            };
        }
        weights
    }

    pub fn exponent(&self) -> i8 {
        decode_scale(self.scale).expect("canonical internal scale")
    }

    /// Input bit 1 represents -1, input bit 0 represents +1.
    /// Returns exact dot-product integer mantissa; result means D*2^exponent.
    pub fn dot(&self, input_negative: u64) -> i32 {
        let same_sign = !(self.negative ^ input_negative);
        let count = |plane: u64| -> i32 {
            2 * (same_sign & plane).count_ones() as i32 - plane.count_ones() as i32
        };
        count(self.lo) + 2 * count(self.hi)
    }

    pub fn to_bytes(self) -> [u8; ENCODED_BYTES] {
        let mut bytes = [0u8; ENCODED_BYTES];
        bytes[0..2].copy_from_slice(&self.scale.to_le_bytes());
        bytes[2..10].copy_from_slice(&self.negative.to_le_bytes());
        bytes[10..18].copy_from_slice(&self.lo.to_le_bytes());
        bytes[18..26].copy_from_slice(&self.hi.to_le_bytes());
        bytes
    }

    pub fn from_bytes(bytes: &[u8; ENCODED_BYTES]) -> Result<Self, &'static str> {
        let b = Self {
            scale: u16::from_le_bytes(bytes[0..2].try_into().unwrap()),
            negative: u64::from_le_bytes(bytes[2..10].try_into().unwrap()),
            lo: u64::from_le_bytes(bytes[10..18].try_into().unwrap()),
            hi: u64::from_le_bytes(bytes[18..26].try_into().unwrap()),
        };
        if decode_scale(b.scale).is_none() {
            return Err("noncanonical scale");
        }
        if b.negative & !(b.lo | b.hi) != 0 {
            return Err("negative zero");
        }
        Ok(b)
    }
}

fn next_random(s: &mut u64) -> u64 {
    *s ^= *s >> 12;
    *s ^= *s << 25;
    *s ^= *s >> 27;
    (*s).wrapping_mul(2685821657736338717)
}

fn x_sign(x: u64, i: usize) -> i32 {
    if (x >> i) & 1 == 1 {
        -1
    } else {
        1
    }
}

fn dense_dot(q: &[i8; N], x: u64) -> i32 {
    q.iter()
        .enumerate()
        .map(|(i, &v)| i32::from(v) * x_sign(x, i))
        .sum()
}

fn teacher(x: u64) -> i8 {
    if 3 * x_sign(x, 0) - 2 * x_sign(x, 1) + 2 * x_sign(x, 2) >= 0 {
        1
    } else {
        -1
    }
}

/// Simple supervised linear learner. The target rule is unavailable to it:
/// it sees only examples with labels and estimates all 64 correlations.
pub fn learn(seed: u64, examples: usize) -> Bpb3 {
    assert!(examples > 0);
    let mut rng = seed;
    let mut correlations = [0i64; N];
    for _ in 0..examples {
        let x = next_random(&mut rng);
        let y = i64::from(teacher(x));
        for (i, sum) in correlations.iter_mut().enumerate() {
            *sum += y * i64::from(x_sign(x, i));
        }
    }
    let mut q = [0i8; N];
    for (i, &c) in correlations.iter().enumerate() {
        // Fixed threshold abs(correlation) > 0.12.
        if c.abs() * 100 > examples as i64 * 12 {
            let rounded = (c.abs() * 4 + examples as i64 / 2) / examples as i64;
            let mag = rounded.clamp(1, 3) as i8;
            q[i] = if c < 0 { -mag } else { mag };
        }
    }
    Bpb3::pack(&q, 0).expect("bounded learned weights")
}

pub fn heldout(model: &Bpb3, seed: u64, examples: usize) -> usize {
    let mut rng = seed;
    let mut correct = 0;
    for _ in 0..examples {
        let x = next_random(&mut rng);
        let prediction = if model.dot(x) >= 0 { 1 } else { -1 };
        correct += usize::from(prediction == teacher(x));
    }
    correct
}

fn benchmark() {
    use std::hint::black_box;
    use std::time::Instant;
    const BLOCKS: usize = 4096;
    const ROUNDS: usize = 64;
    let mut rng = 0x47_22_b0_01u64;
    let mut packed = Vec::with_capacity(BLOCKS);
    let mut dense = Vec::with_capacity(BLOCKS);
    let mut inputs = Vec::with_capacity(BLOCKS);
    for _ in 0..BLOCKS {
        let mut q = [0i8; N];
        for v in &mut q {
            *v = (next_random(&mut rng) % 7) as i8 - 3;
        }
        packed.push(Bpb3::pack(&q, 0).unwrap());
        dense.push(q);
        inputs.push(next_random(&mut rng));
    }
    let t0 = Instant::now();
    let mut packed_sum = 0i64;
    for _ in 0..ROUNDS {
        for (w, &x) in packed.iter().zip(&inputs) {
            packed_sum += i64::from(black_box(w.dot(black_box(x))));
        }
    }
    let packed_ns = t0.elapsed().as_nanos();
    let t1 = Instant::now();
    let mut dense_sum = 0i64;
    for _ in 0..ROUNDS {
        for (w, &x) in dense.iter().zip(&inputs) {
            dense_sum += i64::from(black_box(dense_dot(w, black_box(x))));
        }
    }
    let dense_ns = t1.elapsed().as_nanos();
    assert_eq!(packed_sum, dense_sum);
    black_box(packed_sum);
    println!(
        "Weights: packed={} B; INT8={} B (inputs/allocations excluded)",
        BLOCKS * ENCODED_BYTES,
        BLOCKS * N
    );
    println!(
        "Warm microbenchmark: packed={} ns, INT8={} ns ({} calls per arm)",
        packed_ns,
        dense_ns,
        BLOCKS * ROUNDS
    );
    println!("No measured DRAM traffic or end-to-end model speed claim.");
}

fn main() {
    if std::env::args().any(|arg| arg == "--bench") {
        benchmark();
        return;
    }
    let model = learn(0x2bde, 1024);
    let correct = heldout(&model, 0xdead_beef_8899, 512);
    println!(
        "BPB-3/v0: {} bytes/64 weights, exponent {}, active {}",
        ENCODED_BYTES,
        model.exponent(),
        model.unpack().iter().filter(|&&q| q != 0).count()
    );
    println!("Synthetic heldout supervised accuracy: {correct}/512");
    println!("Not a claim of trained language-model quality.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regime_and_scale() {
        for k in -24..=27 {
            let word = encode_scale(k).unwrap();
            assert_eq!(decode_scale(word), Some(k));
        }
        assert_eq!(encode_scale(-25), None);
        assert_eq!(encode_scale(28), None);
        assert_eq!(decode_scale(0), None);
    }

    #[test]
    fn exhaustive_single_coefficient_and_inputs() {
        for value in -3..=3 {
            for i in 0..N {
                let mut weights = [0i8; N];
                weights[i] = value;
                let block = Bpb3::pack(&weights, 0).unwrap();
                assert_eq!(block.unpack(), weights);
                for x in [0, !0, 1u64 << i, !(1u64 << i)] {
                    assert_eq!(block.dot(x), dense_dot(&weights, x));
                }
            }
        }
    }

    #[test]
    fn randomized_oracle_and_serialization() {
        let mut rng = 0x1ea3_cafe;
        for _ in 0..4096 {
            let mut weights = [0i8; N];
            for q in &mut weights {
                *q = (next_random(&mut rng) % 7) as i8 - 3;
            }
            let exponent = (next_random(&mut rng) % 52) as i8 - 24;
            let block = Bpb3::pack(&weights, exponent).unwrap();
            let serialized = block.to_bytes();
            let recovered = Bpb3::from_bytes(&serialized).unwrap();
            assert_eq!(recovered, block);
            assert_eq!(recovered.unpack(), weights);
            assert_eq!(recovered.exponent(), exponent);
            let x = next_random(&mut rng);
            assert_eq!(recovered.dot(x), dense_dot(&weights, x));
        }
    }

    #[test]
    fn invalid_weight_and_negative_zero() {
        let mut weights = [0i8; N];
        weights[0] = 4;
        assert!(Bpb3::pack(&weights, 0).is_err());
        let mut bytes = Bpb3::pack(&[0i8; N], 0).unwrap().to_bytes();
        bytes[2] |= 1;
        assert!(Bpb3::from_bytes(&bytes).is_err());
    }

    #[test]
    fn effective_learning_on_disjoint_holdout() {
        let model = learn(0x2bde, 1024);
        let correct = heldout(&model, 0xdead_beef_8899, 512);
        assert!(correct >= 461, "accuracy {correct}/512");
    }
}
