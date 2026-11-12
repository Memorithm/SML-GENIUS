//! A3: source-matched pretrained SmolLM2 reconstruction and SAME-mask
//! signed-bipolar dot measurements. CPU research, no model inference.
#![allow(dead_code)]
#[path = "bpb3_adaptive.rs"]
mod adaptive;
#[path = "gguf_k.rs"]
mod gguf;

use std::ffi::{c_char, c_void, CStr};
use std::fs;
use std::path::Path;

const N: usize = 256;
const ROWS: usize = 24;
const MASKS: usize = 16;
const GGML_Q3_K: i32 = 11;
const GGML_Q4_K: i32 = 12;
const SEED: u64 = 0xa3b0_114d_5c15_2026;

#[link(name = "ggml-base")]
unsafe extern "C" {
    fn ggml_type_name(ty: i32) -> *const c_char;
    fn ggml_blck_size(ty: i32) -> i64;
    fn ggml_type_size(ty: i32) -> usize;
    fn ggml_quantize_chunk(
        ty: i32,
        src: *const f32,
        dst: *mut c_void,
        start: i64,
        nrows: i64,
        n_per_row: i64,
        imatrix: *const f32,
    ) -> usize;
    fn dequantize_row_q3_K(x: *const c_void, y: *mut f32, k: i64);
    fn dequantize_row_q4_K(x: *const c_void, y: *mut f32, k: i64);
}

fn official(ty: i32, source: &[f32; N]) -> [f64; N] {
    let (name, bytes) = match ty {
        GGML_Q3_K => ("q3_K", gguf::Q3_BYTES),
        GGML_Q4_K => ("q4_K", gguf::Q4_BYTES),
        _ => panic!("STOP_INVALID_NATIVE_TYPE"),
    };
    let actual = unsafe { CStr::from_ptr(ggml_type_name(ty)) };
    assert_eq!(actual.to_str().unwrap(), name, "STOP_GGML_TYPE_NAME");
    assert_eq!(unsafe { ggml_blck_size(ty) }, N as i64);
    assert_eq!(unsafe { ggml_type_size(ty) }, bytes);
    let mut wire = vec![0u8; bytes];
    let written = unsafe {
        ggml_quantize_chunk(
            ty,
            source.as_ptr(),
            wire.as_mut_ptr().cast::<c_void>(),
            0,
            1,
            N as i64,
            std::ptr::null(),
        )
    };
    assert_eq!(written, bytes, "STOP_GGML_WRONG_ENCODED_BYTES");
    let rust = match ty {
        GGML_Q3_K => gguf::decode_q3_k(&wire).unwrap(),
        GGML_Q4_K => gguf::decode_q4_k(&wire).unwrap(),
        _ => unreachable!(),
    };
    let mut c = [0f32; N];
    unsafe {
        if ty == GGML_Q3_K {
            dequantize_row_q3_K(
                wire.as_ptr().cast::<c_void>(),
                c.as_mut_ptr(),
                N as i64,
            );
        } else {
            dequantize_row_q4_K(
                wire.as_ptr().cast::<c_void>(),
                c.as_mut_ptr(),
                N as i64,
            );
        }
    }
    for (index, (&x, &y)) in rust.iter().zip(&c).enumerate() {
        assert!(x.is_finite() && y.is_finite(), "STOP_NONFINITE_NATIVE");
        assert!(
            (x - y).abs() <= 1e-5f32 * x.abs().max(1.0),
            "STOP_NATIVE_RUST_C_MISMATCH type={ty} index={index}"
        );
    }
    rust.map(f64::from)
}

fn random(seed: &mut u64) -> u64 {
    *seed ^= *seed >> 12;
    *seed ^= *seed << 25;
    *seed ^= *seed >> 27;
    (*seed).wrapping_mul(2685821657736338717)
}

fn bipolar_dot(source: &[f64; N], masks: &[u64; 4]) -> f64 {
    source
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            if (masks[i / 64] >> (i % 64)) & 1 != 0 {
                -v
            } else {
                v
            }
        })
        .sum()
}

fn dense_integer_dot(codes: &[i8; 64], mask: u64) -> i32 {
    codes
        .iter()
        .enumerate()
        .map(|(i, &value)| {
            if (mask >> i) & 1 == 0 {
                i32::from(value)
            } else {
                -i32::from(value)
            }
        })
        .sum()
}

fn decode_blocks(wires: &[Vec<u8>]) -> [f64; N] {
    assert_eq!(wires.len(), 4, "STOP_FOUR_ADAPTIVE_GROUPS");
    let mut values = [0.0; N];
    for (group, wire) in wires.iter().enumerate() {
        let (codes, exponent) = adaptive::unpack(wire).unwrap();
        let scale = 2.0f64.powi(i32::from(exponent));
        for (index, &code) in codes.iter().enumerate() {
            values[group * 64 + index] = f64::from(code) * scale;
        }
    }
    values
}

fn boolean_dot(wires: &[Vec<u8>], masks: &[u64; 4]) -> f64 {
    assert_eq!(wires.len(), 4);
    let mut dot = 0.0;
    for group in 0..4 {
        let (integer, exponent) = adaptive::dot(&wires[group], masks[group]).unwrap();
        let (codes, oracle_exponent) = adaptive::unpack(&wires[group]).unwrap();
        assert_eq!(exponent, oracle_exponent);
        assert_eq!(integer, dense_integer_dot(&codes, masks[group]));
        dot += f64::from(integer) * 2.0f64.powi(i32::from(exponent));
    }
    dot
}

#[derive(Default, Clone, Copy)]
struct Metrics {
    square_error: f64,
    dot_square_error: f64,
    weights: usize,
    dots: usize,
}

impl Metrics {
    fn add_row(&mut self, original: &[f64; N], reconstructed: &[f64; N]) {
        for (&a, &b) in original.iter().zip(reconstructed) {
            let delta = b - a;
            self.square_error += delta * delta;
            self.weights += 1;
        }
    }
    fn add_dot(&mut self, original: f64, reconstructed: f64) {
        let delta = reconstructed - original;
        self.dot_square_error += delta * delta;
        self.dots += 1;
    }
    fn emit(&self, split: &str, name: &str, bytes: usize, modes: [usize; 3]) {
        assert_eq!(self.weights, ROWS * N);
        assert_eq!(self.dots, ROWS * MASKS);
        println!(
            "{split},{name},{bytes},{:.6},{:.12},{:.12},{},{},{},{},{}",
            bytes as f64 * 8.0 / self.weights as f64,
            (self.square_error / self.weights as f64).sqrt(),
            (self.dot_square_error / self.dots as f64).sqrt(),
            self.weights,
            self.dots,
            modes[0],
            modes[1],
            modes[2]
        );
    }
}

fn evaluate(split: &str, weights: &[[f32; N]]) {
    assert_eq!(weights.len(), ROWS);
    let blocks: Vec<[f64; 64]> = weights
        .iter()
        .flat_map(|row| {
            (0..4).map(move |g| std::array::from_fn(|i| f64::from(row[g * 64 + i])))
        })
        .collect();
    let fixed = adaptive::fixed_mode(&blocks, 2).unwrap();
    let chosen = adaptive::allocate(&blocks, ROWS * 108).unwrap();
    assert_eq!(fixed.total_bytes, ROWS * 108);
    assert_eq!(chosen.total_bytes, ROWS * 108);
    assert!(chosen.sse <= fixed.sse + 1e-10);
    let counts = [1, 2, 3].map(|m| chosen.choices.iter().filter(|c| c.mode == m).count());
    let mut stats = [Metrics::default(); 4];
    let mut rng = SEED ^ if split == "TEST" { 0x9e37_79b9_7f4a_7c15 } else { 0 };
    for row in 0..ROWS {
        let mut original = [0.0; N];
        for (i, &v) in weights[row].iter().enumerate() {
            original[i] = f64::from(v);
        }
        let a = decode_blocks(&fixed.wires[row * 4..row * 4 + 4]);
        let b = decode_blocks(&chosen.wires[row * 4..row * 4 + 4]);
        let c = official(GGML_Q3_K, &weights[row]);
        let d = official(GGML_Q4_K, &weights[row]);
        for (metric, values) in stats.iter_mut().zip([&a, &b, &c, &d]) {
            metric.add_row(&original, values);
        }
        for _ in 0..MASKS {
            // All four arms consume precisely the same original input and mask.
            let mask: [u64; 4] = std::array::from_fn(|_| random(&mut rng));
            let real = bipolar_dot(&original, &mask);
            let dots = [
                boolean_dot(&fixed.wires[row * 4..row * 4 + 4], &mask),
                boolean_dot(&chosen.wires[row * 4..row * 4 + 4], &mask),
                bipolar_dot(&c, &mask),
                bipolar_dot(&d, &mask),
            ];
            for (metric, result) in stats.iter_mut().zip(dots) {
                metric.add_dot(real, result);
            }
        }
    }
    for (metric, (name, bytes, modes)) in stats.iter().zip([
        ("fixed_3bit_tagged", ROWS * 108, [0, ROWS * 4, 0]),
        ("adaptive_2to4bit", chosen.total_bytes, counts),
        ("GGUF_Q3_K", ROWS * gguf::Q3_BYTES, [0, 0, 0]),
        ("GGUF_Q4_K", ROWS * gguf::Q4_BYTES, [0, 0, 0]),
    ]) {
        metric.emit(split, name, bytes, modes);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 || args[1] != "--external-f32le" {
        eprintln!("usage: bpb3-adaptive-a3 --external-f32le pinned-48x256-f32le");
        std::process::exit(2);
    }
    let bytes = fs::read(Path::new(&args[2])).expect("STOP_A3_SOURCE_FILE_UNREADABLE");
    assert_eq!(bytes.len(), 48 * N * 4, "STOP_A3_WRONG_SOURCE_SIZE");
    let weights: Vec<[f32; N]> = bytes
        .chunks_exact(N * 4)
        .map(|chunk| {
            std::array::from_fn(|i| {
                f32::from_le_bytes(chunk[i * 4..i * 4 + 4].try_into().unwrap())
            })
        })
        .collect();
    assert_eq!(weights.len(), 48);
    assert!(
        weights.iter().all(|row| row.iter().all(|v| v.is_finite())),
        "STOP_A3_NONFINITE_SOURCE"
    );
    println!("protocol=bpb3-a3-smollm2-135m-v1,model_integrated=false,source=EXTERNALLY_SHA_VERIFIED,masks=16_shared_per_row,precision_budget=108B_per256");
    println!("split,codec,payload_bytes,bits_per_weight,weight_rmse,dot_rmse,coefficients,dots,mode1_blocks,mode2_blocks,mode3_blocks");
    evaluate("DEV", &weights[..24]);
    evaluate("TEST", &weights[24..]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_and_boolean_dots_share_an_exact_signed_oracle() {
        let block = std::array::from_fn(|i| (i % 7) as i8 - 3);
        let mut rng = SEED;
        for mode in 1..=3 {
            let maximum = (1 << mode) - 1;
            let codes = block.map(|v| v.clamp(-maximum, maximum));
            let wire = adaptive::pack(&codes, -3, mode as u8).unwrap();
            let wires = vec![wire.clone(), wire.clone(), wire.clone(), wire];
            let reconstructed = decode_blocks(&wires);
            for _ in 0..MASKS {
                let mask: [u64; 4] = std::array::from_fn(|_| random(&mut rng));
                let actual = boolean_dot(&wires, &mask);
                let dense = bipolar_dot(&reconstructed, &mask);
                assert_eq!(actual, dense, "mode={mode}");
            }
        }
    }

    #[test]
    fn pooled_rmse_uses_sse_not_mean_rmse() {
        let one = Metrics {
            square_error: 9.0,
            dot_square_error: 0.0,
            weights: 1,
            dots: 1,
        };
        let two = Metrics {
            square_error: 0.0,
            dot_square_error: 0.0,
            weights: 3,
            dots: 1,
        };
        let pooled = ((one.square_error + two.square_error)
            / (one.weights + two.weights) as f64)
            .sqrt();
        assert_eq!(pooled, 1.5);
    }
}
