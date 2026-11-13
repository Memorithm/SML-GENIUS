//! CPU-only exploratory BPB-3 A8 INT8 comparison over the SAME canonical wires.
//! No native Q3_K comparison, full-model quality evidence or speed claim.
//! The independent decoded integer oracle runs BEFORE any timed samples.
#![forbid(unsafe_code)]

#[path = "bpb3_a8_superblock.rs"]
#[allow(dead_code)]
mod a8;
#[path = "bpb3_a8_tensor.rs"]
#[allow(dead_code)]
mod tensor;

use std::hint::black_box;
use std::time::Instant;

const LANES: usize = 4;
const WIDTH: usize = 256;
const ROUNDS: usize = 7;
const REPEATS: usize = 3;

fn fixtures(rows: usize) -> Vec<Vec<u8>> {
    (0..rows)
        .map(|row| {
            let base = if row % 2 == 0 { -24 } else { 27 };
            let groups = std::array::from_fn(|group| {
                let mode = 1 + ((row + group) % 3) as u8;
                let max = (1i8 << mode) - 1;
                let codes = std::array::from_fn(|offset| {
                    let magnitude =
                        ((row * 17 + group * 13 + offset * 7) % (usize::from(max as u8) + 1)) as i8;
                    if (row + group + offset) % 2 == 0 {
                        magnitude
                    } else {
                        -magnitude
                    }
                });
                a8::Group {
                    mode,
                    exponent: base,
                    codes,
                    sse: 0.0,
                }
            });
            a8::pack(&a8::Superblock {
                base,
                groups,
                sse: 0.0,
            })
            .unwrap()
        })
        .collect()
}

fn activations() -> [[i8; WIDTH]; LANES] {
    [
        [-128; WIDTH],
        [127; WIDTH],
        std::array::from_fn(|i| i as u8 as i8),
        std::array::from_fn(|i| (i as u8).wrapping_mul(53) as i8),
    ]
}

/// Reproduce the original correctness-first nine-A8-parses-per-row method,
/// preserving the same O(1) indexed tensor row lookup as the other arms.
fn reference_nine_pass_into(
    view: &tensor::TensorView<'_>,
    input: &[i8; WIDTH],
    output: &mut [i128],
) {
    assert_eq!(view.len(), output.len());
    let masks: [[u64; 4]; 8] = std::array::from_fn(|bit| {
        std::array::from_fn(|word| {
            let mut mask = 0u64;
            for offset in 0..64 {
                let code = input[word * 64 + offset] as u8;
                mask |= u64::from((code >> bit) & 1) << offset;
            }
            mask
        })
    });
    let positive = [0u64; 4];
    for (row, out) in output.iter_mut().enumerate() {
        let wire = view.row(row).unwrap();
        let whole_sum = a8::dot_exact(wire, &positive).unwrap();
        let mut sum = 0i128;
        for (bit, mask) in masks.iter().enumerate() {
            let bipolar = a8::dot_exact(wire, mask).unwrap();
            assert_eq!((whole_sum - bipolar) % 2, 0);
            let input_power = if bit == 7 { -128i128 } else { 1i128 << bit };
            sum += ((whole_sum - bipolar) / 2) * input_power;
        }
        *out = sum;
    }
}

fn evaluate(
    arm: usize,
    view: &tensor::TensorView<'_>,
    inputs: &[[i8; WIDTH]; LANES],
    singles: &mut [Vec<i128>; LANES],
    batch: &mut [[i128; LANES]],
) {
    match arm {
        0 => {
            for lane in 0..LANES {
                reference_nine_pass_into(view, &inputs[lane], &mut singles[lane]);
            }
            black_box(&*singles);
        }
        1 => {
            for lane in 0..LANES {
                view.signed_i8_matvec_into(&inputs[lane], &mut singles[lane])
                    .unwrap();
            }
            black_box(&*singles);
        }
        2 => {
            view.signed_i8_matvec_batch4_into(inputs, batch).unwrap();
            black_box(&*batch);
        }
        _ => unreachable!(),
    }
}

fn verify(
    wires: &[Vec<u8>],
    view: &tensor::TensorView<'_>,
    inputs: &[[i8; WIDTH]; LANES],
    singles: &mut [Vec<i128>; LANES],
    batch: &mut [[i128; LANES]],
) -> i128 {
    evaluate(2, view, inputs, singles, batch);
    let mut checksum = 0i128;
    for (row, wire) in wires.iter().enumerate() {
        let (codes, exponents) = a8::unpack(wire).unwrap();
        for lane in 0..LANES {
            let expected: i128 = (0..WIDTH)
                .map(|i| {
                    i128::from(codes[i])
                        * i128::from(inputs[lane][i])
                        * (1i128 << (i32::from(exponents[i / 16]) + 24))
                })
                .sum();
            assert_eq!(batch[row][lane], expected, "batch row={row} lane={lane}");
            checksum = checksum.wrapping_add(expected);
        }
    }
    for arm in [0, 1] {
        evaluate(arm, view, inputs, singles, batch);
        for row in 0..view.len() {
            for lane in 0..LANES {
                assert_eq!(
                    singles[lane][row], batch[row][lane],
                    "arm={arm} row={row} lane={lane}"
                );
            }
        }
    }
    checksum
}

fn median(values: &mut [u128]) -> u128 {
    values.sort_unstable();
    values[values.len() / 2]
}

fn run(rows: usize, measure: bool) {
    let wires = fixtures(rows);
    let bytes = tensor::encode_tensor(&wires).unwrap();
    let view = tensor::TensorView::new(&bytes).unwrap();
    let inputs = activations();
    let mut singles: [Vec<i128>; LANES] = std::array::from_fn(|_| vec![0; rows]);
    let mut batch = vec![[0i128; LANES]; rows];
    let checksum = verify(&wires, &view, &inputs, &mut singles, &mut batch);
    let payload: usize = wires.iter().map(Vec::len).sum();
    if !measure {
        println!(
            "smoke_ok rows={rows} lanes={LANES} wire_bytes={payload} tensor_bytes={} checksum={checksum}",
            bytes.len()
        );
        return;
    }
    let mut samples: [Vec<u128>; 3] = std::array::from_fn(|_| Vec::with_capacity(ROUNDS));
    for round in 0..ROUNDS {
        // Rotate the order to reduce time-drift bias across implementations.
        for step in 0..3 {
            let arm = (round + step) % 3;
            let begin = Instant::now();
            for _ in 0..REPEATS {
                evaluate(
                    arm,
                    black_box(&view),
                    black_box(&inputs),
                    &mut singles,
                    &mut batch,
                );
            }
            samples[arm].push(begin.elapsed().as_nanos() / REPEATS as u128);
        }
    }
    for (arm, name) in ["reference_nine_pass_4x", "fused_single_4x", "batch4"]
        .iter()
        .enumerate()
    {
        let ns = median(&mut samples[arm]);
        println!(
            "a8_int8,arch={},rows={rows},lanes={LANES},arm={name},median_ns_per_four_vectors={ns},rounds={ROUNDS},repeats={REPEATS},wire_bytes={payload},tensor_bytes={},checksum={checksum}",
            std::env::consts::ARCH,
            view.total_serialized_bytes(),
        );
    }
}

fn main() {
    let measure = match std::env::args().nth(1).as_deref() {
        None | Some("--smoke") => false,
        Some("--measure") => true,
        other => panic!("unknown argument {other:?}: use --smoke or --measure"),
    };
    if measure {
        for rows in [64, 257, 1024] {
            run(rows, true);
        }
    } else {
        for rows in [1, 17, 257] {
            run(rows, false);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn deterministic_oracle_and_page_boundary() {
        super::run(257, false);
    }
}
