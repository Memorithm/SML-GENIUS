# SML Boolean corpus extension — 14 immutable Memorithm sources

**Preregistered 2026-10-16 before the next fit.** Status: research-only. Model rights/admission are unchanged: **G0_admitted=false**, no TEST, no GRU-v2, no 25B.

## Experimental rationale

The first successful native Rust Boolean bank trained on four Memorithm source repositories:
[RemoteOps run 38905553555](https://github.com/Memorithm/RemoteOps/actions/runs/38905553555): 11 784 002 actual TRAIN bytes and 5.389160617 bits/byte on the separate frozen DEV files.
[RemoteOps run 38907392072](https://github.com/Memorithm/RemoteOps/actions/runs/38907392072) measured 8 081 133 more *candidate gross* bytes across ten pinned repositories, **before cross-repository SHA-256 deduplication**. This is not hundreds of MiB.

The extension is **no more than 24 roots** and must retain the same model architecture, hash selectors, byte features, Laplace alpha 1.0, file filters and benchmark DEV source; only TRAIN families are expanded. No optimization on DEV is allowed.

## Exact TRAIN identities

| Repository | Commit SHA |
| --- | --- |
| Memorithm/SciRust | `68d544679df465d3b85539c80926ed2644fbbba1` |
| Memorithm/TDI | `7fc1a25bb4b86b6880e521ef270bce6b0b77ebff` |
| Memorithm/FLAT-ATTENTION | `4d95909dec22f8d16c8b5312d502325d584f93df` |
| Memorithm/CCOS-Enterprise | `ac3e64b57aec8120ab98922c76afae4df48d6336` |
| Memorithm/ElasticXxx | `958ce6301730ceeec4bb1252ff12222e5bb7d431` |
| Memorithm/SLHAv2 | `590a7135f23980ce1aac25d52abf34757b2cabea` |
| Memorithm/Replikans | `55933d377c5671430d1bf8043450feb5e9fcc5fe` |
| Memorithm/ProofLab | `b49e2e51397a33370c3c35fa16897eebf2929559` |
| Memorithm/KVLab | `b6793546d27998f5122a449b3841f147b4214dba` |
| Memorithm/forge | `8df26b891d87eeabb3e7102ce3c43de7546fd0f4` |
| Memorithm/scirust-hub | `a1ce544d5a6027a224627b738268214775a56881` |
| Memorithm/FieldLab | `a1b8010df4822948fee8df6cef993e8b01c68959` |
| Memorithm/NeuralOperator | `2eba990da4779aeecfea1c4c288d8a6e552c48d8` |
| Memorithm/NoiseLab | `7f9221ca2cb093a9be8dcc378969389ac4ca8979` |

## DEV and target

- DEV fixed to **Memorithm/SML-GENIUS** revision `566faf5feffb8dcabb0f0955ef390cf73a29a939`, identical to the first successful experiment's DEV version. The **trainer's code commit is separate from the frozen DEV checkout**. No held-out TEST files are accessed.
- New multi-source floor: **16 MiB of actual deduplicated UTF-8 TRAIN bytes**, not gross repo size. No successful training claim if the floor is not met. Max 256 MiB TRAIN and 256 MiB DEV, 2 MiB/file, 50 000 files/role.
- Training: 8192 fixed Boolean conditional-probability counters `[u64;2]`, same per-file reset and same feature extraction as first experiment. Bitwise events and frozen DEV bits/byte measured identically.
- SHA-256 deduplication across TRAIN, and **exact-file-hash barrier** between TRAIN and DEV: DEV identical files are excluded and counted. The metric cannot be called fully independent from adjacent code or shared text; only exact byte duplicates are excluded.
- Keep both training and DEV file path/byte/hash manifests, counts, omitted duplicate counts, stable binary SHA-256 and all weight-table counters, with no raw text in uploaded artifacts.
- Baseline frozen metric of first fit: 5.389160617 bpc. This is a descriptive comparison of two TRAIN sets evaluated on the same SML DEV revision; not a final test of generalization or any statistical significance.

## Admission boundary and safety

The owner authorizes exploratory use of public Memorithm project material; it does not prove chain-of-title for embedded third-party contributions. No external dataset, public-domain assertion or fictitious rights signature is introduced. The larger research trial remains separate from G0/GRU-v2 admission, and trained weights cannot be promoted to production without further authorization and independent evaluation.
