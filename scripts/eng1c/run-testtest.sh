#!/usr/bin/env bash
# Frozen TESTTEST campaign; scientific STOP is a valid completed result.
set -euo pipefail
export LC_ALL=C
if [[ $# != 2 || -e "$1" || ( "$2" != FR && "$2" != EN && "$2" != ALL ) ]]; then
  echo 'usage: run-testtest.sh <NEW-root> FR|EN|ALL' >&2
  exit 2
fi
root="$1"
language="$2"
repo="$(git rev-parse --show-toplevel)"
manifest="$repo/scripts/eng1c/testtest-manifest.tsv"
bin="$repo/target/release/sml-eng1c-testtest"
test -x "$bin"
mkdir -p "$root/private/raw" "$root/private/work" "$root/evidence"
cleanup() { rm -rf -- "$root/private"; }
trap cleanup EXIT
{
  git rev-parse HEAD
  rustc --version
  uname -m
  sha256sum "$bin" "$manifest" "$repo/scripts/eng1c/testtest-roster.tsv" \
    "$repo/crates/sml-core/src/bin/sml-eng1c-testtest.rs" "$repo/crates/sml-core/src/paired_panel.rs"
  echo 'TESTTEST_ORIGINAL_DUMAS_AUSTEN_TEST=UNREAD'
  echo 'TESTTEST_INTERPRETATION=HISTORICAL_FR_EN_PANEL_ONLY'
} > "$root/evidence/environment-$language.txt"
while IFS=$'\t' read -r id lang genre author title url; do
  [[ "$id" == id ]] && continue
  if [[ "$language" != ALL && "$lang" != "$language" ]]; then continue; fi
  echo "TESTTEST_BEGIN,id=$id,language=$lang,genre=$genre"
  curl --proto '=https' --tlsv1.2 --retry 5 --retry-connrefused \
    --location --silent --show-error --fail --connect-timeout 15 --max-time 120 \
    "$url" -o "$root/private/raw/$id.txt"
  (
    ulimit -v 262144
    timeout --signal=TERM --kill-after=5s 3000 "$bin" seal "$manifest" \
      "$root/private/raw" "$root/private/work" "$id" "$root/evidence/$id-seal.tsv"
  )
  (
    ulimit -v 262144
    timeout --signal=TERM --kill-after=5s 3000 "$bin" test "$manifest" \
      "$root/private/raw" "$root/private/work" "$id" \
      "$root/evidence/$id-seal.tsv" "$root/evidence/$id-test.tsv"
  )
  rm -rf -- "$root/private/work/$id-store"
  rm -- "$root/private/raw/$id.txt"
  echo "TESTTEST_COMPLETE,id=$id,integrity=PASS"
done < "$repo/scripts/eng1c/testtest-roster.tsv"
if [[ "$language" == ALL ]]; then
  "$bin" analyze "$manifest" "$root/evidence" "$root/evidence/panel.tsv"
  cat "$root/evidence/panel.tsv"
fi
( cd "$root/evidence"; sha256sum ./*.tsv ./*.sha256 ./environment-*.txt > "manifest-$language.sha256" )
