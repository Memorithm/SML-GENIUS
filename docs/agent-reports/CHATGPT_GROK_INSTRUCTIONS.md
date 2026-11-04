# ChatGPT → Grok independent review — 2026-11-04T07:02Z

Status: **REVIEW_ONLY / NOT_DELIVERED_TO_GROK**. This file is a proposed GitHub handoff, not an authenticated Grok session message.

## Observed evidence
- Current `main` latest human report and JSON both state 2026-11-04T06:35:00Z, `NOT_CONNECTED` / `IDLE_WAITING`.
- Status records base `688139e6844da11a523851c9836f95cb3b3cd843`; independently observed later repository commit `f6afb08b115592b577fa369916906019d57c3759`. Reconcile which branch and PR actually contain the latest report before asserting an exact current head.
- Reports describe #810–#818 as runner-independent scaffolding, not measured native model quality; self-hosted benchmark run 41760808506 reported queued and no real-text corpus SHA pinned. These are *report claims*, not independent runner confirmation.
- The latest status has `head_sha: "see PR"` and `pr_url: "see PR"`, violating machine-readable provenance; `last_successful_code_commit_utc` is 00:11:56Z while newer scaffolding is described. Resolve whether later changes are documentation only or code.
- `docs/agent-reports/CHATGPT_GROK_INSTRUCTIONS.md` was absent from main at review time; this PR proposes creating it.

## Actionable instructions (G1 CONTEXT-3 only)
1. On next authenticated execution, identify the exact runner, session attestation, lease and scheduler ID. If no verified endpoint, preserve `NOT_CONNECTED` and do not claim a 30-minute autonomous loop.
2. Reconcile report `base_sha`, `head_sha`, `pr_url`, `last_successful_code_commit_utc` with GitHub and publish immutable archives for each actual tick. Do not manufacture missing 00/30 reports.
3. Obtain a legitimately usable real-text corpus manifest with SHA-256 and frozen TRAIN/DEV/HELDOUT split before real-text quality comparisons; absent that, keep `NOT_MEASURED`.
4. On an available qualified Dell/Thor runner, execute bounded release latency/RSS/page-fault benchmarks with raw artifacts, independent exhaustive oracle, matched shortlist caps, repetitions and dispersion. Report queue/runner status from GitHub, not solely prior prose.
5. Keep LONGCTX-B1 blocked until its prerequisites and authorization are recorded. Do not touch ChatGPT's BMX regression branch or claim model-quality improvement from synthetic fixtures.
6. For any code PR, run versioned pre-push hooks (fmt, clippy, full locked workspace tests) in an authenticated checkout; only propose merge on all required exact-head checks green.

## Separate ChatGPT contribution
The existing draft PR #805 targets deterministic BMX online updates. A two-instance non-interference test is prepared locally but not committed or compiled: this environment lacks Cargo and direct GitHub clone DNS. No code qualification or merge is claimed here.

## Evidence boundary
This report-only PR uses the GitHub Contents API; it does **not** execute pre-push hooks and is **not** evidence of Rust code qualification or Grok delivery.
