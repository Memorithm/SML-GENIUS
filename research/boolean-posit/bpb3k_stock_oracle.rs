//! Link only in the diagnostic runner against the unchanged pinned upstream
//! AVX2 Q3_K function. This independent C oracle is never part of inference.
#[path = "bpb3k.rs"]
#[allow(dead_code)]
mod codec;
#[path = "bpb3k_cpu_kernels.rs"]
#[allow(dead_code)]
mod kernels;
use std::ffi::c_void;

extern "C" {
    fn original_q3_tiled_oracle(
        blocks: i32,
        weights: *const u8,
        activations: *const u8,
        out: *mut f32,
    );
    fn ggml_vec_dot_q3_K_q8_K(
        n: i32,
        s: *mut f32,
        bs: usize,
        x: *const c_void,
        bx: usize,
        y: *const c_void,
        by: usize,
        nrc: i32,
    );
}
fn main() {
    #[cfg(not(target_arch = "x86_64"))]
    panic!("STOP_STOCK_ORACLE_REQUIRES_X86_64");
    #[cfg(target_arch = "x86_64")]
    assert!(
        std::is_x86_feature_detected!("avx2")
            && std::is_x86_feature_detected!("fma")
            && std::is_x86_feature_detected!("f16c"),
        "STOP_STOCK_ORACLE_ISA"
    );
    let selected = std::env::var("MEM_BPB3K_KERNEL").unwrap();
    assert!(selected == "stock_vector" || selected == "fused_v2_stock");
    let mut random = 0x62d3_93ab_5f01_7cd9u64;
    let mut cases = 0;
    let mut tiled_cases = 0;
    for blocks in [1usize, 2, 3, 6, 9, 36, 257] {
        for case in 0..128usize {
            let mut raw = vec![0u8; blocks * 110];
            let mut act = vec![0u8; blocks * 292];
            for byte in raw.iter_mut().chain(act.iter_mut()) {
                random ^= random << 13;
                random ^= random >> 7;
                random ^= random << 17;
                *byte = random as u8;
            }
            let mut packed = Vec::with_capacity(raw.len());
            for b in 0..blocks {
                let d = [
                    0x0001u16, 0x03ff, 0x0400, 0x3555, 0x7bff, 0x8000, 0xbc00, 0xfbff,
                ][(b + case) % 8];
                raw[b * 110 + 108..(b + 1) * 110].copy_from_slice(&d.to_le_bytes());
                act[b * 292..b * 292 + 4].copy_from_slice(
                    &[0.0f32, -0.125, 0.1, 1.0e-20, 1.0e20][(case + b) % 5].to_le_bytes(),
                );
                if case % 3 == 0 {
                    act[b * 292 + 4..b * 292 + 260].fill(128);
                }
                if case % 3 == 1 {
                    act[b * 292 + 4..b * 292 + 260].fill(127);
                }
                packed.extend_from_slice(
                    &codec::q3_to_bpb3k(raw[b * 110..(b + 1) * 110].try_into().unwrap()).unwrap(),
                );
            }
            let mut expected = 0.0f32;
            let mut actual = 0.0f32;
            unsafe {
                ggml_vec_dot_q3_K_q8_K(
                    (blocks * 256) as i32,
                    &mut expected,
                    0,
                    raw.as_ptr().cast(),
                    0,
                    act.as_ptr().cast(),
                    0,
                    1,
                );
                kernels::mem_bpb3k_dot_q8(
                    (blocks * 256) as i32,
                    &mut actual,
                    0,
                    packed.as_ptr().cast(),
                    0,
                    act.as_ptr().cast(),
                    0,
                    1,
                );
            }
            assert!(expected.is_finite() && actual.is_finite());
            assert_eq!(
                actual.to_bits(),
                expected.to_bits(),
                "blocks={blocks} case={case} expected={expected} actual={actual}"
            );
            cases += 1;
            if selected == "fused_v2_stock" && case < 16 {
                unsafe {
                    original_q3_tiled_oracle(
                        blocks as i32,
                        raw.as_ptr(),
                        act.as_ptr(),
                        &mut expected,
                    );
                    kernels::mem_bpb3k_dot_q8_stock_tiled(
                        (blocks * 256) as i32,
                        &mut actual,
                        0,
                        packed.as_ptr().cast(),
                        0,
                        act.as_ptr().cast(),
                        0,
                        1,
                    );
                }
                assert!(expected.is_finite() && actual.is_finite());
                assert_eq!(
                    actual.to_bits(),
                    expected.to_bits(),
                    "tiled blocks={blocks} case={case}"
                );
                tiled_cases += 1;
            }
        }
    }
    println!("PASS_BPB3K_ORIGINAL_AVX2_ORACLE cases={cases} exact_bits=true boundary=kernel_only_not_model_qualification");
    if tiled_cases > 0 {
        println!("PASS_BPB3K_ORIGINAL_TILED_ORACLE cases={tiled_cases} exact_bits=true boundary=kernel_only_not_model_qualification");
    }
}
