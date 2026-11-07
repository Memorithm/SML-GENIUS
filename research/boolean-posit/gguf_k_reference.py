#!/usr/bin/env python3
"""Independent parity against exactly pinned official gguf-py Q3_K and Q4_K decoders.

Usage: python3 gguf_k_reference.py path/to/pinned/llama.cpp fixture-vectors.csv
No trained model, GGUF container claims, GPU timing or quantizer fitting.
"""
import pathlib
import struct
import sys
import types

import numpy as np

OFFICIAL_SHA = "f7591a6a01f9273de03331732bdc0b39721168be"
CASES = {
    0: (0x3C00, 0x3800),
    1: (0xBC00, 0x4000),
    2: (0x0001, 0x3C00),
    3: (0x0000, 0x0000),
    4: (0x3E00, 0x3A00),
}


def official_modules(repo: pathlib.Path):
    # Deliberately bypass gguf/__init__.py so unrelated file converters and
    # optional model dependencies are not imported for this scalar-only oracle.
    library = repo / "gguf-py" / "gguf"
    if not (library / "quants.py").is_file():
        raise SystemExit("STOP_OFFICIAL_GGUF_REFERENCE_MISSING")
    namespace = types.ModuleType("gguf")
    namespace.__path__ = [str(library)]
    sys.modules["gguf"] = namespace
    from gguf.quants import Q3_K, Q4_K
    return {"Q3_K": Q3_K, "Q4_K": Q4_K}


def raw_fixture(fmt: str, case: int) -> bytes:
    length = {"Q3_K": 110, "Q4_K": 144}[fmt]
    data = bytearray((i * 73 + (i // 4) * 19 + case * 41) % 256 for i in range(length))
    d, dmin = CASES[case]
    if fmt == "Q3_K":
        data[108:110] = struct.pack("<H", d)
    else:
        data[0:2] = struct.pack("<H", d)
        data[2:4] = struct.pack("<H", dmin)
    return bytes(data)


def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit("usage: gguf_k_reference.py official_llama_repo rust_vectors.csv")
    official_root = pathlib.Path(sys.argv[1])
    imports = official_modules(official_root)
    observed: dict[tuple[str, int, int], int] = {}
    for line in pathlib.Path(sys.argv[2]).read_text().splitlines():
        fmt, raw_case, raw_index, bits = line.split(",")
        key = fmt, int(raw_case), int(raw_index)
        if key in observed:
            raise SystemExit(f"STOP_DUPLICATE_VECTOR {key}")
        observed[key] = int(bits, 16)

    tested = 0
    for fmt, implementation in imports.items():
        for case in CASES:
            raw = raw_fixture(fmt, case)
            official = implementation.dequantize_blocks(
                np.frombuffer(raw, dtype=np.uint8).reshape((1, -1))
            )
            values = np.asarray(official, dtype=np.float32).reshape(-1)
            if len(values) != 256:
                raise SystemExit(f"STOP_BAD_OFFICIAL_LENGTH {fmt}/{case}")
            for index, expected in enumerate(values.view(np.uint32)):
                key = fmt, case, index
                actual = observed.pop(key, None)
                if actual is None or actual != int(expected):
                    raise SystemExit(
                        f"STOP_GGUF_REFERENCE_MISMATCH {fmt} case={case} "
                        f"index={index} expected={int(expected):08x} "
                        f"actual={actual if actual is None else format(actual, '08x')}"
                    )
                tested += 1
    if observed:
        raise SystemExit(f"STOP_UNEXPECTED_VECTORS {len(observed)}")
    if tested != 2 * len(CASES) * 256:
        raise SystemExit(f"STOP_INCOMPLETE_VECTORS count={tested}")
    print(
        f"PASS_GGUF_K_OFFICIAL_PYTHON_PARITY reference_sha={OFFICIAL_SHA} "
        f"formats=Q3_K,Q4_K fixtures={2 * len(CASES)} "
        f"coefficients={tested} exact_f32_bits=true"
    )


if __name__ == "__main__":
    main()
