//! BPB3-K CPU kernels: scalar reference, AVX2 SIMD, 4-row tiled, hybrid SIMD×tiled.
//! Research-only kernel, Rust 1.89, no vendor GPU SDK. Direct 110-byte bitplane
//! Q3_K relayout with 292-byte Q8_K activations. Never accepts stock Q3_K bytes.
//! The eight-lane integer / fp32 reduction mirrors ggml's Q3_K generic oracle.
use std::env;
use std::ffi::c_void;
use std::slice;
use std::sync::OnceLock;
#[cfg(test)]
#[path = "bpb3k.rs"]
#[allow(dead_code)]
mod codec;

const BLOCK_W: usize = 110;
const BLOCK_A: usize = 292;
const LANES: usize = 8;
const TILE: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Scalar,
    Simd,
    Tiled,
    Hybrid,
    Fused,
    FusedV2,
    FusedV2I16,
    StockVector,
    FusedV2Stock,
}

fn mode() -> Mode {
    static SELECTED: OnceLock<Mode> = OnceLock::new();
    *SELECTED.get_or_init(|| match env::var("MEM_BPB3K_KERNEL").as_deref() {
        Ok("simd") => Mode::Simd,
        Ok("tiled") => Mode::Tiled,
        Ok("hybrid") => Mode::Hybrid,
        Ok("fused") => Mode::Fused,
        Ok("fused_v2") => Mode::FusedV2,
        Ok("fused_v2_i16") => Mode::FusedV2I16,
        Ok("stock_vector") => Mode::StockVector,
        Ok("fused_v2_stock") => Mode::FusedV2Stock,
        _ => Mode::Scalar,
    })
}
fn u64_at(block: &[u8], p: usize) -> u64 {
    u64::from_le_bytes(block[p..p + 8].try_into().expect("fixed BPB3-K payload"))
}
fn coeff(block: &[u8], i: usize) -> i32 {
    let off = (i / 64) * 24;
    let b = i % 64;
    let sign = (u64_at(block, off) >> b) & 1;
    let lo = (u64_at(block, off + 8) >> b) & 1;
    let hi = (u64_at(block, off + 16) >> b) & 1;
    let mag = (lo + 2 * hi) as i32;
    if sign != 0 {
        -mag - 1
    } else {
        mag
    }
}
fn scale(block: &[u8], g: usize) -> i32 {
    let sc = &block[96..108];
    let low = if g < 8 { sc[g] & 15 } else { sc[g - 8] >> 4 };
    let high = (sc[8 + g % 4] >> (2 * (g / 4))) & 3;
    i32::from(low | (high << 4)) - 32
}
fn fp16(v: u16) -> f32 {
    // Exact IEEE 754 FP16-to-FP32 widening for normals, zero and infinity:
    // avoid per-superblock powi/libm calls in the token-generation hot path.
    let sign = ((v as u32) & 0x8000) << 16;
    let exponent = ((v >> 10) & 31) as u32;
    let fraction = (v as u32) & 1023;
    if exponent == 0 {
        if fraction == 0 {
            return f32::from_bits(sign);
        }
        let magnitude = (fraction as f32) * (1.0f32 / 16_777_216.0f32);
        return if sign != 0 { -magnitude } else { magnitude };
    }
    if exponent == 31 {
        return f32::from_bits(sign | 0x7f80_0000 | (fraction << 13));
    }
    f32::from_bits(sign | ((exponent + 112) << 23) | (fraction << 13))
}

fn global_scale(block: &[u8], act: &[u8]) -> f32 {
    fp16(u16::from_le_bytes([block[108], block[109]]))
        * f32::from_le_bytes(act[..4].try_into().unwrap())
}
fn finish(sums: [f32; LANES]) -> f32 {
    let mut result = 0.0f32;
    for x in sums {
        result += x;
    }
    result
}

fn reference(weights: &[u8], activations: &[u8]) -> f32 {
    let mut sums = [0.0f32; LANES];
    for (block, act) in weights
        .chunks_exact(BLOCK_W)
        .zip(activations.chunks_exact(BLOCK_A))
    {
        let mut ints = [0i32; LANES];
        for g in 0..16 {
            let sc = scale(block, g);
            for lane in 0..8 {
                let a = 16 * g + lane;
                let b = a + 8;
                let va = (act[4 + a] as i8) as i32;
                let vb = (act[4 + b] as i8) as i32;
                ints[lane] += sc * (coeff(block, a) * va + coeff(block, b) * vb);
            }
        }
        let d = global_scale(block, act);
        for lane in 0..LANES {
            sums[lane] += d * (ints[lane] as f32);
        }
    }
    finish(sums)
}

// Diagnostic contract of pinned arch/x86/quants.c Q3_K AVX2: each integer
// lane contains four adjacent coefficients in each 32-value group, followed
// by blockwise FMA and the upstream balanced horizontal reduction. This is
// deliberately separate from v1/v2's generic eight-lane contract.
fn stock_vector_reference(weights: &[u8], activations: &[u8]) -> f32 {
    let mut sums = [0.0f32; LANES];
    for (w, a) in weights
        .chunks_exact(BLOCK_W)
        .zip(activations.chunks_exact(BLOCK_A))
    {
        let mut ints = [0i32; LANES];
        for i in 0..256 {
            ints[(i % 32) / 4] += coeff(w, i) * scale(w, i / 16) * i32::from(a[4 + i] as i8);
        }
        let d = global_scale(w, a);
        for lane in 0..LANES {
            sums[lane] = d.mul_add(ints[lane] as f32, sums[lane]);
        }
    }
    let a = sums[4] + sums[0];
    let b = sums[5] + sums[1];
    let c = sums[6] + sums[2];
    let d = sums[7] + sums[3];
    (a + c) + (b + d)
}

fn stock_tiled_reference(weights: &[u8], activations: &[u8]) -> f32 {
    let mut sum = 0.0f32;
    for (w, a) in weights
        .chunks_exact(BLOCK_W)
        .zip(activations.chunks_exact(BLOCK_A))
    {
        let mut total = 0i32;
        for i in 0..256 {
            total += coeff(w, i) * scale(w, i / 16) * i32::from(a[4 + i] as i8);
        }
        let d = fp16(u16::from_le_bytes([w[108], w[109]]));
        let ad = f32::from_le_bytes(a[..4].try_into().unwrap());
        sum = (d * total as f32).mul_add(ad, sum);
    }
    sum
}

#[cfg(target_arch = "x86_64")]
mod x86 {
    use super::*;
    use std::arch::x86_64::*;
    // Decode in the order (0,8,1,9,...,7,15), so signed VPMADDWD
    // produces the original eight ggml lanes without a horizontal reduction.
    // |coefficient * scale| <= 128; each paired product <= 32768;
    // each superblock lane <= 524288. Neither i16 scaling nor i32 sums overflow.
    #[target_feature(enable = "avx2")]
    pub unsafe fn fused_v2_i16_block(w: &[u8], a: &[u8]) -> __m256i {
        let bits = _mm256_setr_epi16(
            1,
            256,
            2,
            512,
            4,
            1024,
            8,
            2048,
            16,
            4096,
            32,
            8192,
            64,
            16384,
            128,
            i16::MIN,
        );
        let order = _mm_setr_epi8(0, 8, 1, 9, 2, 10, 3, 11, 4, 12, 5, 13, 6, 14, 7, 15);
        let one = _mm256_set1_epi16(1);
        let two = _mm256_set1_epi16(2);
        let mut even = _mm256_setzero_si256();
        let mut odd = even;
        for bank in 0..4 {
            let neg = u64_at(w, bank * 24);
            let lo = u64_at(w, bank * 24 + 8);
            let hi = u64_at(w, bank * 24 + 16);
            for part in 0..4 {
                let g = bank * 4 + part;
                let shift = part * 16;
                let n = _mm256_set1_epi16((neg >> shift) as i16);
                let l = _mm256_set1_epi16((lo >> shift) as i16);
                let h = _mm256_set1_epi16((hi >> shift) as i16);
                let nm = _mm256_cmpeq_epi16(_mm256_and_si256(bits, n), bits);
                let lv = _mm256_and_si256(_mm256_cmpeq_epi16(_mm256_and_si256(bits, l), bits), one);
                let hv = _mm256_and_si256(_mm256_cmpeq_epi16(_mm256_and_si256(bits, h), bits), two);
                // XOR with -1 implements exactly -magnitude-1.
                let signed = _mm256_xor_si256(_mm256_or_si256(lv, hv), nm);
                let scaled = _mm256_mullo_epi16(signed, _mm256_set1_epi16(scale(w, g) as i16));
                let input = _mm_loadu_si128(a.as_ptr().add(4 + 16 * g) as *const __m128i);
                let q8 = _mm256_cvtepi8_epi16(_mm_shuffle_epi8(input, order));
                let paired = _mm256_madd_epi16(scaled, q8);
                if part & 1 == 0 {
                    even = _mm256_add_epi32(even, paired);
                } else {
                    odd = _mm256_add_epi32(odd, paired);
                }
            }
        }
        _mm256_add_epi32(even, odd)
    }
    // Share three 64-bit broadcasts over a full bank; decode 32 signed bytes.
    #[target_feature(enable = "avx2")]
    pub unsafe fn fused_v2_block(w: &[u8], a: &[u8]) -> __m256i {
        let bits = _mm256_set1_epi64x(0x8040_2010_0804_0201u64 as i64);
        let indices = _mm256_setr_epi64x(
            0,
            0x0101_0101_0101_0101,
            0x0202_0202_0202_0202,
            0x0303_0303_0303_0303,
        );
        let order = _mm256_broadcastsi128_si256(_mm_setr_epi8(
            0, 8, 1, 9, 2, 10, 3, 11, 4, 12, 5, 13, 6, 14, 7, 15,
        ));
        let one = _mm256_set1_epi8(1);
        let two = _mm256_set1_epi8(2);
        let mut even = _mm256_setzero_si256();
        let mut odd = even;
        for bank in 0..4 {
            let neg = _mm256_set1_epi64x(u64_at(w, bank * 24) as i64);
            let lo = _mm256_set1_epi64x(u64_at(w, bank * 24 + 8) as i64);
            let hi = _mm256_set1_epi64x(u64_at(w, bank * 24 + 16) as i64);
            for part in 0..2 {
                let g = bank * 4 + part * 2;
                let index = _mm256_add_epi8(indices, _mm256_set1_epi8((part * 4) as i8));
                let n = _mm256_shuffle_epi8(neg, index);
                let l = _mm256_shuffle_epi8(lo, index);
                let h = _mm256_shuffle_epi8(hi, index);
                let nm = _mm256_cmpeq_epi8(_mm256_and_si256(n, bits), bits);
                let lv = _mm256_and_si256(_mm256_cmpeq_epi8(_mm256_and_si256(l, bits), bits), one);
                let hv = _mm256_and_si256(_mm256_cmpeq_epi8(_mm256_and_si256(h, bits), bits), two);
                let coefficients =
                    _mm256_shuffle_epi8(_mm256_xor_si256(_mm256_or_si256(lv, hv), nm), order);
                let input = _mm256_shuffle_epi8(
                    _mm256_loadu_si256(a.as_ptr().add(4 + g * 16).cast()),
                    order,
                );
                let c0 = _mm256_cvtepi8_epi16(_mm256_castsi256_si128(coefficients));
                let c1 = _mm256_cvtepi8_epi16(_mm256_extracti128_si256(coefficients, 1));
                let a0 = _mm256_cvtepi8_epi16(_mm256_castsi256_si128(input));
                let a1 = _mm256_cvtepi8_epi16(_mm256_extracti128_si256(input, 1));
                let p0 = _mm256_madd_epi16(
                    _mm256_mullo_epi16(c0, _mm256_set1_epi16(scale(w, g) as i16)),
                    a0,
                );
                let p1 = _mm256_madd_epi16(
                    _mm256_mullo_epi16(c1, _mm256_set1_epi16(scale(w, g + 1) as i16)),
                    a1,
                );
                even = _mm256_add_epi32(even, p0);
                odd = _mm256_add_epi32(odd, p1);
            }
        }
        _mm256_add_epi32(even, odd)
    }
    #[target_feature(enable = "avx2")]
    pub unsafe fn fused_v2_row(w: &[u8], a: &[u8], packed: bool) -> f32 {
        let mut sums = _mm256_setzero_ps();
        for (weight, act) in w.chunks_exact(BLOCK_W).zip(a.chunks_exact(BLOCK_A)) {
            let ints = if packed {
                fused_v2_block(weight, act)
            } else {
                fused_v2_i16_block(weight, act)
            };
            let product = _mm256_mul_ps(
                _mm256_set1_ps(global_scale(weight, act)),
                _mm256_cvtepi32_ps(ints),
            );
            // No FMA or reassociation: preserve each lane's block order.
            sums = _mm256_add_ps(sums, product);
        }
        let mut lanes = [0.0f32; LANES];
        _mm256_storeu_ps(lanes.as_mut_ptr(), sums);
        finish(lanes)
    }
    #[target_feature(enable = "avx2,fma")]
    pub unsafe fn stock_vector_row(w: &[u8], a: &[u8]) -> f32 {
        let bits = _mm256_set1_epi64x(0x8040_2010_0804_0201u64 as i64);
        let indices = _mm256_setr_epi64x(
            0,
            0x0101_0101_0101_0101,
            0x0202_0202_0202_0202,
            0x0303_0303_0303_0303,
        );
        let one = _mm256_set1_epi8(1);
        let two = _mm256_set1_epi8(2);
        let mut acc = _mm256_setzero_ps();
        for (weight, act) in w.chunks_exact(BLOCK_W).zip(a.chunks_exact(BLOCK_A)) {
            let mut ints = _mm256_setzero_si256();
            for bank in 0..4 {
                let neg = _mm256_set1_epi64x(u64_at(weight, bank * 24) as i64);
                let lo = _mm256_set1_epi64x(u64_at(weight, bank * 24 + 8) as i64);
                let hi = _mm256_set1_epi64x(u64_at(weight, bank * 24 + 16) as i64);
                for part in 0..2 {
                    let g = bank * 4 + part * 2;
                    let index = _mm256_add_epi8(indices, _mm256_set1_epi8((part * 4) as i8));
                    let n = _mm256_shuffle_epi8(neg, index);
                    let l = _mm256_shuffle_epi8(lo, index);
                    let h = _mm256_shuffle_epi8(hi, index);
                    let nm = _mm256_cmpeq_epi8(_mm256_and_si256(n, bits), bits);
                    let lv =
                        _mm256_and_si256(_mm256_cmpeq_epi8(_mm256_and_si256(l, bits), bits), one);
                    let hv =
                        _mm256_and_si256(_mm256_cmpeq_epi8(_mm256_and_si256(h, bits), bits), two);
                    let q = _mm256_xor_si256(_mm256_or_si256(lv, hv), nm);
                    let aq = _mm256_loadu_si256(act.as_ptr().add(4 + g * 16).cast());
                    let p0 = _mm256_madd_epi16(
                        _mm256_mullo_epi16(
                            _mm256_cvtepi8_epi16(_mm256_castsi256_si128(q)),
                            _mm256_set1_epi16(scale(weight, g) as i16),
                        ),
                        _mm256_cvtepi8_epi16(_mm256_castsi256_si128(aq)),
                    );
                    let p1 = _mm256_madd_epi16(
                        _mm256_mullo_epi16(
                            _mm256_cvtepi8_epi16(_mm256_extracti128_si256(q, 1)),
                            _mm256_set1_epi16(scale(weight, g + 1) as i16),
                        ),
                        _mm256_cvtepi8_epi16(_mm256_extracti128_si256(aq, 1)),
                    );
                    // Each madd result is an adjacent pair. Join pairs into
                    // the four-coefficient lanes of the original stock kernel.
                    let joined = _mm256_hadd_epi32(p0, p1);
                    let joined = _mm256_permutevar8x32_epi32(
                        joined,
                        _mm256_setr_epi32(0, 1, 4, 5, 2, 3, 6, 7),
                    );
                    ints = _mm256_add_epi32(ints, joined);
                }
            }
            acc = _mm256_fmadd_ps(
                _mm256_set1_ps(global_scale(weight, act)),
                _mm256_cvtepi32_ps(ints),
                acc,
            );
        }
        let sum = _mm_add_ps(_mm256_extractf128_ps(acc, 1), _mm256_castps256_ps128(acc));
        let sum = _mm_add_ps(sum, _mm_movehl_ps(sum, sum));
        _mm_cvtss_f32(_mm_add_ss(sum, _mm_movehdup_ps(sum)))
    }
    #[target_feature(enable = "avx2,fma")]
    pub unsafe fn stock_tiled_row(w: &[u8], a: &[u8]) -> f32 {
        let mut sum = _mm_setzero_ps();
        for (weight, act) in w.chunks_exact(BLOCK_W).zip(a.chunks_exact(BLOCK_A)) {
            let ints = fused_v2_block(weight, act);
            // Full block sum is bounded by 4,194,304: exactly f32-representable.
            let pair = _mm_add_epi32(
                _mm256_castsi256_si128(ints),
                _mm256_extracti128_si256(ints, 1),
            );
            let pair = _mm_hadd_epi32(pair, pair);
            let total = _mm_cvtsi128_si32(_mm_hadd_epi32(pair, pair));
            let d = fp16(u16::from_le_bytes([weight[108], weight[109]]));
            let ad = f32::from_le_bytes(act[..4].try_into().unwrap());
            sum = _mm_fmadd_ss(_mm_set_ss(d * total as f32), _mm_set_ss(ad), sum);
        }
        _mm_cvtss_f32(sum)
    }
    // Every AVX2 operation is isolated in this dynamically guarded function;
    // no global -march=native or AVX2 assumption in the main code.
    // Fused AVX2: keep eight integer accumulators in registers over the
    // entire 256-element superblock; loads 3 bitplane words per 16-element
    // subgroup rather than per 8-element half. One store per superblock.
    #[target_feature(enable = "avx2")]
    pub unsafe fn fused_block(w: &[u8], a: &[u8]) -> [i32; 8] {
        let bitmask = _mm256_setr_epi32(1, 2, 4, 8, 16, 32, 64, 128);
        let one = _mm256_set1_epi32(1);
        let two = _mm256_set1_epi32(2);
        let zero = _mm256_setzero_si256();
        let mut sum = zero;
        for g in 0..16usize {
            let off = (g / 4) * 24;
            let shift = (g % 4) * 16;
            let neg = u64_at(w, off) >> shift;
            let lo = u64_at(w, off + 8) >> shift;
            let hi = u64_at(w, off + 16) >> shift;
            let sc = _mm256_set1_epi32(scale(w, g));
            for half in 0..2usize {
                let bitshift = half * 8;
                let negbits = _mm256_set1_epi32(((neg >> bitshift) & 255) as i32);
                let lobits = _mm256_set1_epi32(((lo >> bitshift) & 255) as i32);
                let hibits = _mm256_set1_epi32(((hi >> bitshift) & 255) as i32);
                let negmask = _mm256_cmpeq_epi32(_mm256_and_si256(bitmask, negbits), bitmask);
                let low = _mm256_and_si256(
                    _mm256_cmpeq_epi32(_mm256_and_si256(bitmask, lobits), bitmask),
                    one,
                );
                let high = _mm256_and_si256(
                    _mm256_cmpeq_epi32(_mm256_and_si256(bitmask, hibits), bitmask),
                    two,
                );
                let magnitude = _mm256_add_epi32(low, high);
                let signed = _mm256_blendv_epi8(
                    magnitude,
                    _mm256_sub_epi32(zero, _mm256_add_epi32(magnitude, one)),
                    negmask,
                );
                let input =
                    _mm_loadl_epi64(a.as_ptr().add(4 + g * 16 + half * 8) as *const __m128i);
                let q8 = _mm256_cvtepi8_epi32(input);
                sum = _mm256_add_epi32(sum, _mm256_mullo_epi32(sc, _mm256_mullo_epi32(signed, q8)));
            }
        }
        let mut out = [0i32; 8];
        _mm256_storeu_si256(out.as_mut_ptr() as *mut __m256i, sum);
        out
    }
    #[target_feature(enable = "avx2")]
    pub unsafe fn product8(block: &[u8], act: &[u8], start: usize) -> [i32; 8] {
        let group = start / 64;
        let lane = start % 64;
        let off = group * 24;
        let neg = (u64_at(block, off) >> lane) as i32 & 255;
        let lo = (u64_at(block, off + 8) >> lane) as i32 & 255;
        let hi = (u64_at(block, off + 16) >> lane) as i32 & 255;
        let bits = _mm256_setr_epi32(1, 2, 4, 8, 16, 32, 64, 128);
        let lo_v = _mm256_and_si256(
            _mm256_cmpeq_epi32(_mm256_and_si256(bits, _mm256_set1_epi32(lo)), bits),
            _mm256_set1_epi32(1),
        );
        let hi_v = _mm256_and_si256(
            _mm256_cmpeq_epi32(_mm256_and_si256(bits, _mm256_set1_epi32(hi)), bits),
            _mm256_set1_epi32(2),
        );
        let mag = _mm256_add_epi32(lo_v, hi_v);
        let signed = _mm256_blendv_epi8(
            mag,
            _mm256_sub_epi32(
                _mm256_setzero_si256(),
                _mm256_add_epi32(mag, _mm256_set1_epi32(1)),
            ),
            _mm256_cmpeq_epi32(_mm256_and_si256(bits, _mm256_set1_epi32(neg)), bits),
        );
        // Unaligned eight signed activation bytes -> eight 32-bit signed lanes.
        let bytes = _mm_loadl_epi64(act.as_ptr().add(4 + start) as *const __m128i);
        let q8 = _mm256_cvtepi8_epi32(bytes);
        let product = _mm256_mullo_epi32(signed, q8);
        let mut result = [0i32; 8];
        _mm256_storeu_si256(result.as_mut_ptr() as *mut __m256i, product);
        result
    }
}
fn avx2_available() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        std::is_x86_feature_detected!("avx2")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}
fn fma_available() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        std::is_x86_feature_detected!("fma")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}
fn stock_vector_row(weights: &[u8], act: &[u8]) -> f32 {
    assert_eq!(act.len() % BLOCK_A, 0);
    assert_eq!(weights.len(), act.len() / BLOCK_A * BLOCK_W);
    if avx2_available() && fma_available() {
        #[cfg(target_arch = "x86_64")]
        return unsafe { x86::stock_vector_row(weights, act) };
    }
    stock_vector_reference(weights, act)
}
fn stock_tiled_row(weights: &[u8], act: &[u8]) -> f32 {
    assert_eq!(act.len() % BLOCK_A, 0);
    assert_eq!(weights.len(), act.len() / BLOCK_A * BLOCK_W);
    if avx2_available() && fma_available() {
        #[cfg(target_arch = "x86_64")]
        return unsafe { x86::stock_tiled_row(weights, act) };
    }
    stock_tiled_reference(weights, act)
}
#[no_mangle]
pub extern "C" fn mem_bpb3k_uses_stock_order() -> i32 {
    i32::from(mode() == Mode::FusedV2Stock)
}
fn lane_products(block: &[u8], act: &[u8], start: usize, simd: bool) -> [i32; 8] {
    if simd && avx2_available() {
        #[cfg(target_arch = "x86_64")]
        {
            return unsafe { x86::product8(block, act, start) };
        }
    }
    std::array::from_fn(|l| coeff(block, start + l) * (act[4 + start + l] as i8 as i32))
}
fn grouped(weights: &[&[u8]], act: &[u8], simd: bool) -> Vec<f32> {
    let rows = weights.len();
    assert!((1..=TILE).contains(&rows));
    let blocks = act.len() / BLOCK_A;
    assert_eq!(act.len() % BLOCK_A, 0);
    for w in weights {
        assert_eq!(w.len(), blocks * BLOCK_W);
    }
    let mut sums = [[0.0f32; LANES]; TILE];
    for b in 0..blocks {
        let a = &act[b * BLOCK_A..(b + 1) * BLOCK_A];
        let mut acc = [[0i32; LANES]; TILE];
        for g in 0..16 {
            for half in 0..2 {
                let start = 16 * g + 8 * half;
                for row in 0..rows {
                    let w = &weights[row][b * BLOCK_W..(b + 1) * BLOCK_W];
                    let v = lane_products(w, a, start, simd);
                    let s = scale(w, g);
                    for lane in 0..LANES {
                        acc[row][lane] += v[lane] * s;
                    }
                }
            }
        }
        for row in 0..rows {
            let w = &weights[row][b * BLOCK_W..(b + 1) * BLOCK_W];
            let d = global_scale(w, a);
            for lane in 0..LANES {
                sums[row][lane] += d * (acc[row][lane] as f32);
            }
        }
    }
    sums[..rows].iter().copied().map(finish).collect()
}

// Fused AVX2 experiment maintains exactly the eight ggml integer lanes and
// f32 reductions; the existing modes remain independent controls.
fn fused_rows(weights: &[&[u8]], act: &[u8]) -> Vec<f32> {
    let rows = weights.len();
    assert!((1..=TILE).contains(&rows));
    assert_eq!(act.len() % BLOCK_A, 0);
    let blocks = act.len() / BLOCK_A;
    for w in weights {
        assert_eq!(w.len(), blocks * BLOCK_W);
    }
    if !avx2_available() {
        return weights.iter().map(|w| reference(w, act)).collect();
    }
    let mut sums = [[0.0f32; LANES]; TILE];
    for b in 0..blocks {
        let input = &act[b * BLOCK_A..(b + 1) * BLOCK_A];
        for row in 0..rows {
            let w = &weights[row][b * BLOCK_W..(b + 1) * BLOCK_W];
            #[cfg(target_arch = "x86_64")]
            let ints = unsafe { x86::fused_block(w, input) };
            #[cfg(not(target_arch = "x86_64"))]
            let ints = {
                let mut lanes = [0i32; LANES];
                for g in 0..16 {
                    let sc = scale(w, g);
                    for l in 0..LANES {
                        let x = g * 16 + l;
                        lanes[l] += sc
                            * (coeff(w, x) * (input[4 + x] as i8 as i32)
                                + coeff(w, x + 8) * (input[4 + x + 8] as i8 as i32));
                    }
                }
                lanes
            };
            let d = global_scale(w, input);
            for lane in 0..LANES {
                sums[row][lane] += d * (ints[lane] as f32);
            }
        }
    }
    sums[..rows].iter().copied().map(finish).collect()
}
fn fused_v2_row(weights: &[u8], act: &[u8]) -> f32 {
    row_v2(weights, act, true)
}
fn row_v2(weights: &[u8], act: &[u8], _packed: bool) -> f32 {
    assert_eq!(act.len() % BLOCK_A, 0);
    assert_eq!(weights.len(), act.len() / BLOCK_A * BLOCK_W);
    if avx2_available() {
        #[cfg(target_arch = "x86_64")]
        return unsafe { x86::fused_v2_row(weights, act, _packed) };
    }
    reference(weights, act)
}
/// Native ggml vec_dot ABI. bx/by are byte strides; bs is an f32-element
/// stride (matching ggml's multi-row kernel convention). Upstream tensors
/// determine buffer bounds; do not expose this entry to untrusted callers.
///
/// # Safety
/// vx must reference nrc rows of n/256 contiguous 110-byte BPB3-K blocks with
/// bx bytes between rows, and vy must reference a Q8_K row of n/256*292 bytes.
/// s must be writable at the nrc output positions separated by bs f32s.
#[no_mangle]
pub unsafe extern "C" fn mem_bpb3k_dot_q8(
    n: i32,
    s: *mut f32,
    bs: usize,
    vx: *const c_void,
    bx: usize,
    vy: *const c_void,
    by: usize,
    nrc: i32,
) {
    dot_impl(n, s, bs, vx, bx, vy, by, nrc, false);
}
/// Same safety and buffer contract as mem_bpb3k_dot_q8; used only by the
/// pinned adapter when the original stock would select tiled arithmetic.
/// # Safety
/// Caller must supply the valid square microtile buffers documented above.
#[no_mangle]
pub unsafe extern "C" fn mem_bpb3k_dot_q8_stock_tiled(
    n: i32,
    s: *mut f32,
    bs: usize,
    vx: *const c_void,
    bx: usize,
    vy: *const c_void,
    by: usize,
    nrc: i32,
) {
    assert_eq!(mode(), Mode::FusedV2Stock);
    dot_impl(n, s, bs, vx, bx, vy, by, nrc, true);
}
#[allow(clippy::too_many_arguments)]
unsafe fn dot_impl(
    n: i32,
    s: *mut f32,
    bs: usize,
    vx: *const c_void,
    bx: usize,
    vy: *const c_void,
    by: usize,
    nrc: i32,
    stock_tiled: bool,
) {
    assert!(n > 0 && n % 256 == 0 && (1..=4).contains(&nrc));
    let bytes = n as usize / 256;
    // ggml nrc is a square microtile: 4 adjacent weight rows and
    // 4 adjacent activation columns; s[activation * bs + weight].
    let nrc = nrc as usize;
    let m = mode();
    static TRACE: OnceLock<()> = OnceLock::new();
    static TILED_TRACE: OnceLock<()> = OnceLock::new();
    let trace = if stock_tiled { &TILED_TRACE } else { &TRACE };
    trace.get_or_init(|| {
        eprintln!(
            "BPB3K_DIRECT_DOT_EXECUTED kernel={m:?} rows={nrc} avx2={} fma={} stock_tiled={stock_tiled}",
            avx2_available(),
            fma_available()
        )
    });

    if m == Mode::FusedV2
        || m == Mode::FusedV2I16
        || m == Mode::StockVector
        || m == Mode::FusedV2Stock
    {
        // Bypass all per-call row/result Vec allocations for the v2 path.
        for col in 0..nrc {
            let act = slice::from_raw_parts((vy as *const u8).add(col * by), bytes * BLOCK_A);
            for row in 0..nrc {
                let w = slice::from_raw_parts((vx as *const u8).add(row * bx), bytes * BLOCK_W);
                *s.add(col * bs + row) = if stock_tiled {
                    stock_tiled_row(w, act)
                } else if m == Mode::StockVector || m == Mode::FusedV2Stock {
                    stock_vector_row(w, act)
                } else if m == Mode::FusedV2 {
                    fused_v2_row(w, act)
                } else {
                    row_v2(w, act, false)
                };
            }
        }
        return;
    }
    let rows: Vec<_> = (0..nrc)
        .map(|i| slice::from_raw_parts((vx as *const u8).add(i * bx), bytes * BLOCK_W))
        .collect();

    for col in 0..nrc {
        let act = slice::from_raw_parts((vy as *const u8).add(col * by), bytes * BLOCK_A);
        let values: Vec<f32> = match m {
            Mode::Scalar => rows.iter().map(|r| reference(r, act)).collect(),
            Mode::Simd => rows.iter().map(|r| grouped(&[*r], act, true)[0]).collect(),
            Mode::Tiled => grouped(&rows, act, false),
            Mode::Hybrid => grouped(&rows, act, true),
            Mode::Fused => fused_rows(&rows, act),
            Mode::FusedV2 | Mode::FusedV2I16 | Mode::StockVector | Mode::FusedV2Stock => {
                unreachable!("v2 writes directly to caller storage")
            }
        };
        for (row, value) in values.iter().enumerate() {
            *s.add(col * bs + row) = *value;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fp16_all_65536_patterns_categories_and_nan_payloads() {
        let mut counts = [0usize; 5]; // normal, zero, subnormal, infinity, NaN
        for bits in 0u16..=u16::MAX {
            let exp = (bits >> 10) & 31;
            let frac = bits & 1023;
            let sign = if bits & 0x8000 == 0 { 1.0f64 } else { -1.0 };
            let value = fp16(bits);
            let expected = match (exp, frac) {
                (0, 0) => {
                    counts[1] += 1;
                    (sign * 0.0) as f32
                }
                (0, _) => {
                    counts[2] += 1;
                    (sign * f64::from(frac) / 16_777_216.0) as f32
                }
                (31, 0) => {
                    counts[3] += 1;
                    (sign * f64::INFINITY) as f32
                }
                (31, _) => {
                    counts[4] += 1;
                    assert!(value.is_nan());
                    assert_eq!(value.to_bits() >> 31, u32::from(bits >> 15));
                    assert_eq!((value.to_bits() & 0x7fffff) >> 13, u32::from(frac));
                    assert_eq!(value.to_bits() & 0x1fff, 0);
                    continue;
                }
                _ => {
                    counts[0] += 1;
                    (sign * (1024.0 + f64::from(frac)) * 2.0f64.powi(i32::from(exp) - 25)) as f32
                }
            };
            assert_eq!(value.to_bits(), expected.to_bits(), "FP16={bits:04x}");
        }
        assert_eq!(counts, [61440, 2, 2046, 2, 2046]);
    }
    #[test]
    fn fused_v2_original_q3_integer_oracle_all_scales_and_activation_extremes() {
        if !avx2_available() {
            return;
        }
        let mut rng = 0xa72b3915u32;
        for case in 0..4096usize {
            let mut raw = [0u8; BLOCK_W];
            for byte in &mut raw {
                rng ^= rng << 13;
                rng ^= rng >> 17;
                rng ^= rng << 5;
                *byte = rng as u8;
            }
            // Sweep each scale code in each metadata position, plus random cases.
            if case < 64 {
                raw[96..108].fill(0);
                for g in 0..16 {
                    let code = ((case + g) % 64) as u8;
                    raw[96 + g % 8] |= (code & 15) << (4 * (g / 8));
                    raw[104 + g % 4] |= (code >> 4) << (2 * (g / 4));
                }
            }
            raw[108..].copy_from_slice(&0x3c00u16.to_le_bytes());
            let w = codec::q3_to_bpb3k(&raw).unwrap();
            let mut a = input(case, 1);
            if case % 3 == 0 {
                a[4..260].fill(128);
            }
            if case % 3 == 1 {
                a[4..260].fill(127);
            }
            // Independent oracle addresses original Q3_K hmask/qs bytes,
            // never coeff(), scale(), or BPB3-K decoding.
            let mut expected = [0i32; 8];
            for i in 0..256 {
                let local = i % 32;
                let half = i / 128;
                let plane = (i % 128) / 32;
                let q = i32::from((raw[32 + half * 32 + local] >> (2 * plane)) & 3)
                    - if raw[local] & (1 << (half * 4 + plane)) == 0 {
                        4
                    } else {
                        0
                    };
                let g = i / 16;
                let low = if g < 8 {
                    raw[96 + g] & 15
                } else {
                    raw[96 + g - 8] >> 4
                };
                let high = (raw[104 + g % 4] >> (2 * (g / 4))) & 3;
                let sc = i32::from(low | (high << 4)) - 32;
                expected[i % 8] += q * sc * i32::from(a[4 + i] as i8);
            }
            #[cfg(target_arch = "x86_64")]
            unsafe {
                let mut actual = [0i32; 8];
                std::arch::x86_64::_mm256_storeu_si256(
                    actual.as_mut_ptr().cast(),
                    x86::fused_v2_block(&w, &a),
                );
                assert_eq!(actual, expected, "case={case}");
                std::arch::x86_64::_mm256_storeu_si256(
                    actual.as_mut_ptr().cast(),
                    x86::fused_v2_i16_block(&w, &a),
                );
                assert_eq!(actual, expected, "i16 ablation case={case}");
            }
        }
    }
    #[test]
    fn stock_vector_multiblock_contract_unaligned_and_rounding_sensitive() {
        let mut differs_from_generic = 0;
        for blocks in [0, 1, 2, 3, 6, 9, 36, 257] {
            for seed in 0..32 {
                let mut w = wire(seed, blocks);
                let mut a = input(seed, blocks);
                for b in 0..blocks {
                    let d = [
                        0x0001u16, 0x03ff, 0x0400, 0x3555, 0x7bff, 0x8000, 0xbc00, 0xfbff,
                    ][(b + seed) % 8];
                    w[b * BLOCK_W + 108..(b + 1) * BLOCK_W].copy_from_slice(&d.to_le_bytes());
                    a[b * BLOCK_A..b * BLOCK_A + 4].copy_from_slice(
                        &[0.0f32, -0.125, 0.1, 1.0e-20, 1.0e20][(b + seed) % 5].to_le_bytes(),
                    );
                }
                let expected = stock_vector_reference(&w, &a);
                differs_from_generic +=
                    usize::from(expected.to_bits() != reference(&w, &a).to_bits());
                let mut wu = vec![0u8];
                wu.extend_from_slice(&w);
                let mut au = vec![0u8; 3];
                au.extend_from_slice(&a);
                assert_eq!(
                    stock_vector_row(&wu[1..], &au[3..]).to_bits(),
                    expected.to_bits(),
                    "blocks={blocks} seed={seed}"
                );
            }
        }
        assert!(
            differs_from_generic > 0,
            "fixtures must exercise distinct FP32 orders"
        );
    }
    #[test]
    fn stock_vector_rejects_partial_buffers() {
        assert!(
            std::panic::catch_unwind(|| stock_vector_row(&wire(0, 1)[..109], &input(0, 1)))
                .is_err()
        );
        assert!(
            std::panic::catch_unwind(|| stock_vector_row(&wire(0, 1), &input(0, 1)[..291]))
                .is_err()
        );
    }
    #[test]
    fn stock_tiled_multiblock_rounding_and_unaligned_buffers() {
        for blocks in [0, 1, 2, 3, 6, 9, 36, 257] {
            for seed in 0..16 {
                let mut w = wire(seed, blocks);
                let mut a = input(seed, blocks);
                for b in 0..blocks {
                    let d = [
                        0x0001u16, 0x03ff, 0x0400, 0x3555, 0x7bff, 0x8000, 0xbc00, 0xfbff,
                    ][(b + seed) % 8];
                    w[b * BLOCK_W + 108..(b + 1) * BLOCK_W].copy_from_slice(&d.to_le_bytes());
                    a[b * BLOCK_A..b * BLOCK_A + 4].copy_from_slice(
                        &[0.0f32, -0.125, 0.1, 1.0e-20, 1.0e20][(b + seed) % 5].to_le_bytes(),
                    );
                }
                let expected = stock_tiled_reference(&w, &a);
                let mut wu = vec![0u8];
                wu.extend_from_slice(&w);
                let mut au = vec![0u8; 3];
                au.extend_from_slice(&a);
                assert_eq!(
                    stock_tiled_row(&wu[1..], &au[3..]).to_bits(),
                    expected.to_bits(),
                    "blocks={blocks} seed={seed}"
                );
            }
        }
    }
    #[test]
    fn stock_tiled_square_abi_keeps_sentinels_and_column_strides() {
        if mode() != Mode::FusedV2Stock {
            return;
        }
        for nrc in [1usize, 2, 4] {
            let blocks = 6;
            let weights: Vec<_> = (0..nrc).map(|i| wire(i, blocks)).collect();
            let acts: Vec<_> = (0..nrc).map(|i| input(i + 1, blocks)).collect();
            let wb: Vec<_> = weights.iter().flatten().copied().collect();
            let ab: Vec<_> = acts.iter().flatten().copied().collect();
            let mut out = vec![f32::from_bits(0x7fc01234); 2 + nrc * 16];
            unsafe {
                mem_bpb3k_dot_q8_stock_tiled(
                    (blocks * 256) as i32,
                    out.as_mut_ptr().add(1),
                    16,
                    wb.as_ptr().cast(),
                    blocks * BLOCK_W,
                    ab.as_ptr().cast(),
                    blocks * BLOCK_A,
                    nrc as i32,
                );
            }
            for (i, value) in out.iter().enumerate() {
                if i > 0 && (i - 1) / 16 < nrc && (i - 1) % 16 < nrc {
                    assert_eq!(
                        value.to_bits(),
                        stock_tiled_reference(&weights[(i - 1) % 16], &acts[(i - 1) / 16])
                            .to_bits()
                    );
                } else {
                    assert_eq!(value.to_bits(), 0x7fc01234);
                }
            }
        }
    }
    #[test]
    fn fused_v2_multiblock_float_order_unaligned_and_edge_scales() {
        for blocks in [0, 1, 2, 3, 7, 31, 257] {
            for seed in 0..16 {
                let mut w = wire(seed, blocks);
                let mut a = input(seed, blocks);
                for b in 0..blocks {
                    let d = [
                        0x0001u16, 0x03ff, 0x0400, 0x3555, 0x7bff, 0x8000, 0xbc00, 0xfbff,
                    ][(b + seed) % 8];
                    w[b * BLOCK_W + 108..(b + 1) * BLOCK_W].copy_from_slice(&d.to_le_bytes());
                    a[b * BLOCK_A..b * BLOCK_A + 4]
                        .copy_from_slice(&[0.0f32, -0.125, 1.0e-20, 1.0e20][b % 4].to_le_bytes());
                }
                let expected = reference(&w, &a);
                let mut unaligned_w = vec![0u8];
                unaligned_w.extend_from_slice(&w);
                let mut unaligned_a = vec![0u8; 3];
                unaligned_a.extend_from_slice(&a);
                let actual = fused_v2_row(&unaligned_w[1..], &unaligned_a[3..]);
                let ablation = row_v2(&unaligned_w[1..], &unaligned_a[3..], false);
                assert_eq!(
                    ablation.to_bits(),
                    expected.to_bits(),
                    "i16 blocks={blocks} seed={seed}"
                );
                assert_eq!(
                    actual.to_bits(),
                    expected.to_bits(),
                    "blocks={blocks} seed={seed}"
                );
            }
        }
    }
    #[test]
    fn fused_v2_rejects_partial_weights_and_activations() {
        assert!(
            std::panic::catch_unwind(|| fused_v2_row(&wire(0, 1)[..109], &input(0, 1))).is_err()
        );
        assert!(
            std::panic::catch_unwind(|| fused_v2_row(&wire(0, 1), &input(0, 1)[..291])).is_err()
        );
    }
    fn wire(seed: usize, blocks: usize) -> Vec<u8> {
        let mut data = vec![0u8; blocks * BLOCK_W];
        for (i, x) in data.iter_mut().enumerate() {
            *x = ((i * 37 + i / 3 * 13 + seed * 71) % 256) as u8;
        }
        for b in 0..blocks {
            data[b * BLOCK_W + 108..b * BLOCK_W + 110].copy_from_slice(&0x3c00u16.to_le_bytes());
        }
        data
    }
    fn input(seed: usize, blocks: usize) -> Vec<u8> {
        let mut data = vec![0u8; blocks * BLOCK_A];
        for b in 0..blocks {
            data[b * BLOCK_A..b * BLOCK_A + 4]
                .copy_from_slice(&(1.0f32 + (seed as f32) * 0.125).to_le_bytes());
            for i in 0..256 {
                data[b * BLOCK_A + 4 + i] = ((i * 17 + b * 31 + seed * 47) % 256) as u8;
            }
        }
        data
    }
    #[test]
    fn kernel_four_way_parity_real_layout() {
        for blocks in [1, 2, 3, 7] {
            let ws = (0..4).map(|r| wire(r, blocks)).collect::<Vec<_>>();
            let a = input(3, blocks);
            let refs = ws.iter().map(|w| reference(w, &a)).collect::<Vec<_>>();
            let slices = ws.iter().map(|w| w.as_slice()).collect::<Vec<_>>();
            for mode in [false, true] {
                let actual = grouped(&slices, &a, mode);
                for (i, (x, y)) in actual.iter().zip(refs.iter()).enumerate() {
                    assert_eq!(
                        x.to_bits(),
                        y.to_bits(),
                        "mode={mode} blocks={blocks} row={i}"
                    );
                }
            }
        }
    }
    #[test]
    fn fused_avx2_superblock_exact_eight_lane_oracle() {
        for blocks in [1, 2, 3, 7, 31] {
            let ws = (0..4).map(|r| wire(r + 11, blocks)).collect::<Vec<_>>();
            let act = input(17, blocks);
            let slices = ws.iter().map(|w| w.as_slice()).collect::<Vec<_>>();
            let actual = fused_rows(&slices, &act);
            for (row, w) in ws.iter().enumerate() {
                let expected = reference(w, &act);
                assert_eq!(
                    actual[row].to_bits(),
                    expected.to_bits(),
                    "row={row} blocks={blocks}"
                );
            }
        }
    }
    #[test]
    fn ggml_runtime_nrc2_uses_safe_two_by_two_scratch_layout() {
        let blocks = 4usize;
        let weights = (0..2).map(|row| wire(row + 7, blocks)).collect::<Vec<_>>();
        let activations = (0..2).map(|col| input(col + 2, blocks)).collect::<Vec<_>>();
        let mut wb = Vec::new();
        let mut ab = Vec::new();
        for row in &weights {
            wb.extend_from_slice(row);
        }
        for col in &activations {
            ab.extend_from_slice(col);
        }
        let mut output = [f32::NAN; 32]; // upstream ggml uses exactly tmp[32]
        unsafe {
            mem_bpb3k_dot_q8(
                (blocks * 256) as i32,
                output.as_mut_ptr(),
                16,
                wb.as_ptr() as *const c_void,
                blocks * BLOCK_W,
                ab.as_ptr() as *const c_void,
                blocks * BLOCK_A,
                2,
            );
        }
        for col in 0..2 {
            for row in 0..2 {
                let expected = reference(&weights[row], &activations[col]);
                assert_eq!(
                    output[col * 16 + row].to_bits(),
                    expected.to_bits(),
                    "col={col} row={row}"
                );
            }
        }
        assert!(output[2..16].iter().all(|x| x.is_nan()));
        assert!(output[18..32].iter().all(|x| x.is_nan()));
    }
    #[test]
    fn ggml_square_nrc4_abi_matches_16_reference_outputs() {
        let blocks = 3usize;
        let ws = (0..4).map(|row| wire(row, blocks)).collect::<Vec<_>>();
        let acts = (0..4).map(|col| input(col, blocks)).collect::<Vec<_>>();
        let mut output = [f32::NAN; 64];
        let old = std::env::var_os("MEM_BPB3K_KERNEL");
        // Standalone tests process has no env override by default.
        unsafe {
            mem_bpb3k_dot_q8(
                (blocks * 256) as i32,
                output.as_mut_ptr(),
                16,
                ws[0].as_ptr() as *const c_void,
                0,
                acts[0].as_ptr() as *const c_void,
                0,
                1,
            );
        }
        // Actual four-stride ABI: one contiguous buffer per tensor family.
        let mut wb = Vec::new();
        for w in &ws {
            wb.extend_from_slice(w);
        }
        let mut ab = Vec::new();
        for a in &acts {
            ab.extend_from_slice(a);
        }
        let mut got = [f32::NAN; 64];
        unsafe {
            mem_bpb3k_dot_q8(
                (blocks * 256) as i32,
                got.as_mut_ptr(),
                16,
                wb.as_ptr() as *const c_void,
                blocks * BLOCK_W,
                ab.as_ptr() as *const c_void,
                blocks * BLOCK_A,
                4,
            );
        }
        for col in 0..4 {
            for row in 0..4 {
                let expected = reference(&ws[row], &acts[col]);
                assert_eq!(
                    got[col * 16 + row].to_bits(),
                    expected.to_bits(),
                    "col={col} row={row}"
                );
            }
        }
        for col in 0..4 {
            for row in 4..16 {
                assert!(got[col * 16 + row].is_nan());
            }
        }
        assert!(output[0].is_finite());
        drop(old);
    }
    #[test]
    fn fp16_exact_widening_exhaustive_all_finite_values() {
        for bits in 0u16..=u16::MAX {
            let exponent = (bits >> 10) & 31;
            if exponent == 31 {
                continue;
            }
            let sign = if bits & 0x8000 != 0 { -1.0f32 } else { 1.0f32 };
            let mantissa = (bits & 1023) as f32;
            let expected = if exponent == 0 {
                sign * mantissa * 2.0f32.powi(-24)
            } else {
                sign * (1.0 + mantissa / 1024.0) * 2.0f32.powi(i32::from(exponent) - 15)
            };
            assert_eq!(fp16(bits).to_bits(), expected.to_bits(), "fp16={bits:04x}");
        }
    }
    #[test]
    fn coefficient_decode_signed_range() {
        let w = wire(5, 1);
        for i in 0..256 {
            assert!((-4..=3).contains(&coeff(&w, i)));
        }
    }
}
