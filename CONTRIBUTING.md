# Contributing

## Local gates (mandatory before every push)

Enable the versioned git hooks once per clone:

    bash scripts/install-hooks.sh    # == git config core.hooksPath .githooks

- `.githooks/pre-commit` runs `cargo fmt --all -- --check` (fast).
- `.githooks/pre-push` runs the same gates as CI, in the same order:
  `cargo fmt --all -- --check`, then
  `cargo clippy --locked --workspace --all-targets -- -D warnings`, then
  `cargo test --locked --workspace`.

A push that fails any gate is refused locally. Do not bypass the hooks with
`--no-verify`; a "fix CI" follow-up commit is the failure mode these hooks
exist to prevent.

## Pull requests

One significant change per PR. Fill in `.github/pull_request_template.md`:
what changes, what the tests prove (a test must call the code it claims to
cover), and the fmt/clippy/test checklist.
