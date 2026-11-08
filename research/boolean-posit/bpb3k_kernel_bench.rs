//! Diagnostic only: run once per process with MEM_BPB3K_KERNEL=fused/fused_v2.
#[path = "bpb3k_cpu_kernels.rs"]
#[allow(dead_code)]
mod kernels;
use std::{ffi::c_void, hint::black_box, time::Instant};
fn main() {
    for blocks in [3usize, 9, 36] {
        let mut w = vec![0u8; blocks * 110];
        let mut a = vec![0u8; blocks * 292];
        for (i, x) in w.iter_mut().enumerate() {
            *x = (i * 73 + i / 11) as u8;
        }
        for (i, x) in a.iter_mut().enumerate() {
            *x = (i * 37 + i / 7) as u8;
        }
        for b in 0..blocks {
            w[b * 110 + 108..b * 110 + 110].copy_from_slice(&0x3555u16.to_le_bytes());
            a[b * 292..b * 292 + 4].copy_from_slice(&0.125f32.to_le_bytes());
        }
        let mut out = 0f32;
        for _ in 0..100 {
            unsafe {
                kernels::mem_bpb3k_dot_q8(
                    (blocks * 256) as i32,
                    &mut out,
                    1,
                    w.as_ptr().cast::<c_void>(),
                    0,
                    a.as_ptr().cast::<c_void>(),
                    0,
                    1,
                );
            }
        }
        let start = Instant::now();
        for _ in 0..20000 {
            unsafe {
                kernels::mem_bpb3k_dot_q8(
                    (blocks * 256) as i32,
                    black_box(&mut out),
                    1,
                    black_box(w.as_ptr()).cast(),
                    0,
                    black_box(a.as_ptr()).cast(),
                    0,
                    1,
                );
            }
        }
        println!("blocks={blocks} iterations=20000 elapsed_ns={} result_bits={:08x} boundary=kernel_diagnostic_only",start.elapsed().as_nanos(),out.to_bits());
    }
}
