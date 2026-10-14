#!/usr/bin/env python3
"""Prepare fixed public panel without running any model or inspecting quality.

Outputs are experiment data, never edits to source. Only fresh directories are
accepted. Downloads have a size bound and fail closed without substitutions.
"""
import hashlib
import json
import re
import sys
import urllib.request
from pathlib import Path

BOOKS = [
    ("fr-sand", "FR", "George Sand", 23582),
    ("fr-daudet", "FR", "Alphonse Daudet", 5105),
    ("fr-huysmans", "FR", "J.-K. Huysmans", 56149),
    ("fr-loti", "FR", "Pierre Loti", 4785),
    ("en-dolbear", "EN", "Amos Emerson Dolbear", 29444),
    ("en-tyndall", "EN", "John Tyndall", 24527),
    ("en-lodge", "EN", "Oliver Lodge", 28613),
    ("en-maxwell", "EN", "James Clerk Maxwell", 4908),
]
RUST = [
    ("rs-tokio", "tokio-rs/tokio", "4890d027f8a566ccf081e7f019ed766f780df1af", "tokio/src/sync/mutex.rs"),
    ("rs-rayon", "rayon-rs/rayon", "ee0a00bdb1ab039e178a215ad5712fb7fa58e58f", "src/iter/mod.rs"),
    ("rs-serde", "serde-rs/serde", "6693a89cca77e0151437da1c7f890090b9ebf04c", "serde_core/src/de/mod.rs"),
    ("rs-regex", "rust-lang/regex", "72d650cb0a880a01ab6dc2137c0888e8f89740f7", "regex-syntax/src/ast/parse.rs"),
]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def fetch(url):
    request = urllib.request.Request(url, headers={"User-Agent": "Memorithm-BMX-GRU-research/1"})
    with urllib.request.urlopen(request, timeout=60) as response:
        raw = response.read(8_000_001)
    if len(raw) > 8_000_000:
        raise ValueError("source exceeds size bound")
    return raw


def prepare(directory):
    root = Path(directory)
    root.mkdir(exist_ok=False)
    manifest = []
    sources = [(key, domain, author, f"https://www.gutenberg.org/cache/epub/{book}/pg{book}.txt", True)
               for key, domain, author, book in BOOKS]
    sources += [(key, "RS", repo, f"https://raw.githubusercontent.com/{repo}/{commit}/{path}", False)
                for key, repo, commit, path in RUST]
    for key, domain, unit, url, gutenberg in sources:
        raw = fetch(url)
        body = raw
        if gutenberg:
            # Preserve exact source bytes (including CRLF). Remove only wrapper
            # lines through START and from END, not editorial/front matter.
            start = re.search(br"(?m)^\*\*\* START OF (?:THE|THIS) PROJECT GUTENBERG.*?\*\*\*[^\n]*\n", raw)
            end = re.search(br"(?m)^\*\*\* END OF (?:THE|THIS) PROJECT GUTENBERG", raw)
            if not start or not end or start.end() >= end.start():
                raise ValueError(f"missing Gutenberg markers: {key}")
            body = raw[start.end():end.start()]
        if len(body) < 40960:
            raise ValueError(f"source too short: {key}")
        row = {"key": key, "domain": domain, "independent_unit": unit,
               "url": url, "raw_bytes": len(raw), "raw_sha256": digest(raw),
               "body_bytes": len(body), "body_sha256": digest(body)}
        for phase, low, high in [("prime", 0, 16384), ("dev", 16384, 24576), ("test", 24576, 40960)]:
            data = body[low:high]
            (root / f"{key}.{phase}").write_bytes(data)
            row[phase] = {"start": low, "end": high, "sha256": digest(data)}
        manifest.append(row)
        print(f"GRU_PANEL_PREPARED key={key} raw_bytes={len(raw)} body_bytes={len(body)}", flush=True)
    (root / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print("GRU_PANEL_COMPLETE units=12 no_model_executed=true")


if __name__ == "__main__":
    prepare(sys.argv[1])
