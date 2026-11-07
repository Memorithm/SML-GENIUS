//! Issue #853 — deterministic, model-independent E1 reconstruction baseline.
//! The GGUF K *encoders* are the official, pinned ggml C implementation,
//! dynamically linked at CI build time. Decode uses the Rust byte oracle.
//! BPB-3 is evaluated on the same original f32 weights in four 64-wide groups.
//! No trained model, DRAM/GPU timing, or real-weight quality claim is made.
#![allow(dead_code)]
#[path = "gguf_k.rs"]
mod gguf;
#[path = "prototype.rs"]
mod bpb3;

use std::ffi::{c_char, c_void, CStr};
use std::fs;
use std::path::PathBuf;

const N: usize = 256;
const ROWS: usize = 24;
const MASKS: usize = 16;
const FAMILY: [&str; 4] = ["uniform", "sparse", "outlier", "logspread"];
const SPLITS: [&str; 2] = ["DEV", "TEST"];
const GGML_Q3_K: i32 = 11;
const GGML_Q4_K: i32 = 12;

#[link(name = "ggml-base")]
unsafe extern "C" {
    fn ggml_type_name(ty: i32) -> *const c_char;
    fn ggml_blck_size(ty: i32) -> i64;
    fn ggml_type_size(ty: i32) -> usize;
    fn ggml_quantize_chunk(
        ty: i32, src: *const f32, dst: *mut c_void,
        start: i64, nrows: i64, n_per_row: i64, imatrix: *const f32
    ) -> usize;
    fn dequantize_row_q3_K(x: *const c_void, y: *mut f32, k: i64);
    fn dequantize_row_q4_K(x: *const c_void, y: *mut f32, k: i64);
}

fn next_random(seed: &mut u64) -> u64 {
    *seed ^= *seed >> 12;
    *seed ^= *seed << 25;
    *seed ^= *seed >> 27;
    (*seed).wrapping_mul(2685821657736338717)
}

fn unit(seed: &mut u64) -> f32 {
    ((next_random(seed) >> 40) as f32) / ((1u32 << 24) as f32)
}

fn generated(family: &str, seed: &mut u64) -> [f32; N] {
    std::array::from_fn(|i| {
        let base = 2.0 * unit(seed) - 1.0;
        match family {
            "uniform" => 0.85 * base,
            "sparse" => if next_random(seed) % 5 == 0 { 0.7 * base } else { 0.0 },
            "outlier" => if i % 31 == 0 { 8.0 * base } else { 0.12 * base },
            "logspread" => {
                let k = (next_random(seed) % 13) as i32 - 12;
                base.signum() * 2.0f32.powi(k)
            }
            _ => panic!("unknown synthetic weight family"),
        }
    })
}

fn official(ty: i32, original: &[f32; N]) -> Vec<u8> {
    let (name, size) = if ty == GGML_Q3_K {
        ("q3_K", gguf::Q3_BYTES)
    } else if ty == GGML_Q4_K {
        ("q4_K", gguf::Q4_BYTES)
    } else {
        panic!("STOP_UNSUPPORTED_QUANTIZER");
    };
    let actual_name = unsafe { CStr::from_ptr(ggml_type_name(ty)) };
    assert_eq!(actual_name.to_str().unwrap(), name, "STOP_GGML_TYPE_ABI");
    assert_eq!(unsafe { ggml_blck_size(ty) }, N as i64);
    assert_eq!(unsafe { ggml_type_size(ty) }, size);
    let mut encoded = vec![0u8; size];
    let wrote = unsafe {
        ggml_quantize_chunk(
            ty, original.as_ptr(), encoded.as_mut_ptr().cast::<c_void>(),
            0, 1, N as i64, std::ptr::null()
        )
    };
    assert_eq!(wrote, size, "STOP_GGML_QUANTIZER_WRONG_WIRE_SIZE");
    encoded
}

fn decode_official_c(ty: i32, wire: &[u8]) -> [f32; N] {
    let mut values = [0f32; N];
    unsafe {
        if ty == GGML_Q3_K {
            dequantize_row_q3_K(wire.as_ptr().cast::<c_void>(), values.as_mut_ptr(), N as i64);
        } else {
            dequantize_row_q4_K(wire.as_ptr().cast::<c_void>(), values.as_mut_ptr(), N as i64);
        }
    }
    values
}

fn check_c_agreement(ty: i32, bytes: &[u8], rust: &[f32; N]) {
    let c = decode_official_c(ty, bytes);
    for i in 0..N {
        assert!(c[i].is_finite() && rust[i].is_finite(), "STOP_NONFINITE_DECODE");
        let tolerance = 0.00001f32 * c[i].abs().max(1.0);
        assert!((c[i] - rust[i]).abs() <= tolerance,
            "STOP_OFFICIAL_CPP_RUST_PARITY index={i} c={} rust={}", c[i], rust[i]);
    }
}

/// Exact same BPB-3 scale-selection rule as quantization_panel.rs.
/// These original floats are converted into quantized codes before packing.
fn encode_bpb(original: &[f32; N]) -> ([f32; N], usize) {
    let mut reconstructed = [0.0f32; N];
    for block in 0..4 {
        let slice = &original[block * 64..(block + 1) * 64];
        let mut best = [0i8; 64];
        let mut best_error = f64::INFINITY;
        let mut best_k = 0i8;
        for k in -24i8..=27 {
            let scale = 2.0f64.powi(i32::from(k));
            let mut candidate = [0i8; 64];
            let mut sq = 0.0;
            for (i, &value) in slice.iter().enumerate() {
                let v = f64::from(value);
                let q = (v / scale).round().clamp(-3.0, 3.0) as i8;
                candidate[i] = q;
                sq += (v - f64::from(q) * scale).powi(2);
            }
            if sq < best_error {
                best_error = sq;
                best_k = k;
                best = candidate;
            }
        }
        let packed = bpb3::Bpb3::pack(&best, best_k).expect("legal BPB3 scale/weight");
        assert_eq!(packed.to_bytes().len(), 26);
        let unpacked = packed.unpack();
        assert_eq!(unpacked, best);
        let scale = 2.0f32.powi(i32::from(best_k));
        for (i, q) in unpacked.iter().enumerate() {
            reconstructed[block * 64 + i] = f32::from(*q) * scale;
        }
    }
    (reconstructed, 104)
}

/// Round signed finite f32 to IEEE binary16 (nearest/ties-even).
fn to_f16(value: f32) -> u16 {
    assert!(value.is_finite(), "STOP_RESIDUAL_NOT_FINITE");
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exponent = ((bits >> 23) & 255) as i32 - 127;
    let mant = bits & 0x7fffff;
    if exponent > 15 {
        panic!("STOP_RESIDUAL_HALF_OVERFLOW");
    }
    if exponent < -25 {
        return sign;
    }
    let (shift, significand, bias) = if exponent >= -14 {
        (13u32, mant, ((exponent + 15) as u16) << 10)
    } else {
        ((-exponent - 1) as u32, mant | (1 << 23), 0u16)
    };
    let truncated = significand >> shift;
    let lower = significand & ((1u32 << shift) - 1);
    let halfway = 1u32 << (shift - 1);
    let carry = u32::from(lower > halfway || (lower == halfway && truncated & 1 != 0));
    sign | bias.wrapping_add((truncated + carry) as u16)
}

fn from_f16(code: u16) -> f32 {
    let sign = if code & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exp = (code >> 10) & 31;
    let frac = code & 1023;
    match exp {
        0 => sign * f32::from(frac) * 2.0f32.powi(-24),
        31 if frac == 0 => sign * f32::INFINITY,
        31 => f32::NAN,
        _ => sign * (1.0 + f32::from(frac) / 1024.0) * 2.0f32.powi(i32::from(exp) - 15),
    }
}

/// Pre-registered E2 feasibility candidate: 2 indexed f16 additive residuals
/// per 256 values: 4 × 26-byte BPB3 + 2 × (u8 index, f16 residual) = 110 B.
/// No search over a hyperparameter, no TEST-based tuning.
fn bpb_plus_two(original: &[f32; N], baseline: &[f32; N]) -> [f32; N] {
    let mut ranked: Vec<(usize, f32)> = original.iter().zip(baseline)
        .enumerate().map(|(i, (&w, &q))| (i, (w - q).abs())).collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut bytes = [0u8; 6];
    for slot in 0..2 {
        let index = ranked[slot].0;
        let delta = original[index] - baseline[index];
        let packed = to_f16(delta).to_le_bytes();
        bytes[slot * 3] = index as u8;
        bytes[slot * 3 + 1..slot * 3 + 3].copy_from_slice(&packed);
    }
    let mut recovered = *baseline;
    for slot in 0..2 {
        let index = usize::from(bytes[slot * 3]);
        let code = u16::from_le_bytes([bytes[slot * 3 + 1], bytes[slot * 3 + 2]]);
        recovered[index] += from_f16(code);
    }
    recovered
}

fn quantile(sorted: &[f64], fraction: f64) -> f64 {
    let index = ((sorted.len() as f64 * fraction).ceil() as usize).saturating_sub(1);
    sorted[index.min(sorted.len() - 1)]
}

fn dot(w: &[f32; N], mask: &[f32; N]) -> f64 {
    w.iter().zip(mask).map(|(&x, &a)| f64::from(x) * f64::from(a)).sum()
}

fn report(split: &str, family: &str, codec: &str, bytes: usize,
          original: &[[f32; N]], reconstructed: &[[f32; N]], seed: u64) {
    let mut errors = Vec::with_capacity(original.len() * N);
    let mut ss = 0.0f64;
    let mut abs_sum = 0.0f64;
    let mut signed = 0.0f64;
    let mut original_ss = 0.0f64;
    let mut reconstructed_ss = 0.0f64;
    let mut cross = 0.0f64;
    let mut dot_errors = Vec::with_capacity(original.len() * MASKS);
    let mut zcount = 0usize;
    let mut zabs = 0.0;
    let mut worst = (0.0f64, 0usize);
    let mut rng = seed;
    for (row, (reference, estimate)) in original.iter().zip(reconstructed).enumerate() {
        for (i, (&v, &q)) in reference.iter().zip(estimate).enumerate() {
            let error = f64::from(q) - f64::from(v);
            let a = error.abs();
            errors.push(a);
            ss += error * error;
            abs_sum += a;
            signed += error;
            original_ss += f64::from(v).powi(2);
            reconstructed_ss += f64::from(q).powi(2);
            cross += f64::from(v) * f64::from(q);
            if v == 0.0 { zcount += 1; zabs += a; }
            if a > worst.0 { worst = (a, row * N + i); }
        }
        for _ in 0..MASKS {
            let signs = std::array::from_fn(|_| {
                if next_random(&mut rng) & 1 == 0 { 1.0f32 } else { -1.0f32 }
            });
            dot_errors.push((dot(estimate, &signs) - dot(reference, &signs)).powi(2));
        }
    }
    errors.sort_by(|a, b| a.total_cmp(b));
    let n = errors.len() as f64;
    let rmse = (ss / n).sqrt();
    let norm = (original_ss / n).sqrt();
    let cosine = if original_ss > 0.0 && reconstructed_ss > 0.0 {
        cross / (original_ss * reconstructed_ss).sqrt()
    } else { 0.0 };
    println!("{split},{family},{codec},{},{:.6},{},{:.9},{:.9},{:.9},{:.9},{:.9},{:.9},{:.9},{:.9},{},{:.9},{:.9},{:.9},{},{}",
        original.len(), 8.0 * bytes as f64 / N as f64, bytes,
        rmse, if norm > 0.0 { rmse / norm } else { 0.0 },
        abs_sum / n, signed / n, cosine,
        quantile(&errors, 0.95), quantile(&errors, 0.99), quantile(&errors, 0.999),
        worst.1, worst.0,
        (dot_errors.iter().sum::<f64>() / dot_errors.len() as f64).sqrt(),
        if zcount > 0 { zabs / zcount as f64 } else { 0.0 },
        n as usize, dot_errors.len());
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let input: Option<PathBuf> = match args.as_slice() {
        [] => None,
        [a, file] if a == "--external-f32le" => Some(PathBuf::from(file)),
        _ => panic!("STOP_USAGE: [--external-f32le /path/to/original.f32le]"),
    };
    let mode = if input.is_some() { "EXTERNAL_UNVERIFIED" } else { "SYNTHETIC" };
    println!("protocol=bpb3-ggml-original-f32-e1-v1,dataset_kind={mode},ggml_pin=f7591a6a01f9273de03331732bdc0b39721168be,optimizer=e2_top2_f16_no_dev_tuning,model_integrated=false");
    println!("split,family,codec,rows,bpw,wire_bytes_per_256,rmse,nrmse_original_rms,mae,signed_bias,cosine,p95_abs,p99_abs,p999_abs,worst_index,max_abs,dot_rmse,zero_abs_mae,coefficients,dots");
    let external = input.map(|path| fs::read(path).expect("STOP_EXTERNAL_WEIGHT_FILE"));
    if let Some(raw) = &external {
        assert_eq!(raw.len() % (N * 4), 0, "STOP_UNALIGNED_EXTERNAL_FP32");
        assert!(raw.len() >= 2 * N * 4, "STOP_INSUFFICIENT_EXTERNAL_BLOCKS");
    }
    for (si, split) in SPLITS.iter().enumerate() {
        for (fi, family) in FAMILY.iter().enumerate() {
            if external.is_some() && (fi != 0 || si > 1) { continue; }
            let mut seed = 0x7aa5_dafa_9283_4161u64
                ^ ((si as u64 + 1) << 48) ^ ((fi as u64 + 1) << 32);
            let rows: Vec<[f32; N]> = if let Some(raw) = &external {
                let total = raw.len() / (N * 4);
                let midpoint = total / 2;
                let (start, end) = if si == 0 { (0, midpoint) } else { (midpoint, total) };
                (start..end).map(|row| {
                    std::array::from_fn(|i| {
                        let k = (row * N + i) * 4;
                        f32::from_le_bytes(raw[k..k + 4].try_into().unwrap())
                    })
                }).collect()
            } else {
                (0..ROWS).map(|_| generated(family, &mut seed)).collect()
            };
            for row in &rows {
                assert!(row.iter().all(|x| x.is_finite()), "STOP_NONFINITE_ORIGINAL");
            }
            let mut q3 = Vec::new();
            let mut q4 = Vec::new();
            let mut bpb = Vec::new();
            let mut residual = Vec::new();
            for original in &rows {
                let w3 = official(GGML_Q3_K, original);
                let w4 = official(GGML_Q4_K, original);
                let d3 = gguf::decode_q3_k(&w3).expect("GGUF Q3 byte decoder");
                let d4 = gguf::decode_q4_k(&w4).expect("GGUF Q4 byte decoder");
                check_c_agreement(GGML_Q3_K, &w3, &d3);
                check_c_agreement(GGML_Q4_K, &w4, &d4);
                let (bp, count) = encode_bpb(original);
                assert_eq!(count, 104);
                residual.push(bpb_plus_two(original, &bp));
                bpb.push(bp);
                q3.push(d3);
                q4.push(d4);
            }
            let label = if external.is_some() { "external" } else { family };
            let row_seed = seed ^ 0x789a_ffab_a5a5_5a5a;
            report(split, label, "BPB-3", 104, &rows, &bpb, row_seed);
            report(split, label, "BPB-3+2xFP16_RESIDUAL", 110, &rows, &residual, row_seed);
            report(split, label, "GGUF_Q3_K", gguf::Q3_BYTES, &rows, &q3, row_seed);
            report(split, label, "GGUF_Q4_K", gguf::Q4_BYTES, &rows, &q4, row_seed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn f16_rounding_and_signed_zero() {
        for (value, code) in [(0.0, 0), (-0.0, 0x8000), (1.0, 0x3c00),
                              (0.5, 0x3800), (1.5, 0x3e00), (65504.0, 0x7bff),
                              (2.0f32.powi(-24), 1), (2.0f32.powi(-14), 0x400)] {
            assert_eq!(to_f16(value), code, "f16 encode {value:?}");
            assert_eq!(from_f16(code).to_bits(), value.to_bits());
        }
        assert_eq!(to_f16(1.0 + 2.0f32.powi(-11)), 0x3c00, "nearest ties-even");
    }
    #[test]
    fn two_residuals_have_matching_q3_wire_budget_and_reduce_error() {
        let original = std::array::from_fn(|i| {
            if i == 3 { 7.5 } else if i == 241 { -6.0 } else { 0.002 * i as f32 }
        });
        let (b, cost) = encode_bpb(&original);
        assert_eq!(cost + 2 * (1 + 2), gguf::Q3_BYTES);
        let better = bpb_plus_two(&original, &b);
        let e0: f64 = original.iter().zip(b).map(|(&w,q)| f64::from(w-q).powi(2)).sum();
        let e1: f64 = original.iter().zip(better).map(|(&w,q)| f64::from(w-q).powi(2)).sum();
        assert!(e1 <= e0, "top residuals must not increase square error");
    }
    #[test]
    fn deterministic_dev_test_are_disjoint() {
        let mut d = 0x7aa5_dafa_9283_4161u64 ^ (1u64 << 48) ^ (1u64 << 32);
        let mut t = 0x7aa5_dafa_9283_4161u64 ^ (2u64 << 48) ^ (1u64 << 32);
        assert_ne!(generated("uniform", &mut d), generated("uniform", &mut t));
    }
}
