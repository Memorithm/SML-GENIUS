# Exact bit-packed causal ANF delay-line upper bound (non-learned)

This Rust-only independent oracle compares the **same causal ANF pair input** as [RTLR V3 matched delayed-ANF measured report](https://github.com/Memorithm/RemoteOps/blob/main/docs/SML_RTLR_V3_MATCHED_ANF_MEASURED_20261017.md). That fixed-source run had RTLR-V3 at **0.974432743 bit/target position** at delay16 and **1.000432448** at delay1023, against frozen TRAIN-constant baselines **1.000007848** and **1.000008669** respectively.

## Bit-packed deterministic reference

`sml_core::anf_delay_line::AnfDelayLine` is a reusable Rust primitive: dynamic capacity `1..=1_048_576` observed ANF bits, `Vec<u64>` packed storage, no raw future labels, no NVIDIA software, no coupling to model weights. The capacity-1024 reference needs exactly **128 bytes** of packed feature backing storage (excludes allocator and metadata). It stores the Boolean function `parity(source[t-2] & source[t-1])` at arrival t, and retrieves exactly 1, 16, 128 or 1023 feature observations causally. This is a **functional oracle, not learned** and not a fair trainable-parameter matched architecture comparison.

The standalone `sml-rtlr-anf-delay-oracle-bench` repeats the 524,288 synthetic TRAIN and 65,536 generated independent DEV supervised positions used by the RTLR diagnostic. `PANEL=0` has **exact original input source seeds**; panels 1,2 introduce separately generated deterministic streams. Execute `cargo run --locked --release -p sml-core --bin sml-rtlr-anf-delay-oracle-bench -- 0 16`.

For each panel/lag run, fit an unconditional constant predictor and a **no-memory** two-state predictor from the *current* ANF feature using **TRAIN labels only**, with Laplace smoothing. Both score separate generated DEV. The explicit delay line has **zero trainable weights** and must achieve zero hard-label mismatches against an independently indexed DEV label. Any mismatch is a correctness regression, not a neural training issue. Timings cover oracle ring update, read, target-label verification and baseline logloss; they **exclude source generation, allocation and TRAIN baseline fitting**. Raw throughput is therefore not directly comparable to learned RTLR kernel throughput.

Output schema: `RTLR_ANF_ORACLE_V1 panel lag train dev mismatches no_memory_bpc train_prior_bpc kernel_positions_per_sec packed_bytes train_positive dev_positive` (12 tab-separated columns).

Tests enforce source seed preservation for panel0, causal ring wrap/correct labels at 1/16/128/1023, packed-storage size, illegal lag/panel guards. The goal is a **correct bounded memory upper bound** and a control for independent data-seed sensitivity, not a new RTRL performance claim.

No SEC transfer, no 50M physical MOR atlas, no trained SML25B, no GPU execution. Rights **G0 NOT_ADMITTED**; held-out 2024 TEST **UNOPENED**.
