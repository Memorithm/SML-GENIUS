#!/usr/bin/env python3
"""Independent NumPy recurrence check, synthetic bytes only; no corpus scoring.

The reference uses BLAS matvec, not Rust's four-lane reduction. This checks
the native f64 kernel, not bit-exact agreement with the training CUDA kernel.
"""
import hashlib
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np


def run():
    binary, weights_path = sys.argv[1:]
    raw = Path(weights_path).read_bytes()
    if hashlib.sha256(raw).hexdigest() != "6ad7d53991d0b99dfce47522e3e33dd1a0ddbed83a2f7d67e8a6e7b9e865357c":
        raise ValueError("native weights mismatch")
    hidden, embedding = struct.unpack_from("<II", raw, 8)
    shapes = [(256, embedding), (3 * hidden, embedding), (3 * hidden, hidden),
              (3 * hidden,), (3 * hidden,), (256, hidden), (256,)]
    scalars = np.frombuffer(raw, dtype="<f4", offset=48).astype(np.float64)
    values, offset = [], 0
    for shape in shapes:
        count = int(np.prod(shape))
        values.append(scalars[offset:offset + count].reshape(shape))
        offset += count
    emb, wi, wh, bi, bh, wo, bo = values
    state = np.zeros(hidden)
    sequence = b"Synthetic GRU qualification\n" + bytes(range(256))
    expected = []
    for byte in b"\n" + sequence:
        ir, iz, inn = np.split(wi @ emb[byte] + bi, 3)
        hr, hz, hn = np.split(wh @ state + bh, 3)
        reset = 1 / (1 + np.exp(-(ir + hr)))
        update = 1 / (1 + np.exp(-(iz + hz)))
        state = (1 - update) * np.tanh(inn + reset * hn) + update * state
        logits = wo @ state + bo
        probability = np.maximum(np.exp(logits - np.max(logits)), 1e-12)
        expected.append(probability / probability.sum())
    with tempfile.TemporaryDirectory(prefix="bmx-gru-oracle-") as directory:
        path = Path(directory) / "synthetic.bin"
        path.write_bytes(sequence)
        output = subprocess.check_output([binary, "oracle", weights_path, str(path)], text=True)
    actual = np.array([[float(p) for p in line.split()] for line in output.splitlines()])
    expected = np.array(expected)
    if actual.shape != expected.shape:
        raise ValueError("oracle shape mismatch")
    error = float(np.max(np.abs(actual - expected)))
    if error > 1e-11:
        raise ValueError(f"oracle disagreement: {error}")
    print(f"GRU_ORACLE_PASS steps={len(expected)} max_absolute_error={error:.17g}")


if __name__ == "__main__":
    run()
