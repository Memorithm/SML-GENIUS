# SBG-REA P2b: measured overhead and cache-only follow-up

## Physical evidence that motivates this change

The first approved paired Dell T430 run was completed successfully:

- RemoteOps run: https://github.com/Memorithm/RemoteOps/actions/runs/44253770733
- Job: `134630447304`
- Native model source: `1f7e0c4537bbbbe598d89aea0311e25b0d2165a5`
- RemoteOps controller: `4b1ce4dfbdb4cfbb9b17640036405560343ca0b2`
- Runner: `dell-t430-remoteops-x64-01`
- 24 fully validated and paired original-model executions;
  the workflow printed `SML_RAE_PAIRED_24_ROWS_VALIDATED`
  and three SHA-256 records for provenance, source logs and CSV.

Observed mean wall-clock per 8-cycle run:

| Arm | Mean | Median |
| --- | ---: | ---: |
| Uninstrumented native MOR | 140,778 ns | 116,564 ns |
| Full R observer + exact MOR cache | 218,194 ns | 190,784 ns |

The instrumented candidate was slower in **23 of 24 pairs**.
Mean time ratio: `218193.5417 / 140778 = 1.5499` or
approximately **55.0% additional time**, *not* a speedup.
Median *paired* slowdown was about `1.6597x`; a ratio of medians
would represent a different statistic and must not be substituted.

The cache successfully avoided `6/8` native MOR evaluations in
every pair (144 calls over 192 cycles). The dirty journal nevertheless
performed a total of 192 packed-R word scans plus 192 full-reference
reconciliation scans across the 24 combined-observer runs. No R bit
changed after the initial state in this fixed synthetic workload, so
all 24 runs emitted zero dirty word notifications.

**Interpretation:** The last study establishes the overhead of
`observer + cache`, not of the cache alone. There is no demonstrated
acceleration of SML-GENIUS, no realistic trained-model HOLDOUT and no
physical DRAM/power evidence from this run.

## Functional follow-up in this PR

1. On an exact cache hit, preserve the existing `RaeExactMorCache`
   entry without rebuilding/cloning the entire cached native route
   and exact-key metadata. Original full-key equality remains
   compulsory (current SBG R field and native MOR hysteresis).
   Native MOR route is recomputed on a miss; all expert and Delta-KV
   stages still execute. The API is opt-in and disabled by default.
2. Add the ignored test
   `rae_p2_three_arm_native_cpu_cost_profile`. It runs 24
   deterministic *three-arm* comparisons in rotating order:
   **baseline**, **exact MOR cache only**, **observer + exact cache**.
   It checks full output, routing and Delta-KV memory parity across
   all three arms on every sample, and prints versioned raw rows
   plus counts of native calls avoided and observer reads.
3. Do not run this ignored physical measurement as ordinary PR
   testing, on an unadmitted runner, or on any server by an alternate
   control plane. SML source and runner identifiers are mandatory.
   A separately reviewed RemoteOps workflow must pin the exact
   merged SHA and run the experiment in an isolated admitted Dell
   session.

## Scientific controls and decision rule

- Exact cache-only must be faster than baseline on the majority of
  paired samples **and** exhibit favorable, reproducible paired
  central tendency before recommending default activation.
- Compare cache-only to observer+cache to isolate the cost of
  observation/reconciliation. Do not attribute all measured delta
  to journaling alone without controlled component experiments.
- Note that each model execution here includes only 8 cycles and
  a small deterministic SBG expert fixture; results do not
  extrapolate automatically to large Boolean states, model quality,
  full R scans, energy or GPU hardware.
- If pure cache is not beneficial, preserve it as opt-in research
  and search for an actual producer-owned R mutation-generation
  boundary before considering elimination of K/V or other model work.
- No learned candidate is allowed to override the native MOR
  choice as part of this change.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
```

Physical test (only after trusted RemoteOps admission and exact SHA
checkout, **not** as a normal GitHub PR job):

```bash
SML_RAE_SOURCE_SHA="$(git rev-parse HEAD)" \
SML_RAE_RUNNER="<admitted RemoteOps runner>" \
cargo test --locked --release -p sml-core \
  rae_p2_three_arm_native_cpu_cost_profile -- --ignored --nocapture
```
