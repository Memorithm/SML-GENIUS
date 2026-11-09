#!/usr/bin/env bash
set -euo pipefail
# Extract unchanged upstream function bodies; do not copy/rewrite the oracle.
upstream=${1:?pinned upstream tree required}
work=${2:?diagnostic output directory required}
mode=${3:-fused_v2_stock}
test "$(git -C "$upstream" rev-parse HEAD)" = f7591a6a01f9273de03331732bdc0b39721168be
mkdir -p "$work"
python3 - "$upstream" "$work" <<'PY'
from pathlib import Path
import subprocess
import sys
root, output = map(Path, sys.argv[1:])
for name in ('ggml/src/ggml-cpu/tiled/tiled-kernel.cpp',
             'ggml/src/ggml-cpu/tiled/tiled-kernel.h',
             'ggml/src/ggml-common.h', 'ggml/include/ggml.h'):
    if (root / name).read_bytes() != subprocess.check_output(['git', '-C', str(root), 'show', 'HEAD:' + name]):
        raise SystemExit('STOP_TILED_ORACLE_SOURCE_MODIFIED')
relative = 'ggml/src/ggml-cpu/arch/x86/quants.c'
source = (root / relative).read_bytes()
if source != subprocess.check_output(['git', '-C', str(root), 'show', 'HEAD:' + relative]):
    raise SystemExit('STOP_STOCK_ORACLE_SOURCE_MODIFIED')
source = source.decode()
def section(start, end):
    if source.count(start) != 1 or source.count(end) != 1:
        raise SystemExit('STOP_STOCK_ORACLE_SOURCE_LAYOUT_DRIFT')
    return source.split(start, 1)[1].split(end, 1)[0]
hsum = 'static inline float hsum_float_8' + section('static inline float hsum_float_8', '// horizontally add 8 int32_t')
scales = 'static inline __m256i get_scale_shuffle_q3k' + section('static inline __m256i get_scale_shuffle_q3k', 'static inline __m256i get_scale_shuffle_k4')
start = 'void ggml_vec_dot_q3_K_q8_K('
function = start + section(start, 'void ggml_vec_dot_q4_K_q8_K(')
function = function.split('#elif defined __AVX__', 1)[0] + '\n#endif\n}\n'
header = '''#include <immintrin.h>
#include <stdint.h>
#include <string.h>
#include <assert.h>
#define GGML_RESTRICT restrict
#define QK_K 256
#define UNUSED(x) (void)(x)
#define MM256_SET_M128I(hi, lo) _mm256_inserti128_si256(_mm256_castsi128_si256(lo), hi, 1)
#define GGML_CPU_FP16_TO_FP32(x) _cvtsh_ss(x)
typedef struct { uint8_t hmask[32], qs[64], scales[12]; uint16_t d; } block_q3_K;
typedef struct { float d; int8_t qs[256]; int16_t bsums[16]; } block_q8_K;
_Static_assert(sizeof(block_q3_K) == 110, "Q3_K ABI");
_Static_assert(sizeof(block_q8_K) == 292, "Q8_K ABI");
'''
(output / 'original-q3.c').write_text(header + hsum + scales + function)
PY
cc -std=c11 -O2 -mavx2 -mfma -mf16c -ffp-contract=off -c "$work/original-q3.c" -o "$work/original-q3.o"
c++ -std=c++17 -O2 -mavx2 -mfma -mf16c -ffp-contract=off \
    -I"$upstream/ggml/src" -I"$upstream/ggml/src/ggml-cpu" -I"$upstream/ggml/include" \
    -c "$upstream/ggml/src/ggml-cpu/tiled/tiled-kernel.cpp" -o "$work/original-tiled.o"
c++ -std=c++17 -O2 -mavx2 -mfma -mf16c -ffp-contract=off \
    -I"$upstream/ggml/src" -I"$upstream/ggml/src/ggml-cpu" -I"$upstream/ggml/include" \
    -c research/boolean-posit/bpb3k_tiled_oracle.cpp -o "$work/tiled-fixture.o"
ar rcs "$work/liboriginal_q3.a" "$work/original-q3.o" "$work/original-tiled.o" "$work/tiled-fixture.o"
rustc +1.89.0 --edition=2021 -D warnings -O research/boolean-posit/bpb3k_stock_oracle.rs \
    -L "native=$work" -l static=original_q3 -o "$work/stock-oracle"
MEM_BPB3K_KERNEL="$mode" "$work/stock-oracle"
if [[ -n "${4:-}" ]]; then
  cat > "$work/dispatch-policy.c" <<'C'
#define _POSIX_C_SOURCE 200809L
#define MEM_BPB3K_DOT 1
#include "ggml-bpb3k-experimental.h"
static int selected = 1;
int mem_bpb3k_uses_stock_order(void) { return selected; }
int main(void) {
    unsetenv("GGML_CPU_TILED_MM"); unsetenv("GGML_CPU_TILED_MM_FORCE");
    assert(!mem_bpb3k_stock_tiled_enabled(0, 1, 0));
    assert(!mem_bpb3k_stock_tiled_enabled(0, 7, 0));
    assert(mem_bpb3k_stock_tiled_enabled(0, 8, 0));
    assert(!mem_bpb3k_stock_tiled_enabled(1, 8, 0));
    assert(!mem_bpb3k_stock_tiled_enabled(0, 8, 1));
    selected = 0; assert(!mem_bpb3k_stock_tiled_enabled(0, 8, 0)); selected = 1;
    setenv("GGML_CPU_TILED_MM_FORCE", "1", 1);
    assert(mem_bpb3k_stock_tiled_enabled(0, 1, 0));
    setenv("GGML_CPU_TILED_MM", "0", 1);
    assert(!mem_bpb3k_stock_tiled_enabled(0, 8, 0));
    assert(!mem_bpb3k_stock_tiled_enabled(0, 1, 0));
    setenv("GGML_CPU_TILED_MM", "1", 1);
    setenv("GGML_CPU_TILED_MM_FORCE", "2", 1);
    assert(!mem_bpb3k_stock_tiled_enabled(0, 1, 0));
    puts("PASS_BPB3K_STOCK_DISPATCH_POLICY cases=10");
}
C
  cc -std=c11 -O2 -mavx2 -mfma -I"$(dirname "$4")" "$work/dispatch-policy.c" -o "$work/dispatch-policy"
  "$work/dispatch-policy"
fi
