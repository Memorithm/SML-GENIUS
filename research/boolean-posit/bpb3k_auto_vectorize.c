/*
 * Fully timed Q3_K decoding + auto-vectorizable signed-Q8_K integer dot.
 * This is independently authored C, not copied upstream ggml code.
 *
 * Compile this *identical* translation unit as:
 *   Clang -O3 -march=haswell (automatic loop/SLP vectorization enabled)
 *   Clang -O3 -march=haswell -fno-vectorize -fno-slp-vectorize
 *   GCC   -O3 -march=haswell (automatic vectorization enabled)
 * Define MEM_AUTOVEC_SYMBOL separately per compiler build to preserve all
 * three implementations in the same executable and control source drift.
 *
 * Strictly research-only: one Q3_K original 110-byte block and one signed
 * Q8_K 292-byte block. Returns exact *integer* dot. Weight and activation
 * floating scale metadata are deliberately not applied by this function.
 */
#include <stdint.h>
#include <stddef.h>

#ifndef MEM_AUTOVEC_SYMBOL
#error "Define MEM_AUTOVEC_SYMBOL to distinguish compiler variants"
#endif

#if defined(__GNUC__) || defined(__clang__)
#define MEM_NOINLINE __attribute__((noinline, used))
#else
#define MEM_NOINLINE
#endif

MEM_NOINLINE
int32_t MEM_AUTOVEC_SYMBOL(const uint8_t *restrict q3, const uint8_t *restrict q8) {
    /* Decode Q3_K directly from ORIGINAL wire bytes, not BPB3-K.
     * The 256-element temporary array is allocated and populated *inside*
     * this function, so its entire decode cost is measured on every dot.
     * No heap allocation or persistent dense weight cache.
     */
    _Alignas(32) int16_t signed_scaled_weights[256];
    for (size_t i = 0; i < 256; ++i) {
        const size_t half = i / 128;
        const size_t plane = (i % 128) / 32;
        const size_t lane = i % 32;
        const uint8_t low_two = (uint8_t)((q3[32 + half * 32 + lane] >> (2 * plane)) & 3);
        const uint8_t sign_bit = (uint8_t)((q3[lane] >> (half * 4 + plane)) & 1);
        const int32_t signed_q = (int32_t)low_two - (sign_bit == 0 ? 4 : 0);
        const size_t subgroup = i / 16;
        const uint8_t low_scale = (uint8_t)(subgroup < 8
            ? (q3[96 + subgroup] & 15)
            : (q3[96 + subgroup - 8] >> 4));
        const uint8_t upper_scale = (uint8_t)((q3[104 + subgroup % 4] >> (2 * (subgroup / 4))) & 3);
        const int32_t signed_scale = (int32_t)(low_scale | (upper_scale << 4)) - 32;
        signed_scaled_weights[i] = (int16_t)(signed_q * signed_scale);
    }

    /* This regular dot loop is the candidate for automatic vectorization.
     * Each contribution is within +/- 16,384 and the sum within
     * +/- 4,194,304: 32-bit signed accumulation cannot overflow.
     * Casting through int8_t preserves the full signed Q8_K domain.
     */
    int32_t result = 0;
    for (size_t i = 0; i < 256; ++i) {
        result += (int32_t)signed_scaled_weights[i] * (int32_t)(int8_t)q8[4 + i];
    }
    return result;
}
