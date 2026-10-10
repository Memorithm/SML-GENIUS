## What changes

<!-- One significant change per PR. Files/modules touched and the behaviour that differs. -->

## What the tests prove

<!-- For each new or changed test: which behaviour it exercises, which public
API it calls, and what would make it fail. A test that never calls the new
code, or only re-checks a frozen fixture, proves nothing about this change.
For an "integration" PR, name the causal assertion (upstream change => observable
downstream change). -->

## Checklist

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --locked --workspace --all-targets -- -D warnings`
- [ ] `cargo test --locked --workspace`
- [ ] Hooks enabled locally (`bash scripts/install-hooks.sh`)
