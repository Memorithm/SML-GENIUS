"""ENG-1d exploratory diagnostic (pure Python, no dependency).

Re-implements the TESTTEST split/lane contract on ONE local text that is not
in the TESTTEST panel, and reports bits/byte of the Witten-Bell depth chain
under several escape policies:

    p_d(y) = (n_d(y) + e * p_{d-1}(y)) / (N_d + e)
    wb : e = U_d            (frozen ENG-1c readout)
    swb: e = beta * U_d     (scaled Witten-Bell)
    dir: e = tau            (fixed concentration, A6-E5 parent form)

Splits: TRAIN [0, min(3N/5, 600000)), DEV [3N/5, +32768), TEST [4N/5, +65536),
64 contiguous lanes with context reset. Only contexts needed by DEV/TEST are
counted (exact for these queries). Exploration only: selections must be made
on DEV; this is not a confirmation and must not use TESTTEST works.

Usage: python3 -I escape_diagnostic.py TEXT_FILE [--train-cap BYTES]
"""
import math
import sys
from collections import Counter, defaultdict

LANES = 64
MAXD = 32
DEPTHS = [4, 5, 6, 7, 12, 32]
POLICIES = [("wb", None)] + [("swb", b) for b in (2, 4, 8, 12, 16, 24)] + [("dir", t) for t in (8, 16, 32, 64)]


def lanes(b):
    q = len(b) // LANES
    return [b[i * q:(i + 1) * q] for i in range(LANES)]


def positions(part):
    for lane in lanes(part):
        for t in range(1, len(lane)):
            yield lane, t


def escape(kind, param, distinct):
    if kind == "wb":
        return distinct
    if kind == "swb":
        return param * distinct
    return param


def main():
    path = sys.argv[1]
    cap = int(sys.argv[sys.argv.index("--train-cap") + 1]) if "--train-cap" in sys.argv else None
    body = open(path, "rb").read().replace(b"\r", b"")
    n = len(body)
    a, b = 3 * n // 5, 4 * n // 5
    train = body[:min(a, 600000)]
    if cap:
        train = train[:cap]
    dev, test = body[a:min(b, a + 32768)], body[b:min(n, b + 65536)]
    need = [set() for _ in range(MAXD + 1)]
    for part in (dev, test):
        for lane, t in positions(part):
            for d in range(min(MAXD, t) + 1):
                need[d].add(lane[t - d:t])
    counts = [defaultdict(Counter) for _ in range(MAXD + 1)]
    for lane, t in positions(train):
        for d in range(min(MAXD, t) + 1):
            c = lane[t - d:t]
            if c not in need[d]:
                break  # no longer suffix of an unneeded context is needed
            counts[d][c][lane[t]] += 1
    print(f"file={path} train={len(train)} dev={len(dev)} test={len(test)}")
    for name, part in (("DEV", dev), ("TEST", test)):
        bits = defaultdict(float)
        tokens = 0
        for lane, t in positions(part):
            y = lane[t]
            p = [1 / 256] * len(POLICIES)
            snap = {}
            top = min(MAXD, t)
            for d in range(top + 1):
                h = counts[d].get(lane[t - d:t])
                if h:
                    tot = sum(h.values())
                    ny = h.get(y, 0)
                    for i, (kind, param) in enumerate(POLICIES):
                        e = escape(kind, param, len(h))
                        p[i] = (ny + e * p[i]) / (tot + e)
                if d in DEPTHS:
                    snap[d] = list(p)
            for d in DEPTHS:
                s = snap.get(d, p)
                for i in range(len(POLICIES)):
                    bits[(i, d)] -= math.log2(s[i])
            tokens += 1
        print(name, "tokens", tokens)
        for i, (kind, param) in enumerate(POLICIES):
            label = kind if param is None else f"{kind}:{param}"
            print(f"  {label:8s} " + "  ".join(f"d{d}={bits[(i, d)] / tokens:.4f}" for d in DEPTHS))


if __name__ == "__main__":
    main()
