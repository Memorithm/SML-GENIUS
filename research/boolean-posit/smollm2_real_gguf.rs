//! Real trained-model GGUF Q3_K tensor gate (not full-model inference).
//! Research-only: direct BPB3-K vs original Q3_K bipolar dots on exact
//! trained weights. The timing comparator here is intentionally NOT ggml SIMD.
//! Nothing in this test establishes better tokens/s, perplexity or DRAM.
#![forbid(unsafe_code)]

#[path = "bpb3k.rs"]
#[allow(dead_code)]
mod bpb3k;
#[path = "bpb3k_direct.rs"]
#[allow(dead_code)]
mod bpb3k_direct;
#[path = "gguf_k.rs"]
#[allow(dead_code)]
mod gguf_k;
#[path = "q3k_direct.rs"]
#[allow(dead_code)]
mod q3k_direct;

use std::fs::File;
use std::hint::black_box;
use std::io::{Read, Seek, SeekFrom};
use std::time::Instant;

const Q3K_TYPE: u32 = 11;
const MAX_FILE_BYTES: u64 = 600_000_000;
const MAX_ENTRIES: u64 = 200_000;
const MAX_STRING_BYTES: u64 = 2_000_000;
const SAMPLES_PER_TENSOR: u64 = 512;
const MAX_TENSORS: usize = 3;
const ROUNDS: usize = 12;

#[derive(Debug)]
struct Tensor {
    name: String,
    blocks: u64,
    offset: u64,
}

fn number<const N: usize>(file: &mut File) -> Result<[u8; N], String> {
    let mut data = [0u8; N];
    file.read_exact(&mut data).map_err(|e| e.to_string())?;
    Ok(data)
}
fn u32le(file: &mut File) -> Result<u32, String> {
    Ok(u32::from_le_bytes(number(file)?))
}
fn u64le(file: &mut File) -> Result<u64, String> {
    Ok(u64::from_le_bytes(number(file)?))
}
fn skip(file: &mut File, bytes: u64, limit: u64) -> Result<(), String> {
    let next = file
        .stream_position()
        .map_err(|e| e.to_string())?
        .checked_add(bytes)
        .ok_or("STOP_POSITION_OVERFLOW")?;
    if next > limit {
        return Err("STOP_TRUNCATED_GGUF".into());
    }
    file.seek(SeekFrom::Start(next))
        .map_err(|e| e.to_string())?;
    Ok(())
}
fn string(file: &mut File, limit: u64) -> Result<String, String> {
    let len = u64le(file)?;
    if len > MAX_STRING_BYTES {
        return Err("STOP_OVERSIZED_GGUF_STRING".into());
    }
    let pos = file.stream_position().map_err(|e| e.to_string())?;
    if pos.checked_add(len).ok_or("STOP_OVERFLOW")? > limit {
        return Err("STOP_TRUNCATED_STRING".into());
    }
    let mut data = vec![0; len as usize];
    file.read_exact(&mut data).map_err(|e| e.to_string())?;
    String::from_utf8(data).map_err(|_| "STOP_BAD_STRING".into())
}
fn skip_value(file: &mut File, ty: u32, limit: u64) -> Result<(), String> {
    let size = match ty {
        0 | 1 | 7 => Some(1u64),
        2 | 3 => Some(2),
        4 | 5 | 6 => Some(4),
        10..=12 => Some(8),
        8 => {
            let n = u64le(file)?;
            skip(file, n, limit)?;
            return Ok(());
        }
        9 => {
            let child = u32le(file)?;
            let len = u64le(file)?;
            if len > 1_000_000 {
                return Err("STOP_METADATA_ARRAY_TOO_LARGE".into());
            }
            if child == 8 {
                for _ in 0..len {
                    skip_value(file, 8, limit)?;
                }
            } else {
                let width = match child {
                    0 | 1 | 7 => 1u64,
                    2 | 3 => 2,
                    4 | 5 | 6 => 4,
                    10..=12 => 8,
                    _ => return Err("STOP_BAD_ARRAY_TYPE".into()),
                };
                skip(
                    file,
                    len.checked_mul(width).ok_or("STOP_ARRAY_OVERFLOW")?,
                    limit,
                )?;
            }
            return Ok(());
        }
        _ => return Err("STOP_UNKNOWN_GGUF_METADATA_TYPE".into()),
    };
    skip(file, size.ok_or("STOP_NO_SIZE")?, limit)
}
fn align(value: u64, alignment: u64) -> Result<u64, String> {
    if alignment == 0 || alignment > 4096 || !alignment.is_power_of_two() {
        return Err("STOP_UNSUPPORTED_ALIGNMENT".into());
    }
    Ok(value
        .checked_add(alignment - 1)
        .ok_or("STOP_ALIGN_OVERFLOW")?
        & !(alignment - 1))
}
fn gguf_tensors(file: &mut File, limit: u64) -> Result<(u64, Vec<Tensor>), String> {
    if number::<4>(file)? != *b"GGUF" {
        return Err("STOP_NOT_GGUF".into());
    }
    let version = u32le(file)?;
    if version != 3 {
        return Err(format!("STOP_GGUF_VERSION={version}"));
    }
    let nt = u64le(file)?;
    let nk = u64le(file)?;
    if nt > MAX_ENTRIES || nk > MAX_ENTRIES {
        return Err("STOP_GGUF_HEADER_LIMIT".into());
    }
    let mut alignment = 32u64;
    for _ in 0..nk {
        let key = string(file, limit)?;
        let ty = u32le(file)?;
        if key == "general.alignment" {
            alignment = match ty {
                4 => u64::from(u32le(file)?),
                10 => u64le(file)?,
                _ => return Err("STOP_BAD_ALIGNMENT_TYPE".into()),
            };
        } else {
            skip_value(file, ty, limit)?;
        }
    }
    let mut tensors = Vec::new();
    for _ in 0..nt {
        let name = string(file, limit)?;
        let dimensions = u32le(file)?;
        if !(1..=4).contains(&dimensions) {
            return Err("STOP_BAD_TENSOR_DIMS".into());
        }
        let mut values = 1u64;
        for _ in 0..dimensions {
            values = values
                .checked_mul(u64le(file)?)
                .ok_or("STOP_TENSOR_OVERFLOW")?;
        }
        let kind = u32le(file)?;
        let offset = u64le(file)?;
        if kind == Q3K_TYPE && values >= 256 && values % 256 == 0 {
            tensors.push(Tensor {
                name,
                blocks: values / 256,
                offset,
            });
        }
    }
    let start = align(
        file.stream_position().map_err(|e| e.to_string())?,
        alignment,
    )?;
    if start > limit {
        return Err("STOP_INVALID_DATA_START".into());
    }
    Ok((start, tensors))
}
fn mask(index: u64, pattern: usize) -> [u64; 4] {
    std::array::from_fn(|lane| match pattern {
        0 => 0,
        1 => u64::MAX,
        2 => 0xaaaa_aaaa_aaaa_aaaa,
        3 => 0x5555_5555_5555_5555,
        _ => {
            let mut z = index
                .wrapping_mul(0x9e37_79b9_7f4a_7c15)
                .wrapping_add((lane as u64) << 32)
                .wrapping_add(pattern as u64);
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        }
    })
}
fn median(mut values: Vec<u128>) -> u128 {
    values.sort_unstable();
    values[values.len() / 2]
}
fn exercise(path: &str) -> Result<(), String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let limit = file.metadata().map_err(|e| e.to_string())?.len();
    if !(1..=MAX_FILE_BYTES).contains(&limit) {
        return Err("STOP_MODEL_SIZE_LIMIT".into());
    }
    let (base, tensors) = gguf_tensors(&mut file, limit)?;
    if tensors.is_empty() {
        return Err("STOP_NO_Q3K_TENSORS".into());
    }
    let mut raw_blocks: Vec<[u8; 110]> = Vec::new();
    for tensor in tensors.iter().take(MAX_TENSORS) {
        let count = SAMPLES_PER_TENSOR.min(tensor.blocks);
        let data_bytes = tensor
            .blocks
            .checked_mul(110)
            .ok_or("STOP_TENSOR_BYTES_OVERFLOW")?;
        let tensor_end = base
            .checked_add(tensor.offset)
            .and_then(|x| x.checked_add(data_bytes))
            .ok_or("STOP_TENSOR_BOUNDS_OVERFLOW")?;
        if tensor_end > limit {
            return Err("STOP_TENSOR_EXCEEDS_FILE".into());
        }
        println!(
            "REAL_MODEL_TENSOR name={} q3k_blocks={} sampled={}",
            tensor.name, tensor.blocks, count
        );
        for i in 0..count {
            let position = (u128::from(i) * u128::from(tensor.blocks) / u128::from(count)) as u64;
            let offset = base
                .checked_add(tensor.offset)
                .and_then(|x| x.checked_add(position * 110))
                .ok_or("STOP_SAMPLE_OVERFLOW")?;
            file.seek(SeekFrom::Start(offset))
                .map_err(|e| e.to_string())?;
            raw_blocks.push(number::<110>(&mut file)?);
        }
    }
    let begin_transcode = Instant::now();
    let bpb: Vec<[u8; 110]> = raw_blocks
        .iter()
        .map(|raw| bpb3k::q3_to_bpb3k(raw).map_err(|e| format!("STOP_TRANSCODE={e:?}")))
        .collect::<Result<_, _>>()?;
    let conversion_ns = begin_transcode.elapsed().as_nanos();
    let mut checked = 0usize;
    for (i, (q, b)) in raw_blocks.iter().zip(&bpb).enumerate() {
        if i < 32 {
            let before = gguf_k::decode_q3_k(q).map_err(|e| format!("{e:?}"))?;
            let after = bpb3k::decode_bpb3k(b).map_err(|e| format!("{e:?}"))?;
            for lane in 0..256 {
                if before[lane].to_bits() != after[lane].to_bits() {
                    return Err(format!(
                        "STOP_REAL_RECONSTRUCTION_MISMATCH block={i} lane={lane}"
                    ));
                }
            }
        }
        for pattern in 0..8 {
            let x = mask(i as u64, pattern);
            let original = q3k_direct::dot_q3k_bipolar(q, &x).map_err(|e| format!("{e:?}"))?;
            let boolean = bpb3k_direct::dot_bpb3k_bipolar(b, &x).map_err(|e| format!("{e:?}"))?;
            if original.weighted_integer != boolean.weighted_integer
                || original.global_scale_f16_bits != boolean.global_scale_f16_bits
            {
                return Err(format!(
                    "STOP_REAL_DOT_MISMATCH block={i} pattern={pattern}"
                ));
            }
            checked += 1;
        }
    }
    println!(
        "PASS_REAL_MODEL_ORACLE blocks={} bipolar_dots={} checked_f32_blocks={} conversion_ns={}",
        raw_blocks.len(),
        checked,
        raw_blocks.len().min(32),
        conversion_ns
    );
    let mut q_times = Vec::with_capacity(ROUNDS);
    let mut b_times = Vec::with_capacity(ROUNDS);
    for turn in 0..ROUNDS {
        for arm in 0..2 {
            let which = if turn % 2 == 0 { arm } else { 1 - arm };
            let t0 = Instant::now();
            let mut digest: i64 = 0;
            for (i, (q, b)) in raw_blocks.iter().zip(&bpb).enumerate() {
                for pattern in 0..8 {
                    let x = mask(i as u64, pattern);
                    let dot = if which == 0 {
                        q3k_direct::dot_q3k_bipolar(black_box(q), black_box(&x))
                            .map_err(|e| format!("{e:?}"))?
                            .weighted_integer
                    } else {
                        bpb3k_direct::dot_bpb3k_bipolar(black_box(b), black_box(&x))
                            .map_err(|e| format!("{e:?}"))?
                            .weighted_integer
                    };
                    digest = digest.wrapping_add(i64::from(dot));
                }
            }
            black_box(digest);
            let elapsed = t0.elapsed().as_nanos();
            println!(
                "REAL_WEIGHT_DIAGNOSTIC round={} arm={} elapsed_ns={} checksum={}",
                turn,
                if which == 0 {
                    "q3k_scalar"
                } else {
                    "bpb3k_popcount"
                },
                elapsed,
                digest
            );
            if which == 0 {
                q_times.push(elapsed);
            } else {
                b_times.push(elapsed);
            }
        }
    }
    println!("REAL_WEIGHT_DIAGNOSTIC_SUMMARY q3k_median_ns={} bpb3k_median_ns={} block_count={} status=NON_COMPETITIVE_MICROKERNEL_ONLY",
        median(q_times), median(b_times), raw_blocks.len());
    println!("STOP_MODEL_SUPERIORITY_NOT_QUALIFIED reason=no_optimized_ggml_baseline_no_nonbipolar_activations_no_end_to_end_BPB");
    Ok(())
}
fn main() {
    match std::env::args().nth(1) {
        Some(path) => {
            if let Err(e) = exercise(&path) {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        None => {
            eprintln!("usage: smollm2_real_gguf <sha256-verified-Q3_K_S.gguf>");
            std::process::exit(2);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn no_unsupported_alignment() {
        assert!(align(34, 32).is_ok_and(|x| x == 64));
        assert!(align(10, 3).is_err());
        assert!(align(u64::MAX, 32).is_err());
    }
    #[test]
    fn masks_are_deterministic_and_distinct() {
        assert_eq!(mask(1, 4), mask(1, 4));
        assert_ne!(mask(1, 4), mask(2, 4));
    }
}
2026-11-08T14:44:51.1470056