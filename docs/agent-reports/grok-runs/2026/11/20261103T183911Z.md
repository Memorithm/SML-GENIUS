# Grok SML-GENIUS report — 2026-11-03T18:39:11Z

- Status: **IN_PROGRESS** (RemoteOps E2E NOT qualified; 30-min schedule NOT enabled)
- Milestone: G1 / CONTEXT-3 Recall@K tranche (LONGCTX-B1 and BDE-1 untouched)
- PR: https://github.com/Memorithm/SML-GENIUS/pull/803 (not merged)
- Base: 754b523cc0b67aa7279f58cb2f26758718c5e6df (main, #802)
- Tested code commit: 6e668b51214b21b535ed7bd5f7411f5057f3e01d on `feat/g1-context3-recall-k`. This report commit sits on top of it as the PR head.

## Change
Tests only: `crates/sml-store/tests/context_retrieve_recall_k.rs` (4 tests). The PR #803 body has the full test list.
- Recall@K is measured as |retrieved ∩ oracle top-K|/K; ties at the K-th distance count as hits.
- Reference points: the independent oracle and the existing `exhaustive_semantic_scan` control. The test asserts the control equals the oracle.

## Measured table (debug build, single run, re-run for this report with --test-threads=1)
Command: `cargo test -p sml-store --test context_retrieve_recall_k -- --nocapture --test-threads=1`
Columns are as printed by the header line. Accounting values are mean per call across K∈{1,2,4}.
```
G1_RECALL uniform 64 4 0.771 0.469 0.292 0.000 4.6 2.19 35.1 64.0 37.3 51.9 69.9 803072
G1_RECALL uniform 64 8 0.625 0.323 0.167 0.229 0.9 0.88 14.0 64.0 37.3 24.5 80.5 803072
G1_RECALL uniform 256 4 0.771 0.542 0.365 0.000 16.8 2.33 37.3 256.0 37.3 57.9 129.1 3212288
G1_RECALL uniform 256 8 0.625 0.344 0.177 0.083 1.6 1.28 20.6 256.0 37.3 29.1 128.2 3212288
G1_RECALL uniform 1024 4 0.417 0.271 0.224 0.000 32.0 2.33 37.3 1024.0 37.3 72.5 403.8 12849152
G1_RECALL uniform 1024 8 0.625 0.354 0.208 0.000 4.8 2.21 35.3 1024.0 37.3 61.7 434.3 12849152
G1_RECALL clustered 64 4 0.771 0.771 0.771 0.208 12.7 1.85 29.6 64.0 37.3 46.5 69.7 803072
G1_RECALL clustered 64 8 0.625 0.625 0.625 0.375 10.0 1.46 23.3 64.0 37.3 34.7 67.1 803072
G1_RECALL clustered 256 4 0.375 0.469 0.484 0.208 25.3 1.85 29.6 256.0 37.3 53.0 134.3 3212288
G1_RECALL clustered 256 8 0.333 0.396 0.411 0.375 20.0 1.46 23.3 256.0 37.3 39.8 131.7 3212288
G1_RECALL clustered 1024 4 0.125 0.146 0.177 0.208 25.3 1.85 29.6 1024.0 37.3 55.0 412.5 12849152
G1_RECALL clustered 1024 8 0.083 0.115 0.146 0.375 20.0 1.46 23.3 1024.0 37.3 41.9 404.2 12849152
G1_RECALL noisy_clustered 64 4 0.771 0.542 0.411 0.000 5.4 2.07 33.1 64.0 37.3 47.8 67.6 803072
G1_RECALL noisy_clustered 64 8 0.625 0.375 0.266 0.271 1.5 1.03 16.4 64.0 37.3 23.8 67.4 803072
G1_RECALL noisy_clustered 256 4 0.708 0.604 0.531 0.000 21.5 2.31 37.0 256.0 37.3 60.2 131.9 3212288
G1_RECALL noisy_clustered 256 8 0.625 0.469 0.375 0.083 5.4 1.68 26.9 256.0 37.3 39.0 129.9 3212288
G1_RECALL noisy_clustered 1024 4 0.271 0.240 0.240 0.000 30.4 2.33 37.3 1024.0 37.3 74.5 408.2 12849152
G1_RECALL noisy_clustered 1024 8 0.542 0.406 0.307 0.000 12.5 2.22 35.6 1024.0 37.3 61.5 404.0 12849152
G1_CAP clustered 512 cap4_R@1=0.000 cap256_R@1=0.719 cap4_R@4=0.008 cap256_R@4=0.719
```

## Interpretation
- **Quality:** the shortlist has substantial recall loss vs the oracle (R@1 0.08–0.77; worst on clustered, large N). No improvement is claimed.
- **Cost:** the shortlist scores ≤32 candidates vs N; reread bytes are about equal (≤K×16 bytes). Debug-build timings are indicative only.
- **Limits:**
  - Synthetic 64-bit signatures only.
  - Bucket-miss and cap truncation are demonstrated failure modes.
  - **NOT_MEASURED:** peak RSS/heap, release-build latency, run-to-run variance.

## Evidence
- pre-push hook (no --no-verify): fmt --check pass; clippy --locked --workspace --all-targets -D warnings pass; `cargo test --locked --workspace`: 277 suites, 2133 passed, 0 failed (on 6e668b51).

## RemoteOps
- Earlier wake sml-genius-20261103T174453Z-bb866975 → SKIPPED_LOCKED (prior cycle).
- Wake sml-genius-20261103T183120Z-f9624612 was received at 19:31 Europe/Paris, while this cycle was already active (this cycle was started by the user at about 19:28 Paris). It was recorded as SKIPPED_LOCKED. **That wake did NOT start this work.**
- E2E not qualified; the schedule is not enabled.

## Next G1 task
- Multi-probe bucket lookup (neighbouring prefixes within Hamming radius r), measured against this exact matrix.
- Release-build latency protocol with repeated runs.
- An RSS probe, or keep RSS explicitly NOT_MEASURED.
