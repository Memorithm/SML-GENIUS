#!/usr/bin/env python3
"""Immutable SmolLM2-135M trained-source selection for BPB-3 A3.

Read the safetensors header directly, avoiding unsafe deserialization and
framework-specific BF16 limitations. This is a research-only extractor;
never upload original model or sample weight bytes to CI artifacts.
"""
import hashlib
import json
import mmap
from pathlib import Path
import random
import struct
import sys

import numpy as np

MODEL = "HuggingFaceTB/SmolLM2-135M"
REVISION = "93efa2f097d58c2a74874c7e644dbc9b0cee75a2"
FILE_SHA256 = "80521b40281d6ce74e35c9282c22539e75aa0ac8578892b2a59955ef78d55da1"
SEED = "bpb3-a3-smollm2-135m-fresh-v1-20261112"
TENSORS = {
    "attention_output_layer2": "model.layers.2.self_attn.o_proj.weight",
    "attention_q_layer2": "model.layers.2.self_attn.q_proj.weight",
    "mlp_up_layer3": "model.layers.3.mlp.up_proj.weight",
    "mlp_down_layer3": "model.layers.3.mlp.down_proj.weight",
}
WIDTH = 256
DEV = 24
TEST = 24
BYTES_PER_ELEMENT = {"F16": 2, "BF16": 2, "F32": 4}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def extract(model: Path, destination: Path) -> None:
    if sha256(model) != FILE_SHA256:
        raise SystemExit("STOP_SMOLLM2_CHECKPOINT_SHA256_MISMATCH")
    destination.mkdir(parents=True, exist_ok=True)
    manifest = {
        "model": MODEL,
        "source_revision": REVISION,
        "source_sha256": FILE_SHA256,
        "license": "apache-2.0",
        "format": "safetensors_only_no_pickle",
        "split_seed": SEED,
        "blocks_per_tensor": {"DEV": DEV, "TEST": TEST},
        "width": WIDTH,
        "source_kind": "TRAINED_ORIGINAL_WEIGHTS_NOT_MODEL_INFERENCE",
        "prior_A2_overlap": "different_model_and_architecture",
        "tensors": {},
    }
    with model.open("rb") as source:
        with mmap.mmap(source.fileno(), length=0, access=mmap.ACCESS_READ) as memory:
            if len(memory) < 8:
                raise SystemExit("STOP_INVALID_SAFETENSORS_HEADER")
            header_size = struct.unpack_from("<Q", memory, 0)[0]
            if not (1 <= header_size <= min(16_777_216, len(memory) - 8)):
                raise SystemExit("STOP_INVALID_SAFETENSORS_HEADER_LENGTH")
            header = json.loads(memory[8:8 + header_size].decode("utf-8"))
            data_begin = 8 + header_size
            for label, tensor_name in TENSORS.items():
                if tensor_name not in header:
                    raise SystemExit(f"STOP_REQUIRED_SMOLLM2_TENSOR_ABSENT {tensor_name}")
                descriptor = header[tensor_name]
                dtype = descriptor["dtype"]
                shape = descriptor["shape"]
                offsets = descriptor["data_offsets"]
                if dtype not in BYTES_PER_ELEMENT or len(shape) != 2:
                    raise SystemExit(f"STOP_BAD_SMOLLM2_TENSOR_TYPE {tensor_name} {dtype} {shape}")
                count = int(shape[0]) * int(shape[1])
                if count <= 0 or count % WIDTH:
                    raise SystemExit(f"STOP_BAD_SMOLLM2_TENSOR_SHAPE {tensor_name} {shape}")
                begin, end = map(int, offsets)
                expected_size = count * BYTES_PER_ELEMENT[dtype]
                if begin < 0 or end - begin != expected_size or data_begin + end > len(memory):
                    raise SystemExit(f"STOP_BAD_SMOLLM2_TENSOR_OFFSET {tensor_name}")
                if dtype == "F32":
                    values = np.frombuffer(
                        memory, dtype="<f4", count=count, offset=data_begin + begin
                    ).astype("<f4", copy=True)
                elif dtype == "F16":
                    values = np.frombuffer(
                        memory, dtype="<f2", count=count, offset=data_begin + begin
                    ).astype("<f4", copy=True)
                else:
                    raw = np.frombuffer(
                        memory, dtype="<u2", count=count, offset=data_begin + begin
                    )
                    values = (raw.astype("<u4") << 16).view("<f4")
                if not np.all(np.isfinite(values)):
                    raise SystemExit(f"STOP_NONFINITE_SMOLLM2_TENSOR {tensor_name}")
                blocks = values.reshape((-1, WIDTH))
                if len(blocks) < DEV + TEST:
                    raise SystemExit(f"STOP_TOO_FEW_SMOLLM2_BLOCKS {tensor_name}")
                seed = int.from_bytes(
                    hashlib.sha256((SEED + tensor_name).encode()).digest()[:8], "little"
                )
                shuffled = list(range(len(blocks)))
                random.Random(seed).shuffle(shuffled)
                dev, test = shuffled[:DEV], shuffled[DEV:DEV + TEST]
                if set(dev) & set(test):
                    raise SystemExit("STOP_SMOLLM2_TRAIN_TEST_OVERLAP")
                selected = np.ascontiguousarray(
                    np.concatenate((blocks[dev], blocks[test])), dtype="<f4"
                )
                output = destination / f"{label}.f32le"
                output.write_bytes(selected.tobytes(order="C"))
                if output.stat().st_size != (DEV + TEST) * WIDTH * 4:
                    raise SystemExit("STOP_SMOLLM2_SELECTED_BYTES")
                manifest["tensors"][label] = {
                    "source_name": tensor_name,
                    "shape": shape,
                    "source_dtype": dtype,
                    "total_source_blocks": len(blocks),
                    "dev_block_indices": dev,
                    "test_block_indices": test,
                    "selected_f32le_sha256": sha256(output),
                    "selected_f32le_bytes": output.stat().st_size,
                }
    (destination / "manifest.json").write_text(
        json.dumps(manifest, sort_keys=True, indent=2) + "\n"
    )
    print(
        f"PASS_A3_SMOLLM2_PINNED tensors={len(TENSORS)} dev={DEV} test={TEST}"
        f" checkpoint_sha256={FILE_SHA256}"
    )


def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit(
            "usage: smollm2_adaptive_corpus.py verified-model.safetensors output_dir"
        )
    extract(Path(sys.argv[1]), Path(sys.argv[2]))


if __name__ == "__main__":
    main()
