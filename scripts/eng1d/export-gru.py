#!/usr/bin/env python3
"""Offline, restricted-pickle tensor export; no torch import or GPU execution.

Only the explicitly frozen Dell checkpoint is admitted. No generic pickle
global, execution function, arbitrary input checkpoint or external storage.
The Rust predictor consumes ONLY the bounded flat tensor format.
"""
import collections
import hashlib
import io
import math
import pickle
import struct
import sys
import zipfile
from pathlib import Path

EXPECTED = "b570f6a81778843e53e494a46396cba07ad80b2f183b2102691d3bf7cee56efe"

def rebuild(storage, offset, shape, stride, requires_grad, hooks):
    if requires_grad or hooks:
        raise ValueError("unexpected tensor hooks/gradient")
    return storage, offset, shape, stride

class Reader(pickle.Unpickler):
    def find_class(self, module, name):
        allowed = {("collections", "OrderedDict"): collections.OrderedDict,
                   ("torch._utils", "_rebuild_tensor_v2"): rebuild,
                   ("torch", "FloatStorage"): "float32"}
        if (module, name) not in allowed:
            raise ValueError("unapproved pickle global")
        return allowed[module, name]

    def persistent_load(self, ident):
        kind, dtype, key, device, count = ident
        if kind != "storage" or dtype != "float32" or device != "cuda:0":
            raise ValueError("unexpected storage")
        if not str(key).isdigit() or not 0 < count <= 2_000_000:
            raise ValueError("invalid storage size")
        return str(key), count

def run():
    raw = Path(sys.argv[1]).read_bytes()
    if hashlib.sha256(raw).hexdigest() != EXPECTED:
        raise ValueError("checkpoint SHA256 mismatch")
    tensors = [("e.weight", (256, 64)), ("g.weight_ih_l0", (1536, 64)),
               ("g.weight_hh_l0", (1536, 512)), ("g.bias_ih_l0", (1536,)),
               ("g.bias_hh_l0", (1536,)), ("o.weight", (256, 512)), ("o.bias", (256,))]
    out = bytearray(b"SMLGRU01" + struct.pack("<II", 512, 64) + bytes.fromhex(EXPECTED))
    with zipfile.ZipFile(io.BytesIO(raw)) as archive:
        prefix = "gru512_run/"
        if archive.read(prefix + "byteorder") != b"little":
            raise ValueError("unsupported byte order")
        values = Reader(io.BytesIO(archive.read(prefix + "data.pkl"))).load()
        if set(values) != {key for key, shape in tensors}:
            raise ValueError("unexpected tensors")
        for key, expected_shape in tensors:
            (storage_key, storage_count), offset, shape, stride = values[key]
            if tuple(shape) != expected_shape or tuple(stride) != ((shape[1], 1) if len(shape) == 2 else (1,)):
                raise ValueError("invalid tensor shape/stride")
            count = math.prod(shape)
            storage = archive.read(prefix + "data/" + storage_key)
            if len(storage) != storage_count * 4 or offset < 0 or offset + count > storage_count:
                raise ValueError("invalid tensor storage")
            payload = storage[offset * 4:(offset + count) * 4]
            if any(not math.isfinite(v[0]) or abs(v[0]) > 64 for v in struct.iter_unpack("<f", payload)):
                raise ValueError("nonfinite/unbounded weight")
            out.extend(payload)
    path = Path(sys.argv[2])
    with path.open("xb") as target:
        target.write(out)
    print("GRU_EXPORT", "checkpoint=" + EXPECTED, "native=" + hashlib.sha256(out).hexdigest(), "bytes=" + str(len(out)))

if __name__ == "__main__":
    run()
