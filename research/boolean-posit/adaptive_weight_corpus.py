#!/usr/bin/env python3
"""New held-out Pythia-14M trained tensor strata for adaptive BPB-3 v1.

Never selects the previously evaluated layers 0/5 E1 weights. Independent
explicit matrix names and RNG seed frozen before this experiment's TEST.
Only original FP16/FP32 finite source weights are exported as transient f32le.
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
TENSORS = {
    "attention_output_layer2": "gpt_neox.layers.2.attention.dense.weight",
    "attention_qkv_layer2": "gpt_neox.layers.2.attention.query_key_value.weight",
    "mlp_expansion_layer3": "gpt_neox.layers.3.mlp.dense_h_to_4h.weight",
    "mlp_projection_layer3": "gpt_neox.layers.3.mlp.dense_4h_to_h.weight",
}
SEED = "bpb3-adaptive-v1-nov12-independent-layer2-3"
WIDTH = 256
DEV = 24
TEST = 24


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fd:
        for data in iter(lambda: fd.read(1 << 20), b""):
            h.update(data)
    return h.hexdigest()


def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit("usage: adaptive_weight_corpus.py pinned_model.safetensors out_dir")
    model, destination = Path(sys.argv[1]), Path(sys.argv[2])
    if sha256(model) != MODEL_SHA256:
        raise SystemExit("STOP_ADAPTIVE_PYTHIA_SHA_MISMATCH")
    destination.mkdir(parents=True, exist_ok=True)
    manifest = {
        "model": MODEL,
        "source_revision": REVISION,
        "source_sha256": MODEL_SHA256,
        "license": "apache-2.0",
        "split_seed": SEED,
        "source_kind": "TRAINED_ORIGINAL_FLOAT_WEIGHTS_NOT_INFERENCE_VALIDATION",
        "prior_E1_overlap": "different_matrix_names_and_layers_from_E1_0_and_5",
        "test_policy": "fresh_disjoint_blocks_no_tuning_after_TEST",
        "block_width": WIDTH,
        "dev_blocks": DEV,
        "test_blocks": TEST,
        "tensors": {},
    }
    with safe_open(str(model), framework="numpy", device="cpu") as source:
        names = set(source.keys())
        for label, name in TENSORS.items():
            if name not in names:
                raise SystemExit(f"STOP_REQUIRED_ADAPTIVE_TENSOR_MISSING {name}")
            matrix = source.get_tensor(name)
            if matrix.ndim != 2 or matrix.dtype not in (np.float16, np.float32):
                raise SystemExit(f"STOP_UNSUPPORTED_ADAPTIVE_DTYPE {name}: {matrix.shape}/{matrix.dtype}")
            flattened = np.asarray(matrix, dtype="<f4", order="C").reshape(-1)
            if flattened.size % WIDTH or not np.all(np.isfinite(flattened)):
                raise SystemExit(f"STOP_INVALID_ADAPTIVE_MATRIX {name}")
            blocks = flattened.reshape(-1, WIDTH)
            if blocks.shape[0] < DEV + TEST:
                raise SystemExit(f"STOP_TOO_FEW_ADAPTIVE_BLOCKS {name}")
            seed = int.from_bytes(hashlib.sha256((SEED + name).encode()).digest()[:8], "little")
            shuffled = list(range(len(blocks)))
            random.Random(seed).shuffle(shuffled)
            dev = shuffled[:DEV]
            test = shuffled[DEV:DEV + TEST]
            if set(dev) & set(test):
                raise SystemExit("STOP_ADAPTIVE_SPLIT_OVERLAP")
            data = np.concatenate((blocks[dev], blocks[test])).astype("<f4", copy=False)
            dest = destination / f"{label}.f32le"
            dest.write_bytes(data.tobytes(order="C"))
            manifest["tensors"][label] = {
                "source_name": name,
                "source_shape": list(matrix.shape),
                "dtype": str(matrix.dtype),
                "selected_dev_blocks": dev,
                "selected_test_blocks": test,
                "source_blocks": len(blocks),
                "reblocked_selected_f32le_sha256": sha256(dest),
                "data_size_bytes": dest.stat().st_size,
            }
    (destination / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    print(f"PASS_ADAPTIVE_PYTHIA_FRESH_STRATA tensors={len(TENSORS)} dev={DEV} test={TEST} sha256={MODEL_SHA256}")


if __name__ == "__main__":
    main()