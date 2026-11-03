# Grok SML-GENIUS report — 2026-11-03T19:21:10Z

- Status: **IN_PROGRESS** (PR opened with real work; session_attestation NOT available — see RemoteOps)
- remoteops_wake_task_id: `sml-genius-20261103T191749Z-8152791e` (milestone G1)
- RemoteOps Actions run: https://github.com/Memorithm/RemoteOps/actions/runs/41701874209
- Milestone: G1 / CONTEXT-3 neighbour-bucket shortlist (LONGCTX-B1 and BDE-1 untouched)
- PR: https://github.com/Memorithm/SML-GENIUS/pull/804
- Base: 1668554916fe7a31e44c4b3ad3d01d50b88ef7d1 (main, #803)
- Tested code commit: 1463f95b053b04fba66edcc2bba830ffb901b7d6 on `feat/g1-context3-neighbor-buckets` (committed 2026-11-03T19:20:39Z). This report commit sits on top of it as the PR head.

## Change
- Minimal src: `ContextRetriever::retrieve_with_neighbor_radius(query, policy, bucket_radius)` in `crates/sml-store/src/context_retrieve.rs`. `retrieve` now delegates with radius 0. The test asserts that radius 0 gives the same hits and accounting as `retrieve`.
- Bucket keys within Hamming distance `bucket_radius` of the query key are probed in `(key distance, key)` order. The `max_shortlist` cap is unchanged. A radius larger than `bucket_prefix_bits` returns `BudgetOverflow`.
- Radius choice: **r=1 is primary** (it probes ≤ prefix_bits+1 buckets). r=2 is also measured.
- Tests: `crates/sml-store/tests/context_retrieve_neighbor_buckets.rs` (5 tests)
  - matrix r∈{0,1,2}
  - 1-bit bucket-miss recovery
  - honest failure beyond the radius
  - abstention and over-radius rejection
  - corruption in a neighbour bucket fails closed

## Measured (debug build, single run, --test-threads=1)
Command: `cargo test -p sml-store --test context_retrieve_neighbor_buckets -- --nocapture --test-threads=1`

The same distributions, sizes, seeds (48 queries) and cap (32) as #803. The r=0 rows reproduce #803's G1_RECALL recall and accounting numbers exactly.

| dist | N | prefix | R@1 r0→r1 | R@4 r0→r1 | abstain r0→r1 | shortlisted r0→r1 | reread B r0→r1 |
|---|---|---|---|---|---|---|---|
| uniform | 64 | 4 | 0.771→0.979 | 0.292→0.635 | 0.000→0.000 | 4.6→20.2 | 35.1→37.3 |
| uniform | 64 | 8 | 0.625→0.938 | 0.167→0.307 | 0.229→0.021 | 0.9→3.4 | 14.0→30.7 |
| uniform | 256 | 4 | 0.771→0.771 | 0.365→0.443 | 0.000→0.000 | 16.8→32.0 | 37.3→37.3 |
| uniform | 256 | 8 | 0.625→0.938 | 0.177→0.375 | 0.083→0.000 | 1.6→9.9 | 20.6→37.3 |
| uniform | 1024 | 4 | 0.417→0.417 | 0.224→0.224 | 0.000→0.000 | 32.0→32.0 | 37.3→37.3 |
| uniform | 1024 | 8 | 0.625→0.896 | 0.208→0.427 | 0.000→0.000 | 4.8→30.8 | 35.3→37.3 |
| clustered | 64 | 4 | 0.771→0.979 | 0.771→0.979 | 0.208→0.021 | 12.7→26.0 | 29.6→36.6 |
| clustered | 64 | 8 | 0.625→0.938 | 0.625→0.938 | 0.375→0.062 | 10.0→15.0 | 23.3→35.0 |
| clustered | 256 | 4 | 0.375→0.438 | 0.484→0.552 | 0.208→0.021 | 25.3→31.3 | 29.6→36.6 |
| clustered | 256 | 8 | 0.333→0.479 | 0.411→0.583 | 0.375→0.062 | 20.0→30.0 | 23.3→35.0 |
| clustered | 1024 | 4 | 0.125→0.146 | 0.177→0.219 | 0.208→0.021 | 25.3→31.3 | 29.6→36.6 |
| clustered | 1024 | 8 | 0.083→0.167 | 0.146→0.234 | 0.375→0.062 | 20.0→30.0 | 23.3→35.0 |
| noisy_clustered | 64 | 4 | 0.771→0.979 | 0.411→0.828 | 0.000→0.000 | 5.4→23.1 | 33.1→37.3 |
| noisy_clustered | 64 | 8 | 0.625→0.938 | 0.266→0.641 | 0.271→0.000 | 1.5→5.5 | 16.4→33.8 |
| noisy_clustered | 256 | 4 | 0.708→0.771 | 0.531→0.583 | 0.000→0.000 | 21.5→32.0 | 37.0→37.3 |
| noisy_clustered | 256 | 8 | 0.625→0.938 | 0.375→0.750 | 0.083→0.000 | 5.4→19.9 | 26.9→36.8 |
| noisy_clustered | 1024 | 4 | 0.271→0.271 | 0.240→0.240 | 0.000→0.000 | 30.4→32.0 | 37.3→37.3 |
| noisy_clustered | 1024 | 8 | 0.542→0.688 | 0.307→0.448 | 0.000→0.000 | 12.5→30.1 | 35.6→37.3 |

Raw output:
```
G1_NEIGHBOR uniform 64 4 0 0.771 0.469 0.292 0.000 4.6 2.19 35.1 54.5 803072
G1_NEIGHBOR uniform 64 4 1 0.979 0.792 0.635 0.000 20.2 2.33 37.3 58.8 803072
G1_NEIGHBOR uniform 64 4 2 0.979 0.917 0.812 0.000 32.0 2.33 37.3 64.1 803072
G1_NEIGHBOR uniform 64 8 0 0.625 0.323 0.167 0.229 0.9 0.88 14.0 20.5 803072
G1_NEIGHBOR uniform 64 8 1 0.938 0.552 0.307 0.021 3.4 1.92 30.7 45.6 803072
G1_NEIGHBOR uniform 64 8 2 1.000 0.740 0.562 0.000 9.6 2.33 37.3 57.0 803072
G1_NEIGHBOR uniform 256 4 0 0.771 0.542 0.365 0.000 16.8 2.33 37.3 59.2 3212288
G1_NEIGHBOR uniform 256 4 1 0.771 0.583 0.443 0.000 32.0 2.33 37.3 64.7 3212288
G1_NEIGHBOR uniform 256 4 2 0.771 0.583 0.443 0.000 32.0 2.33 37.3 66.9 3212288
G1_NEIGHBOR uniform 256 8 0 0.625 0.344 0.177 0.083 1.6 1.28 20.6 30.4 3212288
G1_NEIGHBOR uniform 256 8 1 0.938 0.594 0.375 0.000 9.9 2.33 37.3 65.0 3212288
G1_NEIGHBOR uniform 256 8 2 0.979 0.719 0.599 0.000 31.5 2.33 37.3 79.5 3212288
G1_NEIGHBOR uniform 1024 4 0 0.417 0.271 0.224 0.000 32.0 2.33 37.3 70.1 12849152
G1_NEIGHBOR uniform 1024 4 1 0.417 0.271 0.224 0.000 32.0 2.33 37.3 67.2 12849152
G1_NEIGHBOR uniform 1024 4 2 0.417 0.271 0.224 0.000 32.0 2.33 37.3 67.3 12849152
G1_NEIGHBOR uniform 1024 8 0 0.625 0.354 0.208 0.000 4.8 2.21 35.3 53.6 12849152
G1_NEIGHBOR uniform 1024 8 1 0.896 0.583 0.427 0.000 30.8 2.33 37.3 80.5 12849152
G1_NEIGHBOR uniform 1024 8 2 0.896 0.583 0.432 0.000 32.0 2.33 37.3 83.6 12849152
G1_NEIGHBOR clustered 64 4 0 0.771 0.771 0.771 0.208 12.7 1.85 29.6 54.2 803072
G1_NEIGHBOR clustered 64 4 1 0.979 0.979 0.979 0.021 26.0 2.28 36.6 59.3 803072
G1_NEIGHBOR clustered 64 4 2 1.000 1.000 1.000 0.000 32.0 2.33 37.3 62.1 803072
G1_NEIGHBOR clustered 64 8 0 0.625 0.625 0.625 0.375 10.0 1.46 23.3 35.0 803072
G1_NEIGHBOR clustered 64 8 1 0.938 0.938 0.938 0.062 15.0 2.19 35.0 50.5 803072
G1_NEIGHBOR clustered 64 8 2 1.000 1.000 1.000 0.000 17.7 2.33 37.3 55.3 803072
G1_NEIGHBOR clustered 256 4 0 0.375 0.469 0.484 0.208 25.3 1.85 29.6 51.7 3212288
G1_NEIGHBOR clustered 256 4 1 0.438 0.552 0.552 0.021 31.3 2.28 36.6 62.4 3212288
G1_NEIGHBOR clustered 256 4 2 0.438 0.573 0.562 0.000 32.0 2.33 37.3 66.5 3212288
G1_NEIGHBOR clustered 256 8 0 0.333 0.396 0.411 0.375 20.0 1.46 23.3 39.9 3212288
G1_NEIGHBOR clustered 256 8 1 0.479 0.573 0.583 0.062 30.0 2.19 35.0 60.4 3212288
G1_NEIGHBOR clustered 256 8 2 0.500 0.615 0.620 0.000 32.0 2.33 37.3 62.3 3212288
G1_NEIGHBOR clustered 1024 4 0 0.125 0.146 0.177 0.208 25.3 1.85 29.6 56.1 12849152
G1_NEIGHBOR clustered 1024 4 1 0.146 0.177 0.219 0.021 31.3 2.28 36.6 64.2 12849152
G1_NEIGHBOR clustered 1024 4 2 0.146 0.177 0.219 0.000 32.0 2.33 37.3 72.0 12849152
G1_NEIGHBOR clustered 1024 8 0 0.083 0.115 0.146 0.375 20.0 1.46 23.3 43.2 12849152
G1_NEIGHBOR clustered 1024 8 1 0.167 0.208 0.234 0.062 30.0 2.19 35.0 70.0 12849152
G1_NEIGHBOR clustered 1024 8 2 0.167 0.208 0.240 0.000 32.0 2.33 37.3 68.3 12849152
G1_NEIGHBOR noisy_clustered 64 4 0 0.771 0.542 0.411 0.000 5.4 2.07 33.1 51.4 803072
G1_NEIGHBOR noisy_clustered 64 4 1 0.979 0.948 0.828 0.000 23.1 2.33 37.3 64.1 803072
G1_NEIGHBOR noisy_clustered 64 4 2 0.979 0.958 0.880 0.000 32.0 2.33 37.3 74.1 803072
G1_NEIGHBOR noisy_clustered 64 8 0 0.625 0.375 0.266 0.271 1.5 1.03 16.4 25.0 803072
G1_NEIGHBOR noisy_clustered 64 8 1 0.938 0.781 0.641 0.000 5.5 2.11 33.8 50.9 803072
G1_NEIGHBOR noisy_clustered 64 8 2 1.000 0.948 0.828 0.000 13.1 2.30 36.8 71.9 803072
G1_NEIGHBOR noisy_clustered 256 4 0 0.708 0.604 0.531 0.000 21.5 2.31 37.0 60.3 3212288
G1_NEIGHBOR noisy_clustered 256 4 1 0.771 0.646 0.583 0.000 32.0 2.33 37.3 64.4 3212288
G1_NEIGHBOR noisy_clustered 256 4 2 0.771 0.646 0.583 0.000 32.0 2.33 37.3 64.4 3212288
G1_NEIGHBOR noisy_clustered 256 8 0 0.625 0.469 0.375 0.083 5.4 1.68 26.9 39.3 3212288
G1_NEIGHBOR noisy_clustered 256 8 1 0.938 0.802 0.750 0.000 19.9 2.30 36.8 64.2 3212288
G1_NEIGHBOR noisy_clustered 256 8 2 0.958 0.865 0.807 0.000 30.6 2.33 37.3 73.9 3212288
G1_NEIGHBOR noisy_clustered 1024 4 0 0.271 0.240 0.240 0.000 30.4 2.33 37.3 91.8 12849152
G1_NEIGHBOR noisy_clustered 1024 4 1 0.271 0.240 0.240 0.000 32.0 2.33 37.3 84.3 12849152
G1_NEIGHBOR noisy_clustered 1024 4 2 0.271 0.240 0.240 0.000 32.0 2.33 37.3 84.9 12849152
G1_NEIGHBOR noisy_clustered 1024 8 0 0.542 0.406 0.307 0.000 12.5 2.22 35.6 62.3 12849152
G1_NEIGHBOR noisy_clustered 1024 8 1 0.688 0.531 0.448 0.000 30.1 2.33 37.3 82.4 12849152
G1_NEIGHBOR noisy_clustered 1024 8 2 0.688 0.542 0.453 0.000 32.0 2.33 37.3 77.5 12849152
```

## Interpretation
- **Quality:**
  - On these synthetic fixtures, r=1 raised R@1 in 15 of 18 cells and never lowered it.
  - The gain is largest at prefix 8 (for example uniform 1024: 0.625→0.896).
  - The cap-bound cells (prefix 4, N=1024) are unchanged: the cap fills from the own bucket.
  - Clustered at large N stays poor (R@1 ≤0.17): the cap, not the bucket radius, is the limit there.
  - This is not a generalization claim.
- **Cost:**
  - More candidates are scored (bounded by the cap of 32).
  - Decodes and reread bytes grow modestly, because fewer queries abstain.
  - Debug latency per query is up to about 2× (indicative only).
- Bucket-miss: r=1 recovered 64/64 clean 1-bit cases. On 2-bit key misses, r=1 found 0/64 (an honest failure) and r=2 found 64/64.
- **NOT_MEASURED:** peak RSS/heap, release-build latency, run-to-run variance.

## Evidence
- fmt; `cargo clippy -p sml-store --all-targets -D warnings` pass; pre-push hook (no --no-verify) on 1463f95b: fmt, clippy --workspace -D warnings, `cargo test --locked --workspace` 279 suites, 2138 passed, 0 failed.

## RemoteOps (E2E evidence, not a qualification verdict)
- The lease was idle at wake: #803 squash-merged as 16685549 and its poll was deleted. Not SKIPPED_LOCKED.
- Wake `sml-genius-20261103T191749Z-8152791e` was received at about 19:17:49Z. This cycle started after it.
- First commit 1463f95b is at 19:20:39Z, after the wake.
- History:
  - sml-genius-20261103T174453Z-bb866975 → SKIPPED_LOCKED
  - sml-genius-20261103T183120Z-f9624612 (run 41694549374) → SKIPPED_LOCKED (lease held by the #803 cycle)
- session_attestation is **not produced**: no attested-invocation artifact exists for this run, and none was invented. Status therefore stays IN_PROGRESS.
- The 30-min schedule is NOT enabled. A draft patch exists only locally at /workspace/agent-tools/remoteops-grok-wake-30min-draft.patch.
- `docs/agent-reports/CHATGPT_GROK_INSTRUCTIONS.md`: 404 on main (missing).

## Next G1 task
- A release-build latency protocol with repeated runs.
- A per-bucket cap split (so clustered/cap-bound cells benefit).
- An RSS probe.

## Gate
`python3 ops/grok/grok_report_gate.py --status docs/agent-reports/GROK_SML_GENIUS_STATUS.json --report docs/agent-reports/GROK_SML_GENIUS_LATEST.md` (RemoteOps copy) → exit 0: `VALIDATED: status/report schema only; Grok invocation NOT independently attested`

## Clustered failure focus (ChatGPT t121u)
All numbers come from the measured table above. Each value goes from r=0 (#803 baseline) to r=1.

| N | prefix | R@1 | R@2 | R@4 | Abstention |
|---|---|---|---|---|---|
| 1024 | 8 | 0.083→0.167 | 0.115→0.208 | 0.146→0.234 | 0.375→0.062 |
| 1024 | 4 | 0.125→0.146 | — | 0.177→0.219 | 0.208→0.021 |

- **False negatives:** r=1 mostly removes abstention, because the empty-bucket misses get filled from neighbouring buckets. Top-1 recall stays poor because the 32-candidate cap fills before the true neighbour is reached. Clustered failure is **not fixed**; it is cap/ordering-bound.
- **Search cost** (mean candidates scored, r=0→r=1, vs exhaustive N=1024): 20.0→30.0 at prefix 8, 25.3→31.3 at prefix 4. Debug-build time per query: about 41.5→67.6 µs at prefix 8.
- **NOT_MEASURED:** peak RSS (no reliable probe), multi-run variance (single run; recall and accounting are deterministic on fixed seeds, timing is not), release-build latency.

## Wake → contribution chain
1. Wake `sml-genius-20261103T191749Z-8152791e` from RemoteOps run 41701874209, received about 19:17:49Z.
2. Lease was idle (#803 merged as 16685549).
3. This cycle branched from 16685549.
4. Code commit 1463f95b at 19:20:39Z.
5. PR #804 opened.
6. Local pre-push passed: 2138 tests, 0 failed.
7. CI on the exact head is pending at report time.
8. Gate exit 0 (schema only).
