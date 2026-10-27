//! Hardware-only resident CUDA PTX panel. Requires an actual Linux NVIDIA
//! device. Reports cold context/JIT, H2D, GPU event, D2H separately; it is
//! NOT a single-token latency or SML-GENIUS inference benchmark.

#[cfg(all(target_os = "linux", feature = "cuda-driver"))]
fn main() {
    use sml_native_kernels::{cuda_driver::ResidentGpu, ptx_word_oracle, PtxOperation};
    use std::time::Instant;

    assert!(
        std::env::var_os("SML_NATIVE_GPU_TEST").is_some(),
        "explicit SML_NATIVE_GPU_TEST=1 required for device timing"
    );

    let capacity = 65_536_usize;
    let start = Instant::now();
    let mut gpu = ResidentGpu::new(capacity).expect("real GPU, PTX JIT and resident allocation");
    let cold_init_ns = start.elapsed().as_nanos();
    println!(
        "SML_NATIVE_RESIDENT_INIT,capacity_words={capacity},cold_context_jit_alloc_ns={cold_init_ns}"
    );

    // Run two shapes through the same device allocations, compiled kernel
    // function handles and context. The second input is fully different.
    for &words in &[1031_usize, capacity] {
        let a: Vec<u32> = (0..words)
            .map(|i| (i as u32).wrapping_mul(0x9e37_79b9) ^ (words as u32))
            .collect();
        let b: Vec<u32> = (0..words)
            .map(|i| (!(i as u32)).rotate_left(11) ^ (words as u32))
            .collect();
        let c: Vec<u32> = (0..words)
            .map(|i| (i as u32).rotate_right(13) ^ 0x4a9b_41c1)
            .collect();
        let upload_start = Instant::now();
        gpu.upload(&a, &b, &c).expect("write resident input planes");
        let upload_ns = upload_start.elapsed().as_nanos();

        for op in [
            PtxOperation::Xor3,
            PtxOperation::Hamming,
            PtxOperation::TernaryPartial,
        ] {
            let iterations = 512_u32;
            let launch_start = Instant::now();
            let t = gpu
                .benchmark_device(op, words, iterations)
                .expect("CUDA event time for warmed-up resident GPU");
            let submit_and_sync_ns = launch_start.elapsed().as_nanos();
            let read_start = Instant::now();
            let observed = gpu.read_output(words).expect("resident result D2H copy");
            let read_ns = read_start.elapsed().as_nanos();
            let mut checksum = 0_u64;
            for (index, &actual) in observed.iter().enumerate() {
                let expected = ptx_word_oracle(op, a[index], b[index], c[index]);
                assert_eq!(
                    actual, expected,
                    "GPU bit-exact mismatch operation={op:?},length={words},index={index}"
                );
                checksum = checksum.wrapping_add(u64::from(actual));
            }
            println!(
                "SML_NATIVE_RESIDENT,operation={op:?},words={words},iterations={},cold_context_jit_alloc_ns={cold_init_ns},upload_ns={upload_ns},device_events_total_ms={:.6},device_ms_per_launch={:.9},host_submit_and_sync_ns={submit_and_sync_ns},download_ns={read_ns},checksum={checksum},exact=true",
                t.iterations,
                t.total_device_ms,
                t.device_ms_per_launch,
            );
        }
    }
    println!("SML_NATIVE_RESIDENT_THOR_ALL_OPERATIONS=PASS");
}

#[cfg(not(all(target_os = "linux", feature = "cuda-driver")))]
fn main() {
    eprintln!("Linux CUDA driver feature required; GPU measurements were not run.");
}
