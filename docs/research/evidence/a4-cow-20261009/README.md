# A4 copy-on-write qualification boundary

The native A4 closed-loop public example is rerun on aarch64 with unchanged
full-output SHA-256. `scripts/measure-a4-cow.sh` samples `/proc/<pid>/status`
VmRSS during each run. Sampling can miss short peaks and the shell wrapper adds
noise; zero or low observations are not evidence of no allocations. This script
is diagnostic, not a controlled before/after benchmark, and it does not measure
per-component clones, GPU memory or inference throughput.

A speedup or physical memory saving needs a matched baseline, pinned hardware,
identical input and compiler settings, multiple samples and resource isolation.
The mandatory frozen fixture and independent arithmetic verifier remain the
correctness admission, independent of these diagnostic samples. No source-tree
output report is overwritten by the measurement script.
