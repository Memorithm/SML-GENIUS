#!/usr/bin/env python3
"""Predeclared BPB-3-A7 SmolLM2 original trained layers24/25.

Exclude earlier A3 (2/3), A4 (6/7), A5 (10/11), A6 (18/19) TEST tensors.
"""
import json
from pathlib import Path
import sys

import smollm2_adaptive_corpus as source

source.SEED = "bpb3-a7-absolute6-unseen-layers24-25-v1"
source.TENSORS = {
    "attention_output_layer24": "model.layers.24.self_attn.o_proj.weight",
    "attention_q_layer24": "model.layers.24.self_attn.q_proj.weight",
    "mlp_up_layer25": "model.layers.25.mlp.up_proj.weight",
    "mlp_down_layer25": "model.layers.25.mlp.down_proj.weight",
}


def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit("usage: bpb3_a7_corpus.py verified-safetensors output-directory")
    checkpoint, destination = Path(sys.argv[1]), Path(sys.argv[2])
    source.extract(checkpoint, destination)
    manifest_file = destination / "manifest.json"
    manifest = json.loads(manifest_file.read_text())
    assert manifest["split_seed"] == source.SEED
    assert set(manifest["tensors"]) == set(source.TENSORS)
    manifest["study"] = "bpb3_a7_absolute6_exponents_preregistered"
    manifest["previous_test_exclusion"] = (
        "new layers24/25, excluding A3 2/3, A4 6/7, A5 10/11, and A6 18/19"
    )
    manifest_file.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    print("PASS_BPB3_A7_FRESH_STRATA tensors=4 dev=24 test=24")


if __name__ == "__main__":
    main()
