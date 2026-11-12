#!/usr/bin/env python3
"""BPB-3-A8 original trained layers28/29 only; pinned prospective source.

This A8 study excludes previously revealed A3 layer2/3, A4 layer6/7,
A5 layer10/11, A6 layer18/19, and A7 layer24/25 TEST matrices.
"""
import json
from pathlib import Path
import sys

import smollm2_adaptive_corpus as source

source.SEED = "bpb3-a8-one-base-256-layers28-29-v1"
source.TENSORS = {
    "attention_output_layer28": "model.layers.28.self_attn.o_proj.weight",
    "attention_q_layer28": "model.layers.28.self_attn.q_proj.weight",
    "mlp_up_layer29": "model.layers.29.mlp.up_proj.weight",
    "mlp_down_layer29": "model.layers.29.mlp.down_proj.weight",
}


def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit("usage: bpb3_a8_corpus.py verified-safetensors output-directory")
    checkpoint, destination = Path(sys.argv[1]), Path(sys.argv[2])
    source.extract(checkpoint, destination)
    manifest_file = destination / "manifest.json"
    manifest = json.loads(manifest_file.read_text())
    assert manifest["split_seed"] == source.SEED
    assert set(manifest["tensors"]) == set(source.TENSORS)
    manifest["study"] = "bpb3_a8_one_common_base_superblock256"
    manifest["previous_test_exclusion"] = (
        "A3 layers2/3; A4 6/7; A5 10/11; A6 18/19; A7 24/25; new A8 28/29"
    )
    manifest_file.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    print("PASS_BPB3_A8_FRESH_STRATA tensors=4 dev=24 test=24")


if __name__ == "__main__":
    main()
