// Diagnostic fixture adapter only. Arithmetic/repacking is executed by the
// unchanged SHA-pinned upstream tiled-kernel.cpp, never by a copied kernel.
#include "tiled/tiled-kernel.h"
#include <cstring>

extern "C" void original_q3_tiled_oracle(int blocks, const uint8_t * weights,
                                        const uint8_t * activations, float * out) {
    tiled_tile_src0 w;
    tiled_tile_src1 a;
    alignas(64) float result[16 * 16] = {};
    for (int b = 0; b < blocks; ++b) {
        std::memset(&w, 0, sizeof(w));
        std::memset(&a, 0, sizeof(a));
        const uint8_t * raw = weights + b * 110;
        const uint8_t * act = activations + b * 292;
        uint16_t d;
        std::memcpy(&d, raw + 108, 2);
        w.d[0] = _cvtsh_ss(d);
        std::memcpy(&a.d[0], act, 4);
        for (int i = 0; i < 256; ++i) {
            const int local = i % 32, half = i / 128, plane = (i % 128) / 32;
            w.q[i] = ((raw[32 + half * 32 + local] >> (2 * plane)) & 3) +
                ((raw[local] & (1 << (half * 4 + plane))) ? 4 : 0);
            a.q[i] = static_cast<int8_t>(act[4 + i]);
            a.bsums[(i / 16) * TILED_TILE_ROWS] += a.q[i];
        }
        for (int g = 0; g < 16; ++g) {
            const int low = g < 8 ? raw[96 + g] & 15 : raw[96 + g - 8] >> 4;
            const int high = (raw[104 + g % 4] >> (2 * (g / 4))) & 3;
            w.scales[g] = (low | (high << 4)) - 32;
        }
        tiled_repack_src0<16>(&w, 1, 1, 4, true);
        tiled_repack_src1(&a, 0, 1, true);
        tiled_run_microtile<16, false, 4, true>(w, a, 0, 0, 1, 0, result, 16);
    }
    *out = result[0];
}
