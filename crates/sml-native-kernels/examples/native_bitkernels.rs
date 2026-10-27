//! Physical CPU benchmark only; optional real GPU correctness smoke.
//! Run --release. Never infer SML-GENIUS inference throughput from this microbench.

use sml_native_kernels::{CpuKernels, PtxOperation};
use std::{hint::black_box, time::Instant};

fn benchmark(label: &str, size: usize, iterations: usize, kernels: CpuKernels) -> u64 {
    let left: Vec<u64> = (0..size)
        .map(|i| (i as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15))
        .collect();
    let right: Vec<u64> = left
        .iter()
        .map(|x| x.rotate_left(17) ^ 0x519d_28c3_729a_45fe)
        .collect();
    let start = Instant::now();
    let mut checksum = 0_u64;
    for _ in 0..iterations {
        for (&a, &b) in left.iter().zip(&right) {
            checksum =
                checksum.wrapping_add(u64::from(kernels.hamming(black_box(a), black_box(b))));
        }
    }
    let nanos = start.elapsed().as_nanos();
    let per_word = nanos as f64 / (size * iterations) as f64;
    println!(
        "cpu_hamming,{},size={},iterations={},nanos_total={},nanos_per_word={:.4},checksum={}",
        label, size, iterations, nanos, per_word, checksum
    );
    checksum
}

fn main() {
    let reference = CpuKernels::portable();
    let selected = CpuKernels::detect();
    let iterations = 80;
    let size = 8192;
    println!(
        "native_bitkernels,arch={},os={},selected={:?},reference={:?}",
        std::env::consts::ARCH,
        std::env::consts::OS,
        selected.path(),
        reference.path()
    );
    let reference_sum = benchmark("portable", size, iterations, reference);
    let native_sum = benchmark("native", size, iterations, selected);
    assert_eq!(reference_sum, native_sum, "CPU native/reference mismatch");

    println!(
        "gpu_ptx_available_as_source=true,device_executed=false,driver_feature={}",
        cfg!(all(target_os = "linux", feature = "cuda-driver"))
    );
    #[cfg(all(target_os = "linux", feature = "cuda-driver"))]
    {
        if std::env::var_os("SML_NATIVE_GPU_TEST").is_some() {
            let size = 1031;
            let a: Vec<u32> = (0..size)
                .map(|i| (i as u32).wrapping_mul(0x9e37_79b9))
                .collect();
            let b: Vec<u32> = (0..size).map(|i| (i as u32).rotate_left(11)).collect();
            let c: Vec<u32> = (0..size).map(|i| !(i as u32)).collect();
            for op in [
                PtxOperation::Xor3,
                PtxOperation::Hamming,
                PtxOperation::TernaryPartial,
            ] {
                let start = Instant::now();
                let gpu = sml_native_kernels::cuda_driver::execute(op, &a, &b, &c)
                    .expect("opted into real NVIDIA GPU validation");
                let nanos = start.elapsed().as_nanos();
                let oracle: Vec<_> = a
                    .iter()
                    .zip(&b)
                    .zip(&c)
                    .map(|((&x, &y), &z)| sml_native_kernels::ptx_word_oracle(op, x, y, z))
                    .collect();
                assert_eq!(gpu, oracle, "gpu={op:?}");
                println!(
                    "gpu_roundtrip,operation={:?},words={},nanos_total={},exact=true",
                    op, size, nanos
                );
            }
        }
    }
    // Avoid an unused import under the default feature path.
    let _ = PtxOperation::Xor3.symbol();
}
