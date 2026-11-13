//! Host-local paired comparison of the proven 256-way kernel with the new
//! const-generic 256/512-way CPU Boolean runtime on the exact same input set.
use super::ComplementCircuit;
use std::hint::black_box;
use std::time::Instant;

const PAIRS: usize = 65536;
const WARMUPS: usize = 2;
const ROUNDS: usize = 7;

fn input_set() -> Vec<(u8, u8)> {
    (0..PAIRS)
        .map(|i| {
            let x = (i as u16).wrapping_mul(40503).wrapping_add(12345);
            crate::deinterleave(x)
        })
        .collect()
}
fn checksum(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf29ce484222325u64, |state, &byte| {
        (state ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}
fn median(mut items: Vec<f64>) -> f64 {
    items.sort_by(f64::total_cmp);
    items[items.len() / 2]
}
fn timing<F>(input_count: usize, mut runner: F) -> f64
where
    F: FnMut(),
{
    let start = Instant::now();
    runner();
    start.elapsed().as_nanos() as f64 / input_count as f64
}

impl ComplementCircuit {
    /// Baseline is existing separately verified 256-lane code. Candidates
    /// use one monomorphized generic graph walker, with widths 4 and 8.
    pub fn benchmark_generic_widths(&self) {
        let input = input_set();
        let mut original = vec![0u8; input.len()];
        let mut generic4 = vec![0u8; input.len()];
        let mut generic8 = vec![0u8; input.len()];
        let mut scratch256 = self.new_scratch256();
        let mut scratch4 = self.new_packed_scratch::<4>();
        let mut scratch8 = self.new_packed_scratch::<8>();

        self.evaluate_slice256_into(&input, &mut scratch256, &mut original)
            .expect("original 256-way shape");
        self.eval_packed_stream(&input, &mut scratch4, &mut generic4)
            .expect("generic four-word shape");
        self.eval_packed_stream(&input, &mut scratch8, &mut generic8)
            .expect("generic eight-word shape");
        assert_eq!(original, generic4, "generic width 4 mismatch");
        assert_eq!(original, generic8, "generic width 8 mismatch");

        let mut samples: [Vec<f64>; 3] = std::array::from_fn(|_| Vec::with_capacity(ROUNDS));
        for trial in 0..(WARMUPS + ROUNDS) {
            for position in 0..3 {
                let candidate = (position + trial) % 3;
                let elapsed = match candidate {
                    0 => timing(input.len(), || {
                        self.evaluate_slice256_into(
                            black_box(&input),
                            &mut scratch256,
                            &mut original,
                        )
                        .expect("baseline shape");
                        black_box(&original);
                    }),
                    1 => timing(input.len(), || {
                        self.eval_packed_stream(
                            black_box(&input),
                            &mut scratch4,
                            &mut generic4,
                        )
                        .expect("generic width four shape");
                        black_box(&generic4);
                    }),
                    _ => timing(input.len(), || {
                        self.eval_packed_stream(
                            black_box(&input),
                            &mut scratch8,
                            &mut generic8,
                        )
                        .expect("generic width eight shape");
                        black_box(&generic8);
                    }),
                };
                if trial >= WARMUPS {
                    samples[candidate].push(elapsed);
                }
            }
        }
        assert_eq!(original, generic4, "timed width4 output mismatch");
        assert_eq!(original, generic8, "timed width8 output mismatch");
        let t256 = median(samples[0].clone());
        let t4 = median(samples[1].clone());
        let t8 = median(samples[2].clone());
        assert!(t256.is_finite() && t4.is_finite() && t8.is_finite());
        assert!(t256 > 0.0 && t4 > 0.0 && t8 > 0.0);
        println!(
            "POSIT8_PACKED_BENCH,op={},pairs={},rounds={},baseline256_ns={:.3},generic256_ns={:.3},generic512_ns={:.3},generic256_ratio={:.5},generic512_ratio={:.5},checksum={:016x}",
            self.operation.name(),
            PAIRS,
            ROUNDS,
            t256,
            t4,
            t8,
            t4 / t256,
            t8 / t256,
            checksum(&original)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_permutation_has_no_duplicates() {
        let mut seen = vec![false; PAIRS];
        for (a, b) in input_set() {
            let id = crate::interleave(a, b) as usize;
            assert!(!seen[id]);
            seen[id] = true;
        }
        assert!(seen.iter().all(|x| *x));
    }

    #[test]
    fn fixed_statistics_are_stable() {
        assert_eq!(median(vec![4.0, 1.0, 2.0]), 2.0);
        assert_eq!(checksum(&[]), 0xcbf29ce484222325);
    }
}
