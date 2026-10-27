//! A3: matched optimized CPU versus real resident CUDA PTX, at two sizes.
//! Research instrumentation only. No dispatch into SML-GENIUS inference.

#[cfg(all(target_os = "linux", feature = "cuda-driver"))]
mod hardware {
    use sml_native_kernels::{cuda_driver::ResidentGpu, ptx_word_oracle, PtxOperation};
    use std::{hint::black_box, time::Instant};

    const ROUNDS: usize = 7;
    const BATCHES: u32 = 128;

    fn median_u128(values: &mut [u128]) -> u128 {
        values.sort_unstable();
        values[values.len() / 2]
    }

    fn median_f64(values: &mut [f64]) -> f64 {
        values.sort_by(|a, b| a.total_cmp(b));
        values[values.len() / 2]
    }

    #[inline(always)]
    fn portable_into(op: PtxOperation, a: &[u32], b: &[u32], c: &[u32], out: &mut [u32]) {
        for (((&x, &y), &z), dst) in a.iter().zip(b).zip(c).zip(out.iter_mut()) {
            *dst = ptx_word_oracle(op, x, y, z);
        }
    }

    fn portable_copy(op: PtxOperation, a: &[u32], b: &[u32], c: &[u32]) -> Vec<u32> {
        let mut output = vec![0_u32; a.len()];
        portable_into(op, a, b, c, &mut output);
        output
    }

    pub fn run() {
        assert!(
            std::env::var_os("SML_NATIVE_GPU_TEST").is_some(),
            "physical Thor GPU required (SML_NATIVE_GPU_TEST=1)"
        );
        let init = Instant::now();
        let mut gpu = ResidentGpu::new(65_536).expect("CUDA/PTX hardware required");
        let init_ns = init.elapsed().as_nanos();
        println!("NATIVE_A3_INIT,capacity_words=65536,cold_context_jit_alloc_ns={init_ns}");

        for words in [1031_usize, 65_536] {
            let a: Vec<u32> = (0..words)
                .map(|i| (i as u32).wrapping_mul(0x9e37_79b9) ^ 0xa5a5_0321)
                .collect();
            let b: Vec<u32> = (0..words)
                .map(|i| !(i as u32).rotate_left(17) ^ 0x7263_9a0d)
                .collect();
            let c: Vec<u32> = (0..words)
                .map(|i| (i as u32).rotate_right(7) ^ 0x4d56_2b61)
                .collect();

            for operation in [
                PtxOperation::Xor3,
                PtxOperation::Hamming,
                PtxOperation::TernaryPartial,
            ] {
                let expected = portable_copy(operation, &a, &b, &c);
                let mut cpu_batch_ns = Vec::with_capacity(ROUNDS);
                let mut gpu_event_ns = Vec::with_capacity(ROUNDS);
                let mut cpu_end_to_end_ns = Vec::with_capacity(ROUNDS);
                let mut gpu_end_to_end_ns = Vec::with_capacity(ROUNDS);
                let mut output = vec![0_u32; words];

                for _ in 0..ROUNDS {
                    // The optimized Rust baseline writes every output word,
                    // reuses output storage, and is not artificially scalarized.
                    let cpu_start = Instant::now();
                    for _ in 0..BATCHES {
                        portable_into(
                            black_box(operation),
                            black_box(&a),
                            black_box(&b),
                            black_box(&c),
                            &mut output,
                        );
                    }
                    let elapsed_cpu = cpu_start.elapsed().as_nanos();
                    assert_eq!(output, expected);
                    black_box(&output);
                    cpu_batch_ns.push(elapsed_cpu / u128::from(BATCHES));

                    gpu.upload(&a, &b, &c).expect("resident H2D");
                    let elapsed_gpu = gpu
                        .benchmark_device(operation, words, BATCHES)
                        .expect("real CUDA events");
                    let gpu_ns = f64::from(elapsed_gpu.device_ms_per_launch) * 1_000_000.0;
                    assert!(gpu_ns.is_finite() && gpu_ns >= 0.0);
                    gpu_event_ns.push(gpu_ns);
                    assert_eq!(gpu.read_output(words).expect("D2H oracle"), expected);

                    let cpu_one = Instant::now();
                    let result_cpu = portable_copy(operation, &a, &b, &c);
                    let cpu_time = cpu_one.elapsed().as_nanos();
                    assert_eq!(result_cpu, expected);
                    cpu_end_to_end_ns.push(cpu_time);

                    // Full per-request CPU-host-visible GPU path: inputs
                    // copy H2D, kernel launch, sync and D2H output.
                    // Excludes initial CUDA context / PTX JIT / allocations.
                    let gpu_one = Instant::now();
                    gpu.upload(&a, &b, &c).expect("device input upload");
                    let result_gpu = gpu.execute(operation, words).expect("full GPU copy and launch");
                    let gpu_time = gpu_one.elapsed().as_nanos();
                    assert_eq!(result_gpu, expected);
                    gpu_end_to_end_ns.push(gpu_time);
                }

                println!(
                    "NATIVE_A3_MATCHED,operation={operation:?},words={words},rounds={ROUNDS},batches={BATCHES},cpu_kernel_median_ns={},gpu_event_kernel_median_ns={:.3},cpu_full_median_ns={},gpu_full_median_ns={},cold_context_jit_alloc_ns={init_ns},exact=true",
                    median_u128(&mut cpu_batch_ns),
                    median_f64(&mut gpu_event_ns),
                    median_u128(&mut cpu_end_to_end_ns),
                    median_u128(&mut gpu_end_to_end_ns),
                );
            }
        }
        println!("NATIVE_A3_MATCHED_ALL_SIX=PASS");
    }
}

#[cfg(all(target_os = "linux", feature = "cuda-driver"))]
fn main() {
    hardware::run();
}

#[cfg(not(all(target_os = "linux", feature = "cuda-driver")))]
fn main() {
    eprintln!("A3 matched hardware benchmark requires Linux and cuda-driver feature.");
}
