#!/usr/bin/env python3
"""Fresh SmolLM2 layers10/11 for A5; never reuse A3/A4 TEST tensors."""
import json
from pathlib import Path
import sys

import smollm2_adaptive_corpus as source

source.SEED = "bpb3-a5-group16-mixed-independent-layers10-11-v1"
source.TENSORS = {
    "attention_output_layer10": "model.layers.10.self_attn.o_proj.weight",
    "attention_q_layer10": "model.layers.10.self_attn.q_proj.weight",
    "mlp_up_layer11": "model.layers.11.mlp.up_proj.weight",
    "mlp_down_layer11": "model.layers.11.mlp.down_proj.weight",
}


def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit("usage: bpb3_a5_corpus.py verified-safetensors output-directory")
    checkpoint, destination = Path(sys.argv[1]), Path(sys.argv[2])
    source.extract(checkpoint, destination)
    manifest_file = destination / "manifest.json"
    manifest = json.loads(manifest_file.read_text())
    assert manifest["split_seed"] == source.SEED
    assert set(manifest["tensors"]) == set(source.TENSORS)
    manifest["study"] = "bpb3_a5_group16_mixed_preregistered"
    manifest["prior_A3_A4_overlap"] = (
        "new layers10/11 in SAME model; old layers2/3 and6/7 excluded"
    )
    manifest_file.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    print("PASS_BPB3_A5_FRESH_STRATA tensors=4 dev=24 test=24")


if __name__ == "__main__":
    main()
