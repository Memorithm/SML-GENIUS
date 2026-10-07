# Pull-request runner trust boundary

Audit finding: `COG-SML-002`.

## Intended final boundary

The CI workflow uses `pull_request_target`, so GitHub evaluates the workflow
definition from the base branch rather than from candidate code.

For a pull request:

- the job runs on `ubuntu-24.04`, a GitHub-hosted ephemeral runner;
- checkout is pinned to the pull request's exact head SHA;
- checkout credentials are not persisted;
- workflow permissions remain `contents: read`;
- no repository secret is supplied to candidate code.

For a push to `main`, the same job may run on the repository's self-hosted
runner. That path only receives code after review and integration.

## Two-step installation

The bootstrap PR temporarily retains the legacy `pull_request` trigger so its
exact head can execute all existing gates. Both pull-request event types select
the GitHub-hosted runner.

After the bootstrap is merged, a follow-up PR removes the legacy trigger. That
follow-up is evaluated by the already-installed `pull_request_target`
definition from `main`, proving the final boundary on its exact head. The
finding remains open until the follow-up and the target push are qualified.

Once the follow-up is integrated, a future pull request cannot change its own
runner selection: workflow changes affect `pull_request_target` only after
review and merge.

## Administrative limits

Repository rulesets and runner-group administration are not available to the
current GitHub App connection. This document does not claim that branch
protection or selected-workflow runner-group restrictions are configured.
