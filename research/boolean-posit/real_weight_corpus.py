#!/usr/bin/env python3
"""Read-only extraction of SHA-pinned, Apache-2.0 Pythia-14M trained weights.

The independent E1 Rust engine does the quantization, reconstruction and
metrics. This Python utility only decodes safetensors, produces disjoint
row-aligned 256-element f32le blocks and an inspectable provenance manifest.
"""
import hashlib
import json
from pathlib import Path
import random
import sys

import numpy as np
from safetensors import safe_open

MODEL = "EleutherAI/pythia-14m"
REVISION = "94f7c35d5e9f2e9bac8ca839329f505b4d007d5d"
MODEL_SHA256 = "116a02532db461f91386a5b20f942ff2c8d4de7341e21b55caafc3d7b25f49a1"
SELECTED = {
    "attention_output": "gpt_neox.layers.0.attention.dense.weight",
    "attention_qkv": "gpt_neox.layers.0.attention.query_key_value.weight",
    "mlp_expansion": "gpt_neox.layers.0.mlp.dense_h_to_4h.weight",
    "mlp_projection": "gpt_neox.layers.5.mlp.dense_4h_to_h.weight",
}
DEV_BLOCKS = 24
TEST_BLOCKS = 24
BLOCK_WIDTH = 256
SPLIT_SEED = "bpb3-e1-gguf-issue853-split-v1"


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit("usage: real_weight_corpus.py pinned_model.safetensors out_dir")
    file, destination = Path(sys.argv[1]), Path(sys.argv[2])
    if sha256(file) != MODEL_SHA256:
        raise SystemExit("STOP_MODEL_SHA256_CHANGED")
    destination.mkdir(parents=True, exist_ok=True)

    manifest = {
        "model": MODEL,
        "revision": REVISION,
        "license": "apache-2.0",
        "model_safetensors_sha256": MODEL_SHA256,
        "split_seed": SPLIT_SEED,
        "kind": "trained_model_original_f16_or_f32_weights",
        "dataset_visibility": "open_model_weights",
        "source_url": f"https://huggingface.co/{MODEL}/tree/{REVISION}",
        "test_independence": "disjoint_blocks_same_model_not_independent_model",
        "block_width": BLOCK_WIDTH,
        "dev_blocks_per_tensor": DEV_BLOCKS,
        "test_blocks_per_tensor": TEST_BLOCKS,
        "tensors": {},
    }
    with safe_open(str(file), framework="numpy", device="cpu") as weights:
        available = set(weights.keys())
        for label, name in SELECTED.items():
            if name not in available:
                raise SystemExit(f"STOP_REQUIRED_TENSOR_ABSENT name={name}")
            raw = weights.get_tensor(name)
            if raw.ndim != 2 or raw.dtype not in (np.float16, np.float32):
                raise SystemExit(f"STOP_BAD_WEIGHT_TENSOR name={name} shape={raw.shape} dtype={raw.dtype}")
            # Native contiguous row-major layout; hidden_size=128, so a
            # 256-value codec superblock spans two adjacent source rows.
            flat = np.asarray(raw, dtype="<f4", order="C").reshape(-1)
            if flat.size % BLOCK_WIDTH:
                raise SystemExit(f"STOP_NOT_EXACT_CODEC_BLOCKS name={name}")
            if not np.all(np.isfinite(flat)):
                raise SystemExit(f"STOP_NONFINITE_TRAINED_WEIGHT name={name}")
            blocks = flat.reshape((-1, BLOCK_WIDTH))
            if len(blocks) < DEV_BLOCKS + TEST_BLOCKS:
                raise SystemExit(f"STOP_TOO_FEW_WEIGHT_BLOCKS name={name}")
            seed = int.from_bytes(hashlib.sha256((SPLIT_SEED + name).encode()).digest()[:8], "little")
            order = list(range(len(blocks)))
            random.Random(seed).shuffle(order)
            dev = order[:DEV_BLOCKS]
            test = order[DEV_BLOCKS:DEV_BLOCKS + TEST_BLOCKS]
            if set(dev) & set(test):
                raise SystemExit("STOP_DEV_TEST_OVERLAP")
            selected = np.concatenate([blocks[dev], blocks[test]], axis=0).astype("<f4", copy=False)
            dest = destination / f"{label}.f32le"
            dest.write_bytes(selected.tobytes(order="C"))
            manifest["tensors"][label] = {
                "tensor_name": name,
                "shape": list(raw.shape),
                "original_dtype": str(raw.dtype),
                "f32le_sha256": sha256(dest),
                "selected_dev_blocks": dev,
                "selected_test_blocks": test,
                "total_tensor_blocks": len(blocks),
                "f32le_size_bytes": dest.stat().st_size,
                "read_as": "contiguous_f32le_reblock_of_float16_or_float32_original",
            }
    (destination / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    print(f"PASS_PYTHIA_REAL_WEIGHTS_ADMISSION tensors={len(SELECTED)} dev={DEV_BLOCKS} test={TEST_BLOCKS} model_sha256={MODEL_SHA256}")


if __name__ == "__main__":
    main()
