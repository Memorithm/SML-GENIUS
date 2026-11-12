//! Reproducible, deliberately isolated ggml CPU Q5-178 experimental patch.
//! Changes only an exact, pinned llama.cpp source checkout; requires Rust
//! Q5 staticlib and a separately validated *prepared* ephemeral GGUF.
//! This does not patch production ggml, nor support native Q3_K simultaneously.
#![deny(unsafe_op_in_unsafe_fn)]
use std::fs;
use std::path::Path;

const GGML_SHA: &str = "f7591a6a01f9273de03331732bdc0b39721168be";

// The source model's Q5-178 sentinel has already been validated by
// "prepare-patched-runtime". The transient file rewrites Q5 type IDs
// to 11, which is intercepted here as a 178-byte *experimental* block.
const C_BRIDGE: &str = r#"
#ifndef MEM_BPB3K_Q5_178_EXPERIMENTAL_H
#define MEM_BPB3K_Q5_178_EXPERIMENTAL_H
#include <stdint.h>
#include <string.h>
#include <assert.h>
#ifdef MEM_BPB3K_DEQUANT
static void mem_q5_178_dequant(const block_q3_K *x, float *y, int64_t k) {
    assert(k % 256 == 0);
    for (int64_t b = 0; b < k / 256; ++b) {
        const uint8_t *w = (const uint8_t *) &x[b];
        uint16_t d_bits;
        memcpy(&d_bits, w + 176, sizeof(d_bits));
        const float d = ggml_fp16_to_fp32(d_bits);
        for (int i = 0; i < 256; ++i) {
            const int group = i / 32;
            const int lane = i % 16;
            const uint8_t packed = w[16 * group + lane];
            const int low = (i % 32) < 16 ? (packed & 15) : (packed >> 4);
            const int high = (w[128 + i / 8] >> (i % 8)) & 1;
            const int signed_code = (low | (high << 4)) - 16;
            const int scale = (int) (int8_t) w[160 + i / 16];
            y[b * 256 + i] = d * (float) scale * (float) signed_code;
        }
    }
}
#endif
#ifdef MEM_BPB3K_DOT
extern void mem_q5_178_vec_dot(int n, float *s, size_t bs,
    const void *vx, size_t bx, const void *vy, size_t by, int nrc);
#endif
#endif
"#;

// Directly unpack actual Q5 nibbles and fifth-bit plane. The hot path
// expands eight fifth bits at once into eight distinct bytes using a 64-bit
// swizzle, replacing 256 independent per-weight bitplane loads/shifts.
// No heap, dense intermediate, or stock Q3_K fallback. Exactly preserves
// the existing unsigned q[0..31] and signed bias=16 microkernel contract.
const Q5_TILED_UNPACK: &str = r#"static inline uint64_t mem_q5_178_spread_eight(uint8_t byte) {
    uint64_t value = byte;
    value = (value | (value << 28)) & UINT64_C(0x0000000F0000000F);
    value = (value | (value << 14)) & UINT64_C(0x0003000300030003);
    value = (value | (value << 7))  & UINT64_C(0x0101010101010101);
    return value << 4;
}

static void tiled_unpack_src0(const block_q3_K * rows, int64_t row_stride, int n_rows, tiled_tile_src0 * tile, int num_k) {
    GGML_ASSERT(n_rows <= TILED_TILE_ROWS);
    constexpr int NB = 16;
    const int qk_stride = num_k * TILED_TILE_K;
    const int nb_stride = NB * num_k;
    for (int slab = 0; slab < num_k; slab++) {
        for (int r = 0; r < n_rows; r++) {
            const int d_off = slab * TILED_MICRO + r;
            const int q_off = r * qk_stride + slab * TILED_TILE_K;
            const int s_off = r * nb_stride + slab * NB;
            const block_q3_K & x = rows[r * row_stride + slab];
            const uint8_t * w = reinterpret_cast<const uint8_t *>(&x);
            uint16_t global_scale_bits;
            std::memcpy(&global_scale_bits, w + 176, sizeof(global_scale_bits));
            tile->d[d_off] = ggml_fp16_to_fp32(global_scale_bits);
            for (int subgroup = 0; subgroup < NB; subgroup++) {
                int8_t signed_scale;
                std::memcpy(&signed_scale, w + 160 + subgroup, sizeof(signed_scale));
                tile->scales[s_off + subgroup] = static_cast<int32_t>(signed_scale);
            }
            uint8_t * q = &tile->q[q_off];
            // The source-pinned CPU backend is x86-64 little-endian.
            // The first nibble is code i, the second is code i+16.
            // The fifth-bit plane is byte-ordered across all 256 codes.
            for (int group = 0; group < 8; group++) {
                uint8_t * dest = q + group * 32;
#if defined(__SSE2__)
                const __m128i packed = _mm_loadu_si128(reinterpret_cast<const __m128i *>(w + group * 16));
                const __m128i mask = _mm_set1_epi8(0x0f);
                _mm_storeu_si128(reinterpret_cast<__m128i *>(dest), _mm_and_si128(packed, mask));
                const __m128i upper = _mm_and_si128(_mm_srli_epi16(packed, 4), mask);
                _mm_storeu_si128(reinterpret_cast<__m128i *>(dest + 16), upper);
#else
                for (int lane = 0; lane < 16; lane++) {
                    const uint8_t packed = w[group * 16 + lane];
                    dest[lane]      = packed & 15;
                    dest[lane + 16] = packed >> 4;
                }
#endif
                for (int byte = 0; byte < 4; byte++) {
                    const uint64_t high = mem_q5_178_spread_eight(w[128 + group * 4 + byte]);
                    uint64_t codes;
                    std::memcpy(&codes, dest + byte * 8, sizeof(codes));
                    codes |= high;
                    std::memcpy(dest + byte * 8, &codes, sizeof(codes));
                }
            }
        }
    }
}

"#;

const Q5_TILED_DISPATCH: &str = r#"        case GGML_TYPE_Q3_K: {
            static std::once_flag first_q5_tiled;
            std::call_once(first_q5_tiled, [] {
                std::fputs("BPB3K_Q5_178_REAL_TILED_EXECUTED source_five_bits=true no_native_q3_unpack=true\n", stderr);
            });
            return tiled_matmul_dispatch<block_q3_K, 16, false, 16, true>(params, dst, expert_rows, cur_a, cne1, scratch);
        }"#;

fn substitute_region(path: &Path, start: &str, end: &str, replacement: &str) -> Result<(), String> {
    let source = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if source.matches(start).count() != 1 || source.matches(end).count() != 1 {
        return Err(format!("STOP_SOURCE_LAYOUT_DRIFT path={}", path.display()));
    }
    let first = source.find(start).unwrap();
    let last = source.find(end).unwrap();
    if last <= first {
        return Err("STOP_SOURCE_LAYOUT_ORDER".into());
    }
    let mut patched = String::with_capacity(source.len() + replacement.len());
    patched.push_str(&source[..first]);
    patched.push_str(replacement);
    patched.push_str(&source[last..]);
    fs::write(path, patched).map_err(|e| e.to_string())
}
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
    let common = root.join("ggml/src/ggml-common.h");
    let core = root.join("ggml/src/ggml.c");
    let cpu = root.join("ggml/src/ggml-cpu/ggml-cpu.c");
    let tiled = root.join("ggml/src/ggml-cpu/tiled/tiled.cpp");
    let tiled_kernel = root.join("ggml/src/ggml-cpu/tiled/tiled-kernel.cpp");
    let cmake = root.join("ggml/src/ggml-cpu/CMakeLists.txt");
    let bridge = root.join("ggml/src/ggml-bpb3k-q5-178-experimental.h");
    if bridge.exists() {
        return Err("STOP_Q5_BRIDGE_EXISTS".into());
    }
    // Preserve field NAMES so unused upstream functions still compile,
    // while changing stride to 178. Live CPU .to_float/.vec_dot dispatch
    // must use our implementations; stock Q3_K cannot read Q5 bytes.
    substitute_once(
        &common,
        "    ggml_half d;           // super-block scale\n} block_q3_K;\nstatic_assert(sizeof(block_q3_K) == sizeof(ggml_half) + QK_K / 4 + QK_K / 8 + 12, \"wrong q3_K block size/padding\");",
        "    ggml_half d;           // original named field, unused with Q5 surrogate\n    uint8_t experimental_q5_178_tail[68];\n} block_q3_K;\nstatic_assert(sizeof(block_q3_K) == 178, \"experimental Q5-178 block stride\");",
    )?;
    let core_text = fs::read_to_string(&core).map_err(|e| e.to_string())?;
    let cpu_text = fs::read_to_string(&cpu).map_err(|e| e.to_string())?;
    let tiled_text = fs::read_to_string(&tiled).map_err(|e| e.to_string())?;
    if core_text.matches(".to_float                 = (ggml_to_float_t) dequantize_row_q3_K,").count() != 1
        || cpu_text.matches(".vec_dot                  = ggml_vec_dot_q3_K_q8_K,").count() != 1
        || tiled_text.matches("        case GGML_TYPE_Q4_K:\n        case GGML_TYPE_Q3_K:\n        case GGML_TYPE_Q2_K:").count() != 1
        || tiled_text.matches("        case GGML_TYPE_Q3_K:\n            return tiled_matmul_dispatch<block_q3_K, 16, false,  4, true>").count() != 1
    {
        return Err("STOP_Q5_TRAITS_OR_TILED_DRIFT".into());
    }
    fs::write(&bridge, C_BRIDGE).map_err(|e| e.to_string())?;
    substitute_once(
        &core,
        "#include <assert.h>",
        "#include <assert.h>\n#define MEM_BPB3K_DEQUANT 1\n#include \"ggml-bpb3k-q5-178-experimental.h\"",
    )?;
    substitute_once(
        &cpu,
        "#include <assert.h>",
        "#include <assert.h>\n#define MEM_BPB3K_DOT 1\n#include \"../ggml-bpb3k-q5-178-experimental.h\"",
    )?;
    substitute_once(
        &core,
        ".to_float                 = (ggml_to_float_t) dequantize_row_q3_K,",
        ".to_float                 = (ggml_to_float_t) mem_q5_178_dequant,",
    )?;
    substitute_once(
        &cpu,
        ".vec_dot                  = ggml_vec_dot_q3_K_q8_K,",
        ".vec_dot                  = mem_q5_178_vec_dot,",
    )?;
    substitute_region(
        &tiled,
        "static void tiled_unpack_src0(const block_q3_K * rows,",
        "static void tiled_unpack_src0(const block_q2_K * rows,",
        Q5_TILED_UNPACK,
    )?;
    substitute_once(
        &tiled,
        "#include <mutex>",
        "#include <mutex>\n#include <cstdio>\n#if defined(__SSE2__)\n#include <emmintrin.h>\n#endif",
    )?;
    substitute_once(
        &tiled,
        "        case GGML_TYPE_Q3_K:\n            return tiled_matmul_dispatch<block_q3_K, 16, false,  4, true>(params, dst, expert_rows, cur_a, cne1, scratch);",
        Q5_TILED_DISPATCH,
    )?;
    // tiled-kernel.cpp explicitly instantiates every supported template
    // variant in its own translation unit. Merely changing tiled.cpp's
    // dispatch to BIAS=16 compiles but leaves an unresolved linker symbol.
    // Guard the exact pinned source and instantiate the new Q5 variant.
    substitute_once(
        &tiled_kernel,
        "// q3_K: act-bias (Wmax 4)",
        "// Experimental signed Q5-178 uses a bias of 16 (Wmax 16).\ntemplate void tiled_run_microtile<16, false, 16, true>(const tiled_tile_src0 & src0, const tiled_tile_src1 & src1,\n                                                 int i0, int j0, int num_k, int slab, float * buf, int buf_stride);\n// q3_K: act-bias (Wmax 4)",
    )?;
    substitute_once(
        &cmake,
        "    ggml_add_backend_library(${GGML_CPU_NAME})",
        "    ggml_add_backend_library(${GGML_CPU_NAME})\n    if (NOT MEM_BPB3K_RUST_STATICLIB OR NOT EXISTS \"${MEM_BPB3K_RUST_STATICLIB}\")\n        message(FATAL_ERROR \"Q5-178 Rust staticlib required\")\n    endif()\n    target_link_libraries(${GGML_CPU_NAME} PRIVATE \"${MEM_BPB3K_RUST_STATICLIB}\")",
    )?;
    println!("PASS_BPB3K_Q5_GGML_PATCH original_ggml_sha={} q3_surrogate_block_bytes=178 rust_q5_dot=true q5_tiled_source_unpack=true stock_q3_weight_unpack=false",
        observed_sha);
    Ok(())
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    match args.as_slice() {
        [_, mode, checkout, sha] if mode == "patch" => patch(Path::new(checkout), sha),
        _ => Err(
            "USAGE: bpb3k_q5_ggml_patch patch <throwaway-pinned-llama-checkout> <exact-sha>".into(),
        ),
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    /// Verify the candidate bit swizzle against the independent per-bit
    /// reference for every possible fifth-bit byte (not a probabilistic test).
    #[test]
    fn exhaustive_fifth_bitbyte_spread_is_exact() {
        for sample in 0..=255u64 {
            let mut x = sample;
            x = (x | (x << 28)) & 0x0000_000f_0000_000f;
            x = (x | (x << 14)) & 0x0003_0003_0003_0003;
            x = (x | (x << 7)) & 0x0101_0101_0101_0101;
            let encoded = x << 4;
            let expected: u64 = (0..8)
                .map(|bit| ((sample >> bit) & 1) << (8 * bit + 4))
                .sum();
            assert_eq!(encoded, expected, "bad bitplane byte {sample}");
        }
    }

    /// Independent 256-weight layout parity for 1,024 deterministic packed
    /// candidate blocks: checks nibble order, high-bit bytes and boundaries.
    #[test]
    fn packed_q5_tiled_decoder_matches_all_scalar_positions() {
        for seed in 0..1024usize {
            let payload: [u8; 178] = std::array::from_fn(|i| {
                ((i.wrapping_mul(131) ^ seed.wrapping_mul(79) ^ i.wrapping_mul(seed))
                    & 255) as u8
            });
            let mut fast = [0u8; 256];
            for group in 0..8 {
                for lane in 0..16 {
                    let packed = payload[group * 16 + lane];
                    fast[group * 32 + lane] = packed & 15;
                    fast[group * 32 + lane + 16] = packed >> 4;
                }
                for byte in 0..4 {
                    let mut x = u64::from(payload[128 + group * 4 + byte]);
                    x = (x | (x << 28)) & 0x0000_000f_0000_000f;
                    x = (x | (x << 14)) & 0x0003_0003_0003_0003;
                    x = (x | (x << 7)) & 0x0101_0101_0101_0101;
                    let offset = group * 32 + byte * 8;
                    let codes = u64::from_le_bytes(fast[offset..offset + 8].try_into().unwrap())
                        | (x << 4);
                    fast[offset..offset + 8].copy_from_slice(&codes.to_le_bytes());
                }
            }
            for (i, actual) in fast.iter().enumerate() {
                let packed = payload[(i / 32) * 16 + (i % 16)];
                let low = if i % 32 < 16 { packed & 15 } else { packed >> 4 };
                let high = (payload[128 + i / 8] >> (i % 8)) & 1;
                assert_eq!(*actual, low | (high << 4), "seed={seed} index={i}");
            }
        }
    }

    #[test]
    fn wrong_pin_is_rejected_without_patching_any_checkout() {
        assert!(patch(Path::new("/nonexistent-checkout"), "wrong-pin").is_err());
    }

    #[test]
    fn q5_unpacked_sources_and_offsets_are_distinct_from_q3_and_q4() {
        assert!(C_BRIDGE.contains("w + 176"));
        assert!(C_BRIDGE.contains("w[160 + i / 16]"));
        assert!(C_BRIDGE.contains("(low | (high << 4)) - 16"));
        assert!(C_BRIDGE.contains("mem_q5_178_vec_dot"));
        assert!(!C_BRIDGE.contains("ggml_vec_dot_q3_K_q8_K"));
        assert!(Q5_TILED_UNPACK.contains("w + 176"));
        assert!(Q5_TILED_UNPACK.contains("w + 160 + subgroup"));
        assert!(Q5_TILED_UNPACK.contains("mem_q5_178_spread_eight"));
        assert!(Q5_TILED_UNPACK.contains("_mm_storeu_si128"));
        assert!(Q5_TILED_UNPACK.contains("#else"));
        assert!(Q5_TILED_UNPACK.contains("_mm_srli_epi16"));
        assert!(Q5_TILED_UNPACK.contains("w[128 + group * 4 + byte]"));
        assert!(Q5_TILED_UNPACK.contains("std::memcpy(dest + byte * 8, &codes, sizeof(codes))"));
        assert!(Q5_TILED_DISPATCH.contains("REAL_TILED_EXECUTED"));
        assert!(Q5_TILED_DISPATCH.contains(", 16, true>"));
        assert!(GGML_SHA.len() == 40);
    }
}
