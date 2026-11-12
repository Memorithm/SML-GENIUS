#!/usr/bin/env python3
"""Predeclared BPB-3-A6 SmolLM2 original trained layers18/19.

Never reuse earlier A3 (2/3), A4 (6/7) or A5 (10/11) TEST tensors.
"""
import json
from pathlib import Path
import sys

import smollm2_adaptive_corpus as source

source.SEED = "bpb3-a6-delta-local16-layers18-19-v1"
source.TENSORS = {
    "attention_output_layer18": "model.layers.18.self_attn.o_proj.weight",
    "attention_q_layer18": "model.layers.18.self_attn.q_proj.weight",
    "mlp_up_layer19": "model.layers.19.mlp.up_proj.weight",
    "mlp_down_layer19": "model.layers.19.mlp.down_proj.weight",
}


def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit("usage: bpb3_a6_corpus.py verified-safetensors output-directory")
    checkpoint, destination = Path(sys.argv[1]), Path(sys.argv[2])
    source.extract(checkpoint, destination)
    manifest_file = destination / "manifest.json"
    manifest = json.loads(manifest_file.read_text())
    assert manifest["split_seed"] == source.SEED
    assert set(manifest["tensors"]) == set(source.TENSORS)
    manifest["study"] = "bpb3_a6_compressed_exponents_preregistered"
    manifest["previous_test_exclusion"] = (
        "no A3 layer2/3, A4 layer6/7 or A5 layer10/11; new layers18/19"
    )
    manifest_file.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    print("PASS_BPB3_A6_FRESH_STRATA tensors=4 dev=24 test=24")


if __name__ == "__main__":
    main()
