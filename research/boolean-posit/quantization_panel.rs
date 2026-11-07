//! Model-independent BPB-3 quantization-quality control panel (research only).
//! No learned SML weights, model architecture, GPU performance or novelty claim.
//! Equal block size (64), scale search [-24,27], bipolar inference inputs.
//! INT3_sym and BPB-3 intentionally share the SAME numeric codebook.

#[path = "prototype.rs"]
#[allow(dead_code)]
mod bpb3;

const LANES: usize = 64;
const BLOCKS: usize = 48;
const MASKS: usize = 32;
const FAMILIES: [&str; 4] = ["uniform", "sparse", "outlier", "logspread"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Quant {
    Ternary2,
    Bpb3,
    Symmetric3,
    Symmetric4,
}

impl Quant {
    const ALL: [Self; 4] = [
        Self::Ternary2,
        Self::Bpb3,
        Self::Symmetric3,
        Self::Symmetric4,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::Ternary2 => "INT2_ternary",
            Self::Bpb3 => "BPB-3",
            Self::Symmetric3 => "INT3_symmetric",
            Self::Symmetric4 => "INT4_symmetric",
        }
    }

    const fn max_magnitude(self) -> i8 {
        match self {
            Self::Ternary2 => 1,
            Self::Bpb3 | Self::Symmetric3 => 3,
            Self::Symmetric4 => 7,
        }
    }

    // Per-block serialized bytes, including the same 16-bit shared scale.
    // Dense INT3 bitpacking does not imply a fast kernel.
    const fn wire_bytes(self) -> usize {
        match self {
            Self::Ternary2 => 18,
            Self::Bpb3 | Self::Symmetric3 => 26,
            Self::Symmetric4 => 34,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Block {
    q: [i8; LANES],
    k: i8,
    sq_error: f64,
}

fn quantize(values: &[f64; LANES], quant: Quant) -> Block {
    let mut best = Block {
        q: [0; LANES],
        k: 0,
        sq_error: f64::INFINITY,
    };
    for k in -24_i8..=27 {
        let step = 2.0f64.powi(i32::from(k));
        let max = f64::from(quant.max_magnitude());
        let mut q = [0i8; LANES];
        let mut error = 0.0f64;
        for (i, &weight) in values.iter().enumerate() {
            // Exact common rule, including round-to-nearest ties away from zero.
            q[i] = (weight / step).round().clamp(-max, max) as i8;
            let delta = weight - f64::from(q[i]) * step;
            error += delta * delta;
        }
        if error < best.sq_error {
            best = Block {
                q,
                k,
                sq_error: error,
            };
        }
    }
    best
}

fn next_random(seed: &mut u64) -> u64 {
    *seed ^= *seed >> 12;
    *seed ^= *seed << 25;
    *seed ^= *seed >> 27;
    (*seed).wrapping_mul(2685821657736338717)
}

fn unit(seed: &mut u64) -> f64 {
    (next_random(seed) >> 11) as f64 / (1u64 << 53) as f64
}

fn weights(family: &str, seed: &mut u64) -> [f64; LANES] {
    std::array::from_fn(|i| {
        let base = 2.0 * unit(seed) - 1.0;
        match family {
            "uniform" => base * 0.85,
            "sparse" => {
                if next_random(seed).is_multiple_of(5) {
                    base * 0.7
                } else {
                    0.0
                }
            }
            "outlier" => {
                if i % 31 == 0 {
                    base * 8.0
                } else {
                    base * 0.12
                }
            }
            "logspread" => {
                let exponent = (next_random(seed) % 13) as i32 - 12;
                base.signum() * 2.0f64.powi(exponent)
            }
            _ => unreachable!("fixed family"),
        }
    })
}

fn integer_dot(q: &[i8; LANES], mask: u64) -> i32 {
    q.iter()
        .enumerate()
        .map(|(i, &v)| if (mask >> i) & 1 == 0 { i32::from(v) } else { -i32::from(v) })
        .sum()
}

fn real_dot(values: &[f64; LANES], mask: u64) -> f64 {
    values
        .iter()
        .enumerate()
        .map(|(i, &v)| if (mask >> i) & 1 == 0 { v } else { -v })
        .sum()
}

#[derive(Default)]
struct Stats {
    weight_sq_error: f64,
    dot_sq_error: f64,
    coeffs: usize,
    dots: usize,
}

fn examine_family(family: &str, seed: u64) -> [Stats; 4] {
    let mut rng = seed;
    let mut result: [Stats; 4] = std::array::from_fn(|_| Stats::default());
    for _ in 0..BLOCKS {
        let original = weights(family, &mut rng);
        let blocks = Quant::ALL.map(|q| quantize(&original, q));
        assert_eq!(blocks[1], blocks[2], "equal-codebook BPB/INT3 control");
        let bp = bpb3::Bpb3::pack(&blocks[1].q, blocks[1].k)
            .expect("legal BPB quantization");
        assert_eq!(bp.to_bytes().len(), 26);
        assert_eq!(bp.unpack(), blocks[1].q);
        for (index, block) in blocks.iter().enumerate() {
            result[index].weight_sq_error += block.sq_error;
            result[index].coeffs += LANES;
        }
        for _ in 0..MASKS {
            let mask = next_random(&mut rng);
            let reference = real_dot(&original, mask);
            assert_eq!(bp.dot(mask), integer_dot(&blocks[1].q, mask));
            for (index, block) in blocks.iter().enumerate() {
                let predicted = f64::from(integer_dot(&block.q, mask))
                    * 2.0f64.powi(i32::from(block.k));
                let delta = predicted - reference;
                result[index].dot_sq_error += delta * delta;
                result[index].dots += 1;
            }
        }
    }
    result
}

fn main() {
    println!("protocol=bpb3-independent-quantization-panel-v1,synthetic=true,model_integrated=false,gpu_measured=false");
    println!("family,format,wire_bytes_64,effective_bits_per_weight,weight_rmse,dot_rmse,coefficients,dots");
    for (index, family) in FAMILIES.iter().enumerate() {
        let stats = examine_family(
            family,
            0x9e37_79b9_7f4a_7c15u64 ^ ((index as u64 + 1) * 0x10001),
        );
        for (quant, stat) in Quant::ALL.iter().zip(stats.iter()) {
            println!(
                "{},{},{},{:.5},{:.12},{:.12},{},{}",
                family,
                quant.label(),
                quant.wire_bytes(),
                8.0 * quant.wire_bytes() as f64 / LANES as f64,
                (stat.weight_sq_error / stat.coeffs as f64).sqrt(),
                (stat.dot_sq_error / stat.dots as f64).sqrt(),
                stat.coeffs,
                stat.dots
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exactly_matched_bpb3_vs_symmetric_int3() {
        for (i, family) in FAMILIES.iter().enumerate() {
            let mut seed = 0x36ab_bca7_u64 ^ (i as u64);
            for _ in 0..24 {
                let w = weights(family, &mut seed);
                let a = quantize(&w, Quant::Bpb3);
                let b = quantize(&w, Quant::Symmetric3);
                assert_eq!(a, b);
                let bp = bpb3::Bpb3::pack(&a.q, a.k).unwrap();
                for _ in 0..12 {
                    let x = next_random(&mut seed);
                    assert_eq!(bp.dot(x), integer_dot(&b.q, x));
                }
            }
        }
    }

    #[test]
    fn formats_account_for_shared_scale_and_realized_values() {
        assert_eq!(Quant::Ternary2.wire_bytes(), 18);
        assert_eq!(Quant::Bpb3.wire_bytes(), 26);
        assert_eq!(Quant::Symmetric3.wire_bytes(), 26);
        assert_eq!(Quant::Symmetric4.wire_bytes(), 34);
        for quant in Quant::ALL {
            for w in [[0.0; LANES], [1.0; LANES], [-0.75; LANES]] {
                let block = quantize(&w, quant);
                assert!(block.q.iter().all(|x| x.abs() <= quant.max_magnitude()));
                assert!((-24..=27).contains(&block.k));
                assert!(block.sq_error.is_finite());
            }
        }
    }

    #[test]
    fn deterministic_replay_and_real_bipolar_inference() {
        for family in FAMILIES {
            let a = examine_family(family, 0xcafe_9866);
            let b = examine_family(family, 0xcafe_9866);
            for (lhs, rhs) in a.iter().zip(b.iter()) {
                assert_eq!(lhs.weight_sq_error, rhs.weight_sq_error);
                assert_eq!(lhs.dot_sq_error, rhs.dot_sq_error);
                assert_eq!(lhs.coeffs, BLOCKS * LANES);
                assert_eq!(lhs.dots, BLOCKS * MASKS);
            }
            assert_eq!(a[1].weight_sq_error, a[2].weight_sq_error);
            assert_eq!(a[1].dot_sq_error, a[2].dot_sq_error);
        }
    }
}
