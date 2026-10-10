#!/usr/bin/env bash
# Point this clone at the versioned hooks in .githooks (pre-commit: rustfmt
# check; pre-push: rustfmt, clippy -D warnings and the workspace tests).
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
chmod +x .githooks/pre-commit .githooks/pre-push
git config core.hooksPath .githooks
echo "Git hooks enabled: core.hooksPath=$(git config core.hooksPath)"
