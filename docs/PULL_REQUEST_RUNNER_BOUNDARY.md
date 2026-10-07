# Pull-request runner trust boundary

Audit finding: `COG-SML-002`.

## Enforced boundary

Pull-request CI uses `pull_request_target`, so GitHub evaluates the workflow
definition from the base branch rather than from candidate code.

For a pull request:

- the job runs on `ubuntu-24.04`, a GitHub-hosted ephemeral runner;
- checkout is pinned to the pull request's exact head SHA;
- checkout credentials are not persisted;
- workflow permissions remain `contents: read`;
- no repository secret is supplied to candidate code.

For a push to `main`, the same job may run on the repository's self-hosted
runner. That path only receives code after review and integration.

A pull request therefore cannot change its own runner selection: workflow
changes affect `pull_request_target` only after review and merge.

## Installation qualification

The boundary was installed in two steps so every final candidate received
exact-head validation:

1. the bootstrap retained the legacy `pull_request` event while routing both
   PR event types to a GitHub-hosted runner;
2. this follow-up removes the legacy event and is itself evaluated by the
   already-installed `pull_request_target` definition from `main`.

The durable audit registry records the exact heads, runs, runner class, reviews
and target-branch validation. Scientific holdouts, preregistrations and result
claims are unchanged.

## Administrative limits

Repository rulesets and runner-group administration are not available to the
current GitHub App connection. This document does not claim that branch
protection or selected-workflow runner-group restrictions are configured.

At qualification time this repository is private. GitHub's announced
November 2, 2026 default block for `pull_request_target` applies to public
repositories. Before any future visibility change to public, maintainers must
either install and verify an Actions policy exception for this event or restore
an unprivileged GitHub-hosted `pull_request` validation path.
