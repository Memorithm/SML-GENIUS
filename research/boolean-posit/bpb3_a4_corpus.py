#!/usr/bin/env python3
"""A4 fresh tensor strata, frozen before evaluation (same verified model as A3).

Only layer-6/7 original trained tensors, never the A3 layer-2/3 TEST
tensors. Reuse the already-qualified safetensors/BF16 reader; do not
copy source model or selected weight bytes to GitHub artifacts.
"""
import json
from pathlib import Path
import sys

import smollm2_adaptive_corpus as reader

reader.SEED = "bpb3-a4-local16-independent-layers6-7-v1"
reader.TENSORS = {
    "attention_output_layer6": "model.layers.6.self_attn.o_proj.weight",
    "attention_q_layer6": "model.layers.6.self_attn.q_proj.weight",
    "mlp_up_layer7": "model.layers.7.mlp.up_proj.weight",
    "mlp_down_layer7": "model.layers.7.mlp.down_proj.weight",
}


def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit("usage: bpb3_a4_corpus.py verified_safetensors outdir")
    model, directory = Path(sys.argv[1]), Path(sys.argv[2])
    reader.extract(model, directory)
    manifest_file = directory / "manifest.json"
    manifest = json.loads(manifest_file.read_text())
    assert set(manifest["tensors"]) == set(reader.TENSORS)
    assert manifest["split_seed"] == reader.SEED
    manifest["prior_A3_overlap"] = (
        "no layer2/3 matrices reused; new layer6/7 in SAME trained model"
    )
    manifest["study"] = "bpb3_a4_local16_preregistered"
    manifest_file.write_text(json.dumps(manifest, sort_keys=True, indent=2) + "\n")
    print("PASS_BPB3_A4_FRESH_STRATA tensors=4 dev=24 test=24")


if __name__ == "__main__":
    main()
