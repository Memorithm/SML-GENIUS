# Pull-request runner trust boundary

Audit finding: `COG-SML-002`.

## Enforced workflow boundary

The CI workflow is installed on `main` and is triggered with
`pull_request_target`. GitHub therefore evaluates the workflow definition from
the base branch rather than from candidate code.

For a pull request:

- the job runs on `ubuntu-24.04`, a GitHub-hosted ephemeral runner;
- checkout is pinned to the pull request's exact head SHA;
- checkout credentials are not persisted;
- workflow permissions remain `contents: read`;
- no repository secret is supplied to candidate code.

For a push to `main`, the same job may run on the repository's self-hosted
runner. That path only receives code after review and integration.

This split prevents a pull request from changing its own runner selection and
then executing candidate code on the persistent runner. Future changes to the
workflow take effect for pull requests only after those changes are merged.

## Qualification and limits

A follow-up pull request must exercise the installed base-branch definition and
record the exact candidate SHA and runner class before this audit finding is
closed. A successful bootstrap run alone is not sufficient, because the old
`pull_request` definition controls that bootstrap run.

Repository rulesets and runner-group administration are not available to the
current GitHub App connection. This document does not claim that branch
protection or selected-workflow runner-group restrictions are configured.
