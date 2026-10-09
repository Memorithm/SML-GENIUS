# Matched native A4 copy-on-write comparison

Build the same `pvp6_closed_loop_qualification` release example from the
pre-optimization commit `4fdc5fef4034c01d9ad6e7fddf69011be474b48e` and
an exact integrated optimization commit, using Rust 1.89.0, locked dependencies,
and separate target directories. Run `scripts/compare-a4-cow.sh BASE CURRENT`
on aarch64 with both executables already built. It checks the unchanged frozen
output SHA-256 for every sample and alternates execution order for 10 pairs.

The script records wall-clock time including launch/sampling and polls Linux
VmHWM. Short processes can finish before a sample, so sampled VmHWM may be zero;
never interpret zero as zero memory. This is diagnostic evidence, not a controlled
inference throughput, heap-allocation, GPU memory or energy benchmark. Record
host load, affinity, compiler, kernel and source SHA separately before claiming
a physical improvement. No regression threshold or automatic architecture
promotion is defined here.
