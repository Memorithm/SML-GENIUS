#!/usr/bin/env python3
"""Execute the published fixed panel and retain every arm; no model selection."""
import csv
import hashlib
import itertools
import json
import random
import subprocess
import sys
import time
from pathlib import Path


def run():
    binary, weights, panel_dir, output_dir, published_commit = sys.argv[1:]
    if len(published_commit) != 40 or any(c not in "0123456789abcdef" for c in published_commit):
        raise ValueError("immutable published commit required")
    actual = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    if actual != published_commit or subprocess.check_output(["git", "status", "--porcelain"]):
        raise ValueError("source must be clean at the declared commit")
    panel, output = Path(panel_dir), Path(output_dir)
    if hashlib.sha256(Path(weights).read_bytes()).hexdigest() != "6ad7d53991d0b99dfce47522e3e33dd1a0ddbed83a2f7d67e8a6e7b9e865357c":
        raise ValueError("frozen native weights mismatch")
    expected_bytes = Path("docs/BMX-GRU-INDEPENDENT-MANIFEST.json").read_bytes()
    if (panel / "manifest.json").read_bytes() != expected_bytes:
        raise ValueError("prepared manifest differs from published manifest")
    manifest = json.loads(expected_bytes)
    if len(manifest) != 12 or [row["domain"] for row in manifest] != ["FR"] * 4 + ["EN"] * 4 + ["RS"] * 4:
        raise ValueError("panel size/domain mismatch")
    # Verify every phase before starting any quality measurement.
    for row in manifest:
        for phase in ["prime", "dev", "test"]:
            data = (panel / f"{row['key']}.{phase}").read_bytes()
            if len(data) != row[phase]["end"] - row[phase]["start"] or hashlib.sha256(data).hexdigest() != row[phase]["sha256"]:
                raise ValueError("phase integrity mismatch")
    output.mkdir(exist_ok=False)
    deadline = time.monotonic() + 345 * 60
    (output / "provenance.json").write_text(json.dumps({"source_commit": actual,
        "binary_sha256": hashlib.sha256(Path(binary).read_bytes()).hexdigest(),
        "manifest_sha256": hashlib.sha256(expected_bytes).hexdigest()}, indent=2) + "\n")
    results = {}
    for phase in ["dev", "test"]:
        for row in manifest:
            key = row["key"]
            report = output / f"{key}.{phase}.tsv"
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError("345-minute campaign limit")
            subprocess.run([binary, weights, str(panel / f"{key}.prime"),
                str(panel / f"{key}.{phase}"), str(report)], check=True, timeout=remaining)
            lines = report.read_text().splitlines()
            if lines[-1] != "# COMPLETE":
                raise ValueError("incomplete report")
            arms = list(csv.DictReader(lines[1:-1], delimiter="\t"))
            if [arm["arm"] for arm in arms] != ["v5", "v5-gru", "v5-placebo"] or any(arm["roundtrip"] != "PASS" for arm in arms):
                raise ValueError("arm/replay failure")
            size = row[phase]["end"] - row[phase]["start"]
            results[(phase, key)] = (float(arms[0]["model_bits"]) - float(arms[1]["model_bits"])) / size
            print(f"GRU_PANEL_CASE_COMPLETE phase={phase} unit={key}", flush=True)
    gains = [results[("test", row["key"])] for row in manifest]
    mean = sum(gains) / 12
    rng = random.Random(20261014)
    boot = sorted(sum(rng.choice(gains[start:start + 4]) for start in [0, 4, 8] for _ in range(4)) / 12 for _ in range(9999))
    centered = [gain - 0.01 for gain in gains]
    observed = sum(centered)
    p = sum(sum(sign * gain for sign, gain in zip(signs, centered)) >= observed
        for signs in itertools.product([-1, 1], repeat=12)) / 4096
    domains = {domain: {"mean_gain": sum(gains[start:start + 4]) / 4,
        "strict_wins": sum(gain > 0 for gain in gains[start:start + 4])}
        for domain, start in [("FR", 0), ("EN", 4), ("RS", 8)]}
    lower, upper = boot[int(0.025 * 9998)], boot[int(0.975 * 9998)]
    accepted = lower > 0.01 and p <= 0.05 and all(d["mean_gain"] >= 0 and d["strict_wins"] >= 3 for d in domains.values())
    summary = {"complete": True, "units": 12, "mean_gain_bits_per_byte": mean,
        "bootstrap_95": [lower, upper], "sign_flip_p": p, "domains": domains,
        "conditional_panel_superiority_established": accepted,
        "weight_amortization_raw_bytes": 8 * Path(weights).stat().st_size / mean if mean > 0 else None,
        "test_unit_gains": dict(zip([row["key"] for row in manifest], gains)),
        "dev_unit_gains": {row["key"]: results[("dev", row["key"])] for row in manifest}}
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    run()
