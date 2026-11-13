//! Paired host-local throughput experiment: 64-way native u64 Boolean planes
//! versus the SAME learned graph evaluated one operand pair at a time.
//! Not a comparison to hand-written arithmetic or assembled optimized kernels.
use super::ComplementCircuit;
use std::hint::black_box;
use std::time::Instant;

const INPUT_COUNT: usize = 65536;
const ROUNDS: usize = 7;
const WARMUP: usize = 2;

fn fixed_inputs() -> Vec<(u8, u8)> {
    (0..INPUT_COUNT)
        .map(|i| {
            let packed = (i as u16).wrapping_mul(40503).wrapping_add(12345);
            crate::deinterleave(packed)
        })
        .collect()
}
fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325u64, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}
fn paired_median(mut rows: Vec<f64>) -> f64 {
    rows.sort_by(f64::total_cmp);
    rows[rows.len() / 2]
}
fn time_scalar(circuit: &ComplementCircuit, inputs: &[(u8, u8)], output: &mut [u8]) -> f64 {
    let start = Instant::now();
    for (slot, &(a, b)) in output.iter_mut().zip(inputs.iter()) {
        *slot = circuit.evaluate(black_box(a), black_box(b));
    }
    black_box(&output[..]);
    start.elapsed().as_nanos() as f64 / inputs.len() as f64
}
fn time_bitplanes(
    circuit: &ComplementCircuit,
    inputs: &[(u8, u8)],
    scratch: &mut super::bitplane64::Scratch64,
    output: &mut [u8],
) -> f64 {
    let start = Instant::now();
    circuit
        .evaluate_slice_into(black_box(inputs), scratch, output)
        .expect("correct workspace and output length");
    black_box(&output[..]);
    start.elapsed().as_nanos() as f64 / inputs.len() as f64
}

impl ComplementCircuit {
    /// All timings are in ns per independent operand pair, including
    /// input transpose, gate evaluation, output transpose and caller output.
    pub fn benchmark_bitplanes64(&self) {
        let inputs = fixed_inputs();
        let mut scratch = self.new_scratch64();
        let mut scalar = vec![0u8; inputs.len()];
        let mut packed = vec![0u8; inputs.len()];
        time_scalar(self, &inputs, &mut scalar);
        time_bitplanes(self, &inputs, &mut scratch, &mut packed);
        assert_eq!(
            scalar, packed,
            "a performance comparison cannot change outputs"
        );

        let mut scalar_times = Vec::with_capacity(ROUNDS);
        let mut packed_times = Vec::with_capacity(ROUNDS);
        for round in 0..(WARMUP + ROUNDS) {
            if round & 1 == 0 {
                let s = time_scalar(self, &inputs, &mut scalar);
                let p = time_bitplanes(self, &inputs, &mut scratch, &mut packed);
                if round >= WARMUP {
                    scalar_times.push(s);
                    packed_times.push(p);
                }
            } else {
                let p = time_bitplanes(self, &inputs, &mut scratch, &mut packed);
                let s = time_scalar(self, &inputs, &mut scalar);
                if round >= WARMUP {
                    scalar_times.push(s);
                    packed_times.push(p);
                }
            }
        }
        assert_eq!(scalar, packed, "timed output divergence");
        let scalar_ns = paired_median(scalar_times);
        let packed_ns = paired_median(packed_times);
        assert!(scalar_ns.is_finite() && packed_ns.is_finite());
        assert!(scalar_ns > 0.0 && packed_ns > 0.0);
        println!(
            "POSIT8_BITPLANE64_BENCH,op={},inputs={},rounds={},median_scalar_ns={:.3},median_packed_ns={:.3},ratio_packed_to_scalar={:.5},checksum={:016x}",
            self.operation.name(),
            INPUT_COUNT,
            ROUNDS,
            scalar_ns,
            packed_ns,
            packed_ns / scalar_ns,
            checksum(&scalar)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_schedule_is_a_permutation_of_all_65536_pairs() {
        let mut seen = vec![false; INPUT_COUNT];
        for (a, b) in fixed_inputs() {
            let packed = crate::interleave(a, b) as usize;
            assert!(!seen[packed]);
            seen[packed] = true;
        }
        assert!(seen.iter().all(|&value| value));
    }

    #[test]
    fn fixed_median_and_output_checksum_are_reproducible() {
        assert_eq!(paired_median(vec![10.0, 3.0, 8.0]), 8.0);
        assert_eq!(checksum(&[]), 0xcbf29ce484222325u64);
    }
}
