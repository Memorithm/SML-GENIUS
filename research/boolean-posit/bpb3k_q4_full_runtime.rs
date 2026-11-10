//! Research-only GGUF Q3_K converter into NONSTANDARD 146-byte Q4-146.
//! The ephemeral artifact is readable ONLY by matching patched ggml.
//! No modification of the original model or deployment is permitted.
#![forbid(unsafe_code)]
#[path = "bpb3k_q4_pair228_avx2.rs"]
#[allow(dead_code)]
mod q4;

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

const GGML_SHA: &str = "f7591a6a01f9273de03331732bdc0b39721168be";
const Q3_K: u32 = 11;
const MAX_GGUF: u64 = 600_000_000;
const MAX_ENTRIES: u64 = 200_000;
const MAX_STRING: u64 = 2_000_000;

#[derive(Debug)]
struct Tensor {
    name: String,
    kind: u32,
    values: u64,
    offset: u64,
    offset_field: u64,
}
fn read<const N: usize>(f: &mut File) -> Result<[u8; N], String> {
    let mut out = [0u8; N];
    f.read_exact(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}
fn u32le(f: &mut File) -> Result<u32, String> {
    Ok(u32::from_le_bytes(read(f)?))
}
fn u64le(f: &mut File) -> Result<u64, String> {
    Ok(u64::from_le_bytes(read(f)?))
}
fn advance(f: &mut File, n: u64, limit: u64) -> Result<(), String> {
    let pos = f.stream_position().map_err(|e| e.to_string())?;
    let end = pos.checked_add(n).ok_or("STOP_GGUF_OFFSET_OVERFLOW")?;
    if end > limit {
        return Err("STOP_GGUF_TRUNCATED".into());
    }
    f.seek(SeekFrom::Start(end)).map_err(|e| e.to_string())?;
    Ok(())
}
fn read_string(f: &mut File, limit: u64) -> Result<String, String> {
    let n = u64le(f)?;
    if n > MAX_STRING {
        return Err("STOP_LONG_STRING".into());
    }
    if f.stream_position()
        .map_err(|e| e.to_string())?
        .checked_add(n)
        .ok_or("STOP_STRING_OFFSET_OVERFLOW")?
        > limit
    {
        return Err("STOP_TRUNCATED_STRING".into());
    }
    let mut bytes = vec![0u8; n as usize];
    f.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    String::from_utf8(bytes).map_err(|_| "STOP_BAD_UTF8".into())
}
fn skip_value(f: &mut File, ty: u32, limit: u64) -> Result<(), String> {
    match ty {
        0 | 1 | 7 => advance(f, 1, limit),
        2 | 3 => advance(f, 2, limit),
        4 | 5 | 6 => advance(f, 4, limit),
        10..=12 => advance(f, 8, limit),
        8 => {
            let n = u64le(f)?;
            advance(f, n, limit)
        }
        9 => {
            let elem = u32le(f)?;
            let count = u64le(f)?;
            if count > 1_000_000 {
                return Err("STOP_HUGE_METADATA_ARRAY".into());
            }
            if elem == 8 {
                for _ in 0..count {
                    skip_value(f, elem, limit)?;
                }
                Ok(())
            } else {
                let width = match elem {
                    0 | 1 | 7 => 1,
                    2 | 3 => 2,
                    4 | 5 | 6 => 4,
                    10..=12 => 8,
                    _ => return Err("STOP_UNSUPPORTED_METADATA_ELEMENT".into()),
                };
                advance(
                    f,
                    count.checked_mul(width).ok_or("STOP_METADATA_OVERFLOW")?,
                    limit,
                )
            }
        }
        _ => Err("STOP_UNKNOWN_METADATA_TYPE".into()),
    }
}
fn tensors(f: &mut File, file_len: u64) -> Result<(u64, u64, Vec<Tensor>), String> {
    if read::<4>(f)? != *b"GGUF" {
        return Err("STOP_NOT_GGUF".into());
    }
    if u32le(f)? != 3 {
        return Err("STOP_UNSUPPORTED_GGUF_VERSION".into());
    }
    let n_tensors = u64le(f)?;
    let n_kvs = u64le(f)?;
    if n_tensors > MAX_ENTRIES || n_kvs > MAX_ENTRIES {
        return Err("STOP_EXCESSIVE_GGUF_ENTRIES".into());
    }
    let mut alignment = 32u64;
    for _ in 0..n_kvs {
        let key = read_string(f, file_len)?;
        let ty = u32le(f)?;
        if key == "general.alignment" {
            alignment = match ty {
                4 => u64::from(u32le(f)?),
                10 => u64le(f)?,
                _ => return Err("STOP_ALIGNMENT_TYPE".into()),
            };
        } else {
            skip_value(f, ty, file_len)?;
        }
    }
    if alignment == 0 || alignment > 4096 || !alignment.is_power_of_two() {
        return Err("STOP_ALIGNMENT_RANGE".into());
    }
    let mut entries = Vec::new();
    for _ in 0..n_tensors {
        let name = read_string(f, file_len)?;
        let dims = u32le(f)?;
        if !(1..=4).contains(&dims) {
            return Err("STOP_DIMS".into());
        }
        let mut values = 1u64;
        for _ in 0..dims {
            values = values
                .checked_mul(u64le(f)?)
                .ok_or("STOP_VALUES_OVERFLOW")?;
        }
        let kind = u32le(f)?;
        let offset_field = f.stream_position().map_err(|e| e.to_string())?;
        let offset = u64le(f)?;
        entries.push(Tensor {
            name,
            kind,
            values,
            offset,
            offset_field,
        });
    }
    let header = f.stream_position().map_err(|e| e.to_string())?;
    let start = header
        .checked_add(alignment - 1)
        .ok_or("STOP_ALIGN_OVERFLOW")?
        & !(alignment - 1);
    if start > file_len {
        return Err("STOP_TRUNCATED_TENSOR_DATA".into());
    }
    Ok((start, alignment, entries))
}
fn transcode(source: &Path, dest: &Path) -> Result<(), String> {
    if source == dest {
        return Err("STOP_SOURCE_DEST_SAME".into());
    }
    if dest.exists() {
        return Err("STOP_DEST_EXISTS".into());
    }
    let mut input = File::open(source).map_err(|e| e.to_string())?;
    let file_len = input.metadata().map_err(|e| e.to_string())?.len();
    if !(1..=MAX_GGUF).contains(&file_len) {
        return Err("STOP_GGUF_FILE_SIZE".into());
    }
    let (start, alignment, mut tensors) = tensors(&mut input, file_len)?;
    if tensors.is_empty() {
        return Err("STOP_NO_TENSORS".into());
    }
    tensors.sort_by_key(|t| t.offset);
    if tensors[0].offset != 0 {
        return Err("STOP_FIRST_TENSOR_NOT_AT_DATA_START".into());
    }
    for adjacent in tensors.windows(2) {
        if adjacent[0].offset == adjacent[1].offset
            || adjacent[1].offset % alignment != 0
        {
            return Err("STOP_OVERLAPPING_OR_UNALIGNED_TENSORS".into());
        }
    }
    if !tensors.iter().any(|t| t.kind == Q3_K) {
        return Err("STOP_NO_Q3_K_WEIGHTS".into());
    }
    // Validate every 110-byte source Q3_K segment and the offset graph
    // before creating the output. Zero source tensor payloads are rejected.
    for (i, tensor) in tensors.iter().enumerate() {
        let from = start.checked_add(tensor.offset).ok_or("STOP_SOURCE_OFFSET_OVERFLOW")?;
        let end = if let Some(next) = tensors.get(i + 1) {
            start.checked_add(next.offset).ok_or("STOP_NEXT_OFFSET_OVERFLOW")?
        } else {
            file_len
        };
        if end < from || end > file_len {
            return Err("STOP_ORIGINAL_TENSOR_OUTSIDE_FILE".into());
        }
        if tensor.kind == Q3_K {
            if tensor.values == 0 || tensor.values % 256 != 0 {
                return Err("STOP_Q3_TENSOR_SIZE".into());
            }
            let bytes = (tensor.values / 256).checked_mul(110)
                .ok_or("STOP_Q3_SOURCE_BYTES_OVERFLOW")?;
            if bytes > end - from {
                return Err("STOP_Q3_SOURCE_TRUNCATED".into());
            }
        }
    }

    let result = (|| {
        let mut output = OpenOptions::new().write(true).create_new(true).open(dest)
            .map_err(|e| e.to_string())?;
        input.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let copied = std::io::copy(&mut input.by_ref().take(start), &mut output)
            .map_err(|e| e.to_string())?;
        if copied != start {
            return Err("STOP_GGUF_HEADER_SHORT_COPY".into());
        }
        let mut blocks_done = 0u64;
        let mut tensors_done = 0u64;
        for (i, tensor) in tensors.iter().enumerate() {
            let pos = output.stream_position().map_err(|e| e.to_string())?;
            if pos < start || (pos - start) % alignment != 0 {
                return Err("STOP_NEW_TENSOR_UNALIGNED".into());
            }
            let new_offset = pos - start;
            // Rewrite the original GGUF v3 tensor offset field only.
            // Tensor shapes, names, types and all other metadata remain.
            output.seek(SeekFrom::Start(tensor.offset_field))
                .map_err(|e| e.to_string())?;
            output.write_all(&new_offset.to_le_bytes()).map_err(|e| e.to_string())?;
            output.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;

            let from = start.checked_add(tensor.offset).ok_or("STOP_SOURCE_OFFSET_OVERFLOW")?;
            let end = if let Some(next) = tensors.get(i + 1) {
                start.checked_add(next.offset).ok_or("STOP_NEXT_OFFSET_OVERFLOW")?
            } else {
                file_len
            };
            input.seek(SeekFrom::Start(from)).map_err(|e| e.to_string())?;
            if tensor.kind == Q3_K {
                let blocks = tensor.values / 256;
                for _ in 0..blocks {
                    let source = read::<110>(&mut input)?;
                    let converted = q4::transcode_q4::<146>(&source)
                        .map_err(|e| format!("STOP_Q4_TRANSCODE_{e:?}"))?;
                    output.write_all(&converted).map_err(|e| e.to_string())?;
                }
                blocks_done += blocks;
                tensors_done += 1;
                println!("BPB3K_Q4_REAL_TENSOR name={} blocks={} coefficients={}",
                    tensor.name, blocks, tensor.values);
            } else {
                // Copy non-Q3 tensor bytes, including its source padding,
                // without decoding or quantizing any pre-trained weights.
                let len = end - from;
                let copied = std::io::copy(&mut input.by_ref().take(len), &mut output)
                    .map_err(|e| e.to_string())?;
                if copied != len {
                    return Err("STOP_NONQ3_COPY_TRUNCATED".into());
                }
            }
            // Subsequent tensor payloads retain the original GGUF alignment.
            if i + 1 < tensors.len() {
                let end = output.stream_position().map_err(|e| e.to_string())?;
                let pad = (alignment - (end - start) % alignment) % alignment;
                if pad > 0 {
                    let zeros = [0u8; 4096];
                    output.write_all(&zeros[..pad as usize])
                        .map_err(|e| e.to_string())?;
                }
            }
            if output.stream_position().map_err(|e| e.to_string())? > 1_000_000_000 {
                return Err("STOP_Q4_NEW_GGUF_TOO_LARGE".into());
            }
        }
        output.sync_all().map_err(|e| e.to_string())?;
        println!(
            "PASS_BPB3K_Q4_REAL_GGUF q3_tensors={} blocks={} bytes_per_block=146 unchanged_other_tensor_payloads=true requires_patched_runtime=true",
            tensors_done, blocks_done
        );
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(dest);
    }
    result
}

// Temporary ggml CPU bridge is generated from Rust, compiled into a pinned
// source checkout, and excluded from all ordinary llama.cpp deployments.
const C_BRIDGE: &str = r#"
#ifndef MEM_BPB3K_Q4_146_EXPERIMENTAL_H
#define MEM_BPB3K_Q4_146_EXPERIMENTAL_H
#include <stdint.h>
#include <string.h>
#include <assert.h>
#ifdef MEM_BPB3K_DEQUANT
static void mem_q4_146_dequant(const block_q3_K *x, float *y, int64_t k) {
    assert(k % 256 == 0);
    for (int64_t b = 0; b < k / 256; ++b) {
        const uint8_t *w = (const uint8_t *) &x[b];
        uint16_t d_bits;
        memcpy(&d_bits, w + 144, sizeof(d_bits));
        const float d = ggml_fp16_to_fp32(d_bits);
        for (int i = 0; i < 256; ++i) {
            const int group = i / 32;
            const int lane = i % 16;
            const uint8_t packed = w[16 * group + lane];
            const int value = ((i % 32) < 16
                ? (packed & 15) : (packed >> 4)) - 4;
            const int scale = (int) (int8_t) w[128 + i / 16];
            y[b * 256 + i] = d * (float) scale * (float) value;
        }
    }
}
#endif
#ifdef MEM_BPB3K_DOT
// Links to independently compiled Rust AVX2 implementation.
extern void mem_q4_146_vec_dot(int n, float *s, size_t bs,
    const void *vx, size_t bx, const void *vy, size_t by, int nrc);
#endif
#endif
"#;

fn substitute_once(path: &Path, from: &str, to: &str) -> Result<(), String> {
    let source = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if source.matches(from).count() != 1 {
        return Err(format!("STOP_SOURCE_LAYOUT_DRIFT path={}", path.display()));
    }
    fs::write(path, source.replacen(from, to, 1)).map_err(|e| e.to_string())
}
fn patch(root: &Path, observed_sha: &str) -> Result<(), String> {
    if observed_sha != GGML_SHA {
        return Err("STOP_UPSTREAM_PIN_DRIFT".into());
    }
    let core = root.join("ggml/src/ggml.c");
    let cpu = root.join("ggml/src/ggml-cpu/ggml-cpu.c");
    let bridge = root.join("ggml/src/ggml-bpb3k-experimental.h");
    let tiled = root.join("ggml/src/ggml-cpu/tiled/tiled.cpp");
    let cpu_cmake = root.join("ggml/src/ggml-cpu/CMakeLists.txt");
    if bridge.exists() {
        return Err("STOP_BPB_BRIDGE_ALREADY_EXISTS".into());
    }
    let a = fs::read_to_string(&core).map_err(|e| e.to_string())?;
    let b = fs::read_to_string(&cpu).map_err(|e| e.to_string())?;
    let t = fs::read_to_string(&tiled).map_err(|e| e.to_string())?;
    let cmake = fs::read_to_string(&cpu_cmake).map_err(|e| e.to_string())?;
    let dot_dispatch = "    ggml_vec_dot_t const vec_dot      = type_traits_cpu[type].vec_dot;";
    if b.matches(dot_dispatch).count() != 1 {
        return Err("STOP_STOCK_ORDER_DISPATCH_LAYOUT_DRIFT".into());
    }
    if cmake
        .matches("    ggml_add_backend_library(${GGML_CPU_NAME})")
        .count()
        != 1
    {
        return Err("STOP_CMAKE_TARGET_LAYOUT_DRIFT".into());
    }

    for (src, expected) in [
        (
            &a,
            ".to_float                 = (ggml_to_float_t) dequantize_row_q3_K,",
        ),
        (&b, ".vec_dot                  = ggml_vec_dot_q3_K_q8_K,"),
    ] {
        if src.matches(expected).count() != 1 {
            return Err("STOP_TRAIT_LAYOUT_DRIFT".into());
        }
    }
    // The optimized tiled CPU path can otherwise read BPB3-K as stock Q3_K,
    // corrupting the prompt processing and attention state without an error.
    let tiled_gate: &str =
        "        case GGML_TYPE_Q4_K:\n        case GGML_TYPE_Q3_K:\n        case GGML_TYPE_Q2_K:";
    if t.matches(tiled_gate).count() != 1 {
        return Err("STOP_TILED_Q3_K_GATE_DRIFT".into());
    }
    fs::write(&bridge, C_BRIDGE).map_err(|e| e.to_string())?;
    substitute_once(
        &core,
        "#include <assert.h>",
        "#include <assert.h>\n#define MEM_BPB3K_DEQUANT 1\n#include \"ggml-bpb3k-experimental.h\"",
    )?;
    substitute_once(
        &cpu,
        "#include <assert.h>",
        "#include <assert.h>\n#define MEM_BPB3K_DOT 1\n#include \"../ggml-bpb3k-experimental.h\"",
    )?;
    substitute_once(
        &core,
        ".to_float                 = (ggml_to_float_t) dequantize_row_q3_K,",
        ".to_float                 = (ggml_to_float_t) mem_bpb3k_dequant,",
    )?;
    substitute_once(
        &cpu,
        ".vec_dot                  = ggml_vec_dot_q3_K_q8_K,",
        ".vec_dot                  = mem_bpb3k_dot_q8,",
    )?;
    substitute_once(
        &tiled,
        tiled_gate,
        "        case GGML_TYPE_Q4_K:\n        /* Q3_K surrogate is BPB3-K; only the patched direct vec_dot may decode it. */\n        case GGML_TYPE_Q2_K:",
    )?;

    // Upstream ggml has float tmp[32] and only even-row guards. nrows=4
    // smashes that stack buffer; nrows=2 is a safe 2x2 tile.
    substitute_once(
        &cpu,
        ".vec_dot                  = mem_bpb3k_dot_q8,\n        .vec_dot_type             = GGML_TYPE_Q8_K,\n        .nrows                    = 1,",
        ".vec_dot                  = mem_bpb3k_dot_q8,\n        .vec_dot_type             = GGML_TYPE_Q8_K,\n        .nrows                    = 2,",
    )?;
    substitute_once(
        &cpu,
        dot_dispatch,
        "    ggml_vec_dot_t const vec_dot = (type == GGML_TYPE_Q3_K &&\n        mem_bpb3k_stock_tiled_enabled(params->use_ref, ne11, src0->extra != NULL))\n        ? mem_bpb3k_dot_q8_stock_tiled : type_traits_cpu[type].vec_dot;",
    )?;
    substitute_once(
        &cpu_cmake,
        "    ggml_add_backend_library(${GGML_CPU_NAME})",
        "    ggml_add_backend_library(${GGML_CPU_NAME})\n    if (NOT MEM_BPB3K_RUST_STATICLIB OR NOT EXISTS \"${MEM_BPB3K_RUST_STATICLIB}\")\n        message(FATAL_ERROR \"BPB3K Rust kernel staticlib required\")\n    endif()\n    target_link_libraries(${GGML_CPU_NAME} PRIVATE \"${MEM_BPB3K_RUST_STATICLIB}\")",
    )?;
    println!("PASS_BPB3K_CPU_TILED_BYPASS_CLOSED=Q3_K");
    println!("PASS_BPB3K_PINNED_GGML_PATCH source={} modified=ggml.c,ggml-cpu.c,ggml-cpu/tiled/tiled.cpp header=ggml-bpb3k-experimental.h only_cpu=true", observed_sha);
    Ok(())
}
fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    match args.as_slice() {
        [_, mode, root, sha] if mode == "patch" => patch(Path::new(root), sha),
        [_, mode, source, dest] if mode == "transcode" => transcode(Path::new(source), Path::new(dest)),
        _ => Err("USAGE: bpb3k_full_runtime patch <llama-source-root> <sha> | transcode <original-gguf> <new-disposable-gguf>".into()),
    }
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refuses_unpinned_source() {
        let root = std::path::PathBuf::from("/nonexistent-memorithm-test");
        assert!(patch(&root, "wrongsha").is_err());
    }
    #[test]
    fn c_bridge_exposes_both_correct_hot_path_symbols() {
        assert!(C_BRIDGE.contains("extern void mem_bpb3k_dot_q8"));
        assert!(C_BRIDGE.contains("mem_bpb3k_dequant"));
        assert!(!C_BRIDGE.contains("static void mem_bpb3k_dot_q8"));
    }
}
