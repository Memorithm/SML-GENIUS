# A3 complete-output reconstruction and observed evidence

The executed example is `crates/sml-store/examples/dkv_observed_history_a3.rs`.
The prospective protocol is `docs/SML-DKV-A3-BATCH10-PREREGISTRATION.md`.
`MANIFEST.txt` pins the initial successful exact code/protocol/output identities
and actual existing-CI run. `SUMMARY.csv` preserves the observed arm/family rows.

The complete 1728-row A3 CSV is printed in CI run 37080613129, job 111080075039.
It is not duplicated as a second large tracked CSV here: the complete A2 input
rows are already committed in `research/dkv-r1a2/development.csv`, and the frozen
A3 generator deterministically regenerates every A3 output byte from those rows
and independently generated/verified canonical source events. The retained
SHA-256 binds the full CSV, not just aggregate metrics. CI log retention may
expire; committed input, code and checksum do not depend on those logs.

Reproduce from the repository root (Rust 1.89 toolchain):

```bash
cargo +1.89.0 test --locked -p sml-store --test dkv_a3_retained_evidence
cargo +1.89.0 run --locked --quiet -p sml-store --example dkv_observed_history_a3 > /tmp/sml-dkv-a3.csv
printf '%s  %s\n' 2b9793b6ad72c8d473cb418264786bac6268b496c01baaf6f1e5d7e9def4e22f /tmp/sml-dkv-a3.csv | sha256sum --check
```

The regression tests were added AFTER observing the diagnostic. They freeze
reproduction; they do not turn known outcomes into independent confirmation.
A hash mismatch blocks software conformance and must be investigated, not fixed
by silently changing the retained hash or the original A/A2 observations.

## Interpretation

Each arm has 576 episodes, including 432 answerable and 144 absent/conflicting
observations. Full-history screening leaves latent correctness unchanged at
360 standard, 372 contrast, 504 direct. Screened outcomes partition into:

| arm | correct known | wrong known | correct unknown | wrong unknown |
| --- | ---: | ---: | ---: | ---: |
| standard | 360 | 72 | 144 | 0 |
| contrast | 368 | 52 | 144 | 12 |
| direct | 432 | 0 | 144 | 0 |

Complete observed histories are additional stored source values and additional
verification work. Each arm rereads 3672 segments / 40392 raw bytes over repeated
synthetic episodes. These are logical byte counts, not disk I/O, RSS or latency.
They must not be attributed to the compressed matrix or omitted from costs.

The screen never substitutes an observed value for a candidate. It removes
known outputs only on missing/conflicting observations. Agreement does not prove
the downstream answer correct: old/near-old latent errors remain, and an unknown
candidate remains unknown. Neither a screen nor a numeric cache grants action
permission, source authenticity or semantic entailment.

The separate two-observation retention regression preserves a known case where
correlated later writes damage an earlier Q12 value while exact history retains
it. Cache hits reproduce the same damaged numeric value with fewer matrix reads;
this is NOT a retention repair or an equal-resource comparison result.

## Unqualified work

Full context promotion remains BLOCKED. Learned representation quality, improved
near-key retention, large/representative tasks, matched total resource budgets,
physical measurements, crash-durable history and independent candidate comparison
remain open. No protected final evaluation, training or counterpart code is used.
The original A and A2 implementations and complete evidence are unchanged.
