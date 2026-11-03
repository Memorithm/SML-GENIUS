# GROK — SML-GENIUS continuous development contract (2026-11-03)

**Status:** proposed, pending PR qualification and merge. **Owner:** Grok for assigned slices; ChatGPT for hourly independent review. **Repository:** `Memorithm/SML-GENIUS`. **Control plane:** `Memorithm/RemoteOps`.

## Mission and non-negotiable boundaries

Deliver a **genuinely trainable, measured, native Rust Boolean model**, not a collection of static Boolean engines. The programme target is the **first real SML-6 25B trained model**, followed by instruction tuning, independent qualification and, only then, the separate SML-HARNESS. Follow `docs/ROADMAP.md`, `AGENTS.md`, `BOOTSTRAP.md` and the off-main `agent/ecosystem-roadmap:.agent/SML_GENIUS_ECOSYSTEM_ROADMAP.yaml`. Read RemoteOps `AGENTS.md` and its off-main roadmap before any remote dispatch.

Each new engine or mechanism must include (a) actual learning/update semantics, (b) frozen TRAIN/DEV/HELDOUT evaluation, (c) independent correctness oracle, (d) measured performance and resource accounting, (e) baseline/ablation, and (f) negative controls. An untrained algorithm, a benchmark-only kernel or a synthetic demonstration is **not** a model-quality promotion. No mandatory NVIDIA stack; use Rust and vendor-neutral CPU/WGPU where qualified. No protected TEST tuning, fabricated metrics, forced merge, bypassed pre-push hooks or branch protections.

## Grok scope, dependencies and milestones

**G1 — CONTEXT-3 qualification (first priority).** Rebase/inspect the merged `context_retrieve` implementation and 12 fixture tests from PR #768. Do not duplicate already merged work. Add an independent exhaustive retrieval oracle, provenance and tie/abstention/corruption tests, bounded-work counters, measured latency/RSS/page faults on a qualified runner, and matched exhaustive/fixed controls. Introduce learned selection/ranking only with actual training and heldout measurements. Deliver PRs with one independently testable hypothesis each.

**G2 — CONTEXT-2/4/5 integration.** Implement bounded recurrent working context, hot/warm/cold retention with immutable raw provenance, and history refinement/adaptive stopping. Compare fixed versus learned policies with matched budgets. Keep raw exact-source verification and crash/reopen behavior fail-closed. Do not claim an effective context span from storage capacity.

**G3 — LONGCTX-B1 prerequisites, NOT an unauthorized campaign.** Reuse merged PR #772 harness and `sml-byte-v0`. Freeze and document corpus SHA-256, choose and pin B-lex reference and B-attn comparator with weights/runtime/context, independent scorer, token accounting, train/dev/protected evaluation separation, budget and stopping rules. Mark B1 `BLOCKED` until all preregistered prerequisites and approvals are met; do not launch the protected B1 campaign without authorization.

**G4 — Trainable context-to-model bridge.** Connect context retrieval to SBG/MOR, learned expert selection, bounded Delta-KV, recurrent rewrite and native decision/output. Evaluate learning with versus without retrieval, fixed versus learned routing, correct versus distractor provenance, and matched resource budgets. No generic all-history scoring in the inference hot path.

**G5 — Scientific contribution to SML-1→SML-2.** Coordinate with ChatGPT's BDE-0/BDE-1 work without editing its assigned files concurrently. Propose and implement trainable nonlinear Boolean credit assignment, learned wiring or memory-control experiments only after preregistered ablations and heldout baselines. Prioritize predictive quality on real language over atlas size or isolated kernel speed.

**G6 — Scale evidence.** Only after G4/G5 show reproducible learned-model value, contribute reproducible 1–5M then 50M training/inference experiments; report quality, active/total learned parameters, throughput, storage, RSS, memory traffic, time and uncertainty. Do not represent untrained parameters or raw storage capacity as achieved model scale.

## 30-minute executor contract (external scheduler, not ChatGPT Automations)

An authorized **RemoteOps/GitHub Actions scheduler** must trigger Grok's **actual connected coding agent** at minute 00 and minute 30, Europe/Paris, continuously. A cron job that only writes “continue” or a report is **not** proof that Grok developed anything. Each tick:
1. Acquire a non-overlapping lease/lock (skip with `SKIPPED_LOCKED` if previous run still active); fetch `main`, contracts, off-main roadmaps, PRs and exact-head CI.
2. Read the latest ChatGPT instructions and prior report; choose the highest-priority unblocked, smallest measurable slice; resume an open PR before opening another.
3. Implement Rust code, tests and protocol on a task branch; run `bash scripts/install-hooks.sh`, `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace`, plus focused heldout measurement on an authorized qualified runner. Never use `--no-verify`.
4. Commit/push via the qualified checkout, open/update PR, verify exact SHA CI and required reviews. Merge only when all applicable rules are satisfied; RSI candidate promotion retains human GitHub merge authorization.
5. Atomically publish the report and machine-readable status with exact evidence. If blocked, identify a concrete unblock task for the next tick rather than fabricating work. Retry transient errors with bounded backoff; do not loop indefinitely.

**A 30-minute trigger requires external orchestration. ChatGPT scheduled tasks are hourly at most and cannot by themselves connect to or compel a separate Grok session.** Until a verified Grok agent endpoint, authorization, runner and scheduler exist, report `NOT_CONNECTED` and do not claim the half-hour loop is active.

## Mandatory report files (stable paths)

- **Latest human report:** `docs/agent-reports/GROK_SML_GENIUS_LATEST.md`
- **Latest machine status:** `docs/agent-reports/GROK_SML_GENIUS_STATUS.json`
- **Append-only run history:** `docs/agent-reports/grok-runs/YYYY/MM/YYYYMMDDTHHMMSSZ.md`
- **ChatGPT review/instructions:** `docs/agent-reports/CHATGPT_GROK_INSTRUCTIONS.md`

Every tick MUST publish the latest report (including no-op or blocked runs), and archive it in a timestamped immutable path. Use a dedicated reports branch/PR or authorized automation branch, never bypass protections or race on `main`. GitHub API file writes do not run local hooks: any branch used for code must go through the qualified checkout and hooks; report-only writes must not be confused with code qualification. Reports must contain no secrets, raw private corpora or protected test data.

Required report fields:
- UTC timestamp, run ID, scheduler ID, Grok agent identity/session attestation, runner, task lease and execution status (`COMPLETED`, `IN_PROGRESS`, `BLOCKED`, `FAILED`, `NOT_CONNECTED`, `SKIPPED_LOCKED`);
- objective and exact roadmap milestone; base/head SHA, changed files, commits and PR URL;
- exact test commands and results, CI run URLs, hook evidence, baseline and ablation;
- TRAIN/DEV/HELDOUT dataset IDs and hashes, seeds, measured quality, latency, RAM/VRAM/traffic and uncertainty, or explicit `NOT_MEASURED`;
- negative results, limitations, blocking reason, next executable task and dependencies;
- handoff for ChatGPT review, with machine-readable `last_successful_code_commit_utc` and `last_report_utc`.

Example minimal status JSON (replace placeholders only with observed values):

```json
{
  "schema_version": 1,
  "timestamp_utc": null,
  "run_id": null,
  "status": "NOT_CONNECTED",
  "agent": "grok",
  "runner": null,
  "base_sha": null,
  "head_sha": null,
  "pr_url": null,
  "last_successful_code_commit_utc": null,
  "last_report_utc": null,
  "blocked_reason": "Grok execution endpoint and 30-minute RemoteOps scheduler not yet verified"
}
```

## ChatGPT hourly independent review

The existing hourly ChatGPT SML-GENIUS development automation must additionally:
1. Fetch latest Grok report/status and newest archived run, verify timestamps and SHA/PR/CI against GitHub; compare with previous hour and detect missing half-hour ticks.
2. Independently review code, learning evidence, baselines, performance, regressions, tests, security and compliance with AGENTS/roadmap. Reject unsupported model-quality or speedup claims.
3. Publish precise corrections, next priority, acceptance criteria and blockers to `docs/agent-reports/CHATGPT_GROK_INSTRUCTIONS.md` via reviewed, authorized GitHub changes. Avoid modifying Grok's code branch concurrently.
4. Continue **its own** Rust Boolean learning development independently; prioritize existing PRs. Only send an instruction directly to Grok when a verified connected interface exists. Publishing a file is not proof Grok read it.
5. Escalate missing reports after 2 scheduled ticks, stale code after 4 ticks, repeated failing CI or unavailable runners. Do not equate inactivity with failure without checking the agent/scheduler state.

## Completion criteria

A milestone is complete only when the code and measured learning improvement exist, independent oracle and negative controls pass, exact-head required CI and hooks are evidenced, results and artifacts are archived, and GitHub merge authorization is satisfied. The final programme requires an actually trained, independently evaluated SML-25B model meeting the preregistered learning and quality/cost parity criteria; targets are not achievements.

## Immediate bootstrap tasks

1. Merge this documentation contract after required review/CI; retain off-main ecosystem-roadmap boundary.
2. Verify that Grok has a real callable agent/session endpoint and an authorized RemoteOps runner; record proof without credentials.
3. Add and qualify a dedicated 00/30 RemoteOps scheduler with lock, retry, safe permissions and report writer. Do not activate a fake timer.
4. Initialize the four report paths on a protected workflow/report branch; test one real Grok run and one ChatGPT review with observed GitHub SHA.
5. Enable the half-hour cycle only after end-to-end evidence confirms Grok execution, real code/testing and report publication.
