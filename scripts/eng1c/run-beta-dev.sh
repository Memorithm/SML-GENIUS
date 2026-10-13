#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C
if [[ $# != 2 || -e "$1" || ( "$2" != FR && "$2" != EN && "$2" != ALL ) ]]; then
  echo 'usage: run-beta-dev.sh <NEW-root> FR|EN|ALL' >&2
  exit 2
fi
root="$1"
language="$2"
repo="$(git rev-parse --show-toplevel)"
bin="$repo/target/release/sml-eng1c-beta-dev"
test -x "$bin"
mkdir -p "$root/private" "$root/evidence"
cleanup() { rm -rf -- "$root/private"; }
trap cleanup EXIT
{
  git rev-parse HEAD
  rustc --version
  uname -m
  sha256sum "$bin" "$repo/scripts/eng1c/testtest-manifest.tsv" \
    "$repo/docs/ENG-1C-BETA-DEV-GRID-PROTOCOL.md"
  echo 'BETA_DEV_SCOPE=EXPLORATORY_DEV_ONLY'
  echo 'BETA_DEV_BETAS=1,0.25,0.5,2,4,8'
  echo 'BETA_DEV_DEPTHS=4,7,12,32'
  echo 'BETA_DEV_TEST_SCORING=UNAVAILABLE'
} > "$root/evidence/environment-$language.txt"
while IFS=$'\t' read -r id lang genre author title url; do
  [[ "$id" == id ]] && continue
  if [[ "$language" != ALL && "$lang" != "$language" ]]; then continue; fi
  echo "BETA_DEV_BEGIN,id=$id,language=$lang"
  curl --proto '=https' --tlsv1.2 --retry 5 --retry-connrefused \
    --location --silent --show-error --fail --connect-timeout 15 --max-time 120 \
    "$url" -o "$root/private/raw"
  "$bin" prepare "$id" "$root/private/raw" "$root/private/TRAIN" "$root/private/DEV"
  (
    ulimit -v 262144
    timeout --signal=TERM --kill-after=5s 3000 "$bin" grid "$id" \
      "$root/private/TRAIN" "$root/private/DEV" "$root/private/store" \
      "$root/private/sort" "$root/evidence/$id-grid.txt"
  )
  test "$(grep -c '^BETA_DEV_POLICY,' "$root/evidence/$id-grid.txt")" -eq 25
  grep -Eq '^BETA_DEV_SELECTION,.*decision=EXPLORATORY_DEV_SELECTION_ONLY,test=UNREAD,integrity=PASS$' "$root/evidence/$id-grid.txt"
  rm -rf -- "$root/private/store"
  rm -- "$root/private/raw" "$root/private/TRAIN" "$root/private/DEV"
  echo "BETA_DEV_COMPLETE,id=$id,integrity=PASS"
done < "$repo/scripts/eng1c/testtest-roster.tsv"
if [[ "$language" == ALL ]]; then
  "$bin" summarize "$root/evidence" "$root/evidence/summary.tsv"
  cat "$root/evidence/summary.tsv"
fi
( cd "$root/evidence"; sha256sum ./*.txt > "manifest-$language.sha256" )
