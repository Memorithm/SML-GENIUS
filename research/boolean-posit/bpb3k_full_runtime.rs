//! Research-only Rust GGUF BPB3-K adapter for a SHA-pinned llama.cpp CPU runtime.
//! A BPB3-K payload is NOT a standard GGUF Q3_K payload. Both the transformed
//! ephemeral GGUF and the matching patched ggml must be used together.
//! NEVER distribute the transformed file as a conforming Q3_K GGUF.
#![forbid(unsafe_code)]
#[path = "bpb3k.rs"]
#[allow(dead_code)]
mod bpb3k;

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
fn tensors(f: &mut File, file_len: u64) -> Result<(u64, Vec<Tensor>), String> {
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
        let offset = u64le(f)?;
        entries.push(Tensor {
            name,
            kind,
            values,
            offset,
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
    Ok((start, entries))
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
    let (start, all) = tensors(&mut input, file_len)?;
    let q3: Vec<_> = all.iter().filter(|x| x.kind == Q3_K).collect();
    if q3.is_empty() {
        return Err("STOP_NO_Q3_K_WEIGHTS".into());
    }
    for x in &q3 {
        if x.values == 0 || x.values % 256 != 0 {
            return Err("STOP_MISALIGNED_Q3_VALUES".into());
        }
        let bytes = (x.values / 256)
            .checked_mul(110)
            .ok_or("STOP_BLOCK_BYTES_OVERFLOW")?;
        let end = start
            .checked_add(x.offset)
            .and_then(|v| v.checked_add(bytes))
            .ok_or("STOP_BLOCK_END_OVERFLOW")?;
        if end > file_len {
            return Err(format!("STOP_TENSOR_OUT_OF_FILE name={}", x.name));
        }
    }
    // Prevalidate metadata before touching the destination. Always keep it transient.
    fs::copy(source, dest).map_err(|e| e.to_string())?;
    let result = (|| {
        let mut output = OpenOptions::new()
            .read(true)
            .write(true)
            .open(dest)
            .map_err(|e| e.to_string())?;
        let mut processed = 0u64;
        for tensor in &q3 {
            let blocks = tensor.values / 256;
            let at = start
                .checked_add(tensor.offset)
                .ok_or("STOP_TENSOR_OFFSET")?;
            input.seek(SeekFrom::Start(at)).map_err(|e| e.to_string())?;
            output
                .seek(SeekFrom::Start(at))
                .map_err(|e| e.to_string())?;
            for _ in 0..blocks {
                let original = read::<110>(&mut input)?;
                let converted = bpb3k::q3_to_bpb3k(&original).map_err(|e| format!("{e:?}"))?;
                output.write_all(&converted).map_err(|e| e.to_string())?;
            }
            processed += blocks;
            println!(
                "BPB3K_LIVE_TENSOR name={} blocks={} coefficients={}",
                tensor.name, blocks, tensor.values
            );
        }
        output.sync_all().map_err(|e| e.to_string())?;
        println!("PASS_BPB3K_REAL_GGUF all_q3_tensors={} blocks={} bytes_per_block=110 gguf_type_surrogate=Q3_K requires_patched_runtime=true", q3.len(), processed);
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
#ifndef MEMORITHM_EXPERIMENTAL_BPB3K_H
#define MEMORITHM_EXPERIMENTAL_BPB3K_H
#include <stdint.h>
#include <string.h>
#include <assert.h>
#include <stdio.h>
#include <stdatomic.h>

static uint64_t mem_bpb3k_load64(const uint8_t *v) {
    uint64_t x;
    memcpy(&x, v, 8);
#if defined(__BYTE_ORDER__) && __BYTE_ORDER__ == __ORDER_BIG_ENDIAN__
    x = __builtin_bswap64(x);
#endif
    return x;
}
static int mem_bpb3k_subscale(const uint8_t *sc, int g) {
    unsigned low = g < 8 ? (sc[g] & 15) : (sc[g - 8] >> 4);
    unsigned high = (sc[8 + g % 4] >> (2 * (g / 4))) & 3;
    return (int) (low | (high << 4)) - 32;
}
static int mem_bpb3k_coeff(const uint8_t *v, int i) {
    int group = i / 64, lane = i % 64;
    uint64_t neg = mem_bpb3k_load64(v + group * 24);
    uint64_t lo = mem_bpb3k_load64(v + group * 24 + 8);
    uint64_t hi = mem_bpb3k_load64(v + group * 24 + 16);
    int mag = (int) ((lo >> lane) & 1) + 2 * (int) ((hi >> lane) & 1);
    return ((neg >> lane) & 1) ? -(mag + 1) : mag;
}
#ifdef MEM_BPB3K_DEQUANT
static void mem_bpb3k_dequant(const block_q3_K *x, float *y, int64_t k) {
    assert(k % 256 == 0);
    for (int64_t b = 0; b < k / 256; ++b) {
        const uint8_t *v = (const uint8_t *) &x[b];
        uint16_t bits;
        memcpy(&bits, v + 108, sizeof(bits));
        const float d = ggml_fp16_to_fp32(bits);
        for (int i = 0; i < 256; ++i) {
            const int value = mem_bpb3k_coeff(v, i);
            const int scale = mem_bpb3k_subscale(v + 96, i / 16);
            y[b * 256 + i] = d * (float) scale * (float) value;
        }
    }
}
#endif
#ifdef MEM_BPB3K_DOT
static void mem_bpb3k_dot_q8(int n, float *s, size_t bs,
                             const void *vx, size_t bx,
                             const void *vy, size_t by, int nrc) {
    assert(n % 256 == 0 && nrc == 1);
    static atomic_flag seen = ATOMIC_FLAG_INIT;
    if (!atomic_flag_test_and_set(&seen)) {
        fputs("BPB3K_DIRECT_DOT_EXECUTED\n", stderr);
    }
    (void) bs; (void) bx; (void) by;
    const block_q3_K *x = (const block_q3_K *)vx;
    const block_q8_K *y = (const block_q8_K *)vy;
    float total = 0.0f;
    for (int b = 0; b < n / 256; ++b) {
        const uint8_t *v = (const uint8_t *) &x[b];
        uint16_t bits;
        memcpy(&bits, v + 108, sizeof(bits));
        int32_t subtotal = 0;
        for (int sg = 0; sg < 16; ++sg) {
            int32_t dot = 0;
            for (int lane = 0; lane < 16; ++lane) {
                int i = 16 * sg + lane;
                dot += mem_bpb3k_coeff(v, i) * (int32_t)y[b].qs[i];
            }
            subtotal += mem_bpb3k_subscale(v + 96, sg) * dot;
        }
        const float d = ggml_fp16_to_fp32(bits);
        total += (d * y[b].d) * (float) subtotal;
    }
    *s = total;
}
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
    if bridge.exists() {
        return Err("STOP_BPB_BRIDGE_ALREADY_EXISTS".into());
    }
    let a = fs::read_to_string(&core).map_err(|e| e.to_string())?;
    let b = fs::read_to_string(&cpu).map_err(|e| e.to_string())?;
    let t = fs::read_to_string(&tiled).map_err(|e| e.to_string())?;
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
    let tiled_gate: &str = "        case GGML_TYPE_Q4_K:\n        case GGML_TYPE_Q3_K:\n        case GGML_TYPE_Q2_K:";
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
        assert!(C_BRIDGE.contains("mem_bpb3k_dot_q8"));
        assert!(C_BRIDGE.contains("mem_bpb3k_dequant"));
        assert!(C_BRIDGE.contains("y[b].qs[i]"));
        assert!(C_BRIDGE.contains("subtotal"));
        assert!(C_BRIDGE.contains("BPB3K_DIRECT_DOT_EXECUTED"));
    }
}
