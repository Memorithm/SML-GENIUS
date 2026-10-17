# RTLR-V3 temporal diagnostic (Rust-only, proposed experiment)

This opt-in diagnostic follows the measured 100× native [Dell/Thor report](https://github.com/Memorithm/RemoteOps/blob/main/docs/SML_RTLR_V3_THOR_DELL_100X_MEASURED_20261017.md). On the four-lag parity task, every measured arm scored **more than 1.0 bit per target position** on generated DEV. A constant predictor of 0.5 has exactly **1.0 bit per binary target position** regardless of label balance. The 100× result is therefore **not** an absolute learning gain.

## Task isolation

The existing 4-argument `sml-rtlr-multiscale-bench MODE SEED UNIQUE_BANK_DIR` remains unchanged, including 419,430,400 TRAIN / 104,857,600 DEV and its `RTLR_TIMESCALE_CPU_V1` 19-column output for archived jobs.

The new **7-argument opt-in** CLI is:

```sh
cargo run --locked --release -p sml-core --bin sml-rtlr-multiscale-bench -- scalar 11 /tmp/rtlr-diagnostic-scalar-11 lag1 524288 65536
```

For new runs, always use **a fresh unique bank directory** for `v2-8`, `v3-8`, `v3-16` (including fresh process/seed/task isolation). Arguments after `BANK_DIR`: `TASK TRAIN DEV`. The output prefix becomes `RTLR_TEMPORAL_DIAG_V1` and adds **task name** before the original 18 fields; this intentionally does *not* satisfy the historical 100× schema. TRAIN must be a multiple of 524,288 between 524,288 and 419,430,400; DEV a multiple of 65,536 between 65,536 and 104,857,600.

| Task | Target | Purpose |
|---|---|---|
| `anf2` | parity of `bytes[t-2] & bytes[t-1]` | immediate nonlinear input/readout control |
| `lag1` | parity of `bytes[t-1]` | one-step input signal control |
| `lag16` | parity of `bytes[t-16]` | short memory |
| `lag128` | parity of `bytes[t-128]` | medium memory |
| `lag1024` | parity of `bytes[t-1024]` | long memory |
| `parity4` | XOR of those lags (1,16,128,1024) | nonlinear integration across time |

All labels use already-seen bytes; the model's two input bytes remain identical across tasks. Each task is generated deterministically in contiguous 65,536-position source blocks plus 1,024 bytes of history; 8 TRAIN documents, independent-seeded read-only DEV, same RTRL scalar/V2/V3 optimizer and train-only capacity policy. The sequence seed does **not** change with model seed, so paired trials see exactly the same generated training/DEV positions. Rust tests validate every task's boundary/label parity and argument validation.

## Scientific admission

This is a *diagnostic* experiment, not an automatically accepted new algorithm. The directly observed loss on generated DEV must be compared with a constant `p=0.5` control (1 bit per target position); simply beating RTRL-2 is insufficient. Report all tasks / arms / three model seeds, widths actually reached, per-process RAM, train/DEV CPU rates excluding stream generation, and exact SML commit + runner provenance. A capacity ceiling `16` does not mean 16 cells were ever activated.

Do not transfer SEC data to Thor; **G0 NOT_ADMITTED**, frozen **2024 TEST UNOPENED**. Synthetic diagnostic conclusions must not be generalized to natural-language performance or 50M/25B physical MOR/ANF training. Run strict CI and actual native CPU paired evidence before claiming further performance progress.
