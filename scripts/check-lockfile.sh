#!/usr/bin/env bash
set -euo pipefail
if ! git ls-files --error-unmatch Cargo.lock >/dev/null 2>&1; then
  echo 'ERROR: Cargo.lock must be tracked before merge.' >&2
  echo 'GENERATED_LOCKFILE_BEGIN'
  cat Cargo.lock
  echo 'GENERATED_LOCKFILE_END'
  exit 1
fi
git diff --exit-code -- Cargo.lock
