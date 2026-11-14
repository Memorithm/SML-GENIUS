//! Pairwise throughput: 64-lane and 256-lane bit-sliced evaluation of
//! precisely the same complemented Boolean DAG on the same 65536 input pairs.
use super::ComplementCircuit;
use std::hint::black_box;
use std::time::Instant;

const INPUTS: usize = 65536;
const WARMUPS: usize = 2;
const ROUNDS: usize = 7;

fn fixed_pairs() -> Vec<(u8, u8)> {
    (0..INPUTS)
        .map(|i| {
            let encoded = (i as u16).wrapping_mul(40503).wrapping_add(12345);
            crate::deinterleave(encoded)
        })
        .collect()
}
fn hash_output(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325u64, |state, &byte| {
        (state ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}
fn median(mut timings: Vec<f64>) -> f64 {
    timings.sort_by(f64::total_cmp);
    timings[timings.len() / 2]
}
fn measure_64(
    circuit: &ComplementCircuit,
    inputs: &[(u8, u8)],
    scratch: &mut super::bitplane64::Scratch64,
    output: &mut [u8],
) -> f64 {
    let start = Instant::now();
    circuit
        .evaluate_slice_into(black_box(inputs), scratch, output)
        .expect("correct batch64 shape");
    black_box(&output[..]);
    start.elapsed().as_nanos() as f64 / inputs.len() as f64
}
fn measure_256(
    circuit: &ComplementCircuit,
    inputs: &[(u8, u8)],
    scratch: &mut super::bitplane256::Scratch256,
    output: &mut [u8],
) -> f64 {
    let start = Instant::now();
    circuit
        .evaluate_slice256_into(black_box(inputs), scratch, output)
        .expect("correct batch256 shape");
    black_box(&output[..]);
    start.elapsed().as_nanos() as f64 / inputs.len() as f64
}

impl ComplementCircuit {
    /// Includes input packing and output unpacking; no speedup assumed.
    pub fn benchmark_bitplanes256_against64(&self) {
        let inputs = fixed_pairs();
        let mut scratch64 = self.new_scratch64();
        let mut scratch256 = self.new_scratch256();
        let mut output64 = vec![0u8; inputs.len()];
        let mut output256 = vec![0u8; inputs.len()];
        measure_64(self, &inputs, &mut scratch64, &mut output64);
        measure_256(self, &inputs, &mut scratch256, &mut output256);
        assert_eq!(output64, output256, "256-lane graph altered outputs");

        let mut measurements64 = Vec::with_capacity(ROUNDS);
        let mut measurements256 = Vec::with_capacity(ROUNDS);
        for trial in 0..(WARMUPS + ROUNDS) {
            let (t64, t256) = if trial % 2 == 0 {
                let a = measure_64(self, &inputs, &mut scratch64, &mut output64);
                let b = measure_256(self, &inputs, &mut scratch256, &mut output256);
                (a, b)
            } else {
                let b = measure_256(self, &inputs, &mut scratch256, &mut output256);
                let a = measure_64(self, &inputs, &mut scratch64, &mut output64);
                (a, b)
            };
            if trial >= WARMUPS {
                measurements64.push(t64);
                measurements256.push(t256);
            }
        }
        assert_eq!(output64, output256, "timed output mismatch");
        let t64 = median(measurements64);
        let t256 = median(measurements256);
        assert!(t64.is_finite() && t256.is_finite() && t64 > 0.0 && t256 > 0.0);
        println!(
            "POSIT8_BITPLANE256_BENCH,op={},pairs={},rounds={},median_64_ns={:.3},median_256_ns={:.3},ratio_256_to_64={:.5},checksum={:016x}",
            self.operation.name(),
            INPUTS,
            ROUNDS,
            t64,
            t256,
            t256 / t64,
            hash_output(&output64)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Operation;

    #[test]
    fn predetermined_input_sequence_covers_all_65536_pairs() {
        let mut seen = vec![false; INPUTS];
        for (a, b) in fixed_pairs() {
            let code = crate::interleave(a, b) as usize;
            assert!(!seen[code], "repeated input");
            seen[code] = true;
        }
        assert!(seen.into_iter().all(|value| value));
    }

    #[test]
    fn reference_and_256_lanes_agree_on_mixed_lengths() {
        for operation in [Operation::Add, Operation::Mul] {
            let graph = ComplementCircuit::synthesize(operation).unwrap();
            let mut scratch64 = graph.new_scratch64();
            let mut scratch256 = graph.new_scratch256();
            for size in [0usize, 1, 63, 64, 65, 127, 255, 256, 257, 1025] {
                let pairs: Vec<_> = fixed_pairs().into_iter().take(size).collect();
                let mut a = vec![0xa5; size];
                let mut b = vec![0x5a; size];
                graph
                    .evaluate_slice_into(&pairs, &mut scratch64, &mut a)
                    .unwrap();
                graph
                    .evaluate_slice256_into(&pairs, &mut scratch256, &mut b)
                    .unwrap();
                assert_eq!(a, b, "operation={} size={size}", operation.name());
            }
        }
    }

    #[test]
    fn checksum_and_median_are_deterministic() {
        assert_eq!(hash_output(&[]), 0xcbf29ce484222325);
        assert_eq!(median(vec![11.0, 1.0, 3.0]), 3.0);
    }
}
