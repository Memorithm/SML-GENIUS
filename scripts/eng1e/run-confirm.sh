#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C
if [[ $# -lt 2 || $# -gt 3 || -e "$1" || ( "$2" != FR && "$2" != EN && "$2" != ALL ) ]]; then
  echo 'usage: run-confirm.sh <NEW-root> FR|EN|ALL [FROZEN_RAW_CACHE]' >&2
  exit 2
fi
root="$1"
language="$2"
cache="${3:-}"
repo="$(git rev-parse --show-toplevel)"
bin="$repo/target/release/sml-eng1e-confirm"
test -x "$bin"
mkdir -p "$root/private/raw" "$root/evidence"
cleanup() { rm -rf -- "$root/private"; }
trap cleanup EXIT
{
  git rev-parse HEAD
  rustc --version
  uname -m
  sha256sum "$bin" "$repo/scripts/eng1e/confirm-roster.tsv" \
    "$repo/scripts/eng1e/confirm-manifest.tsv" "$repo/docs/ENG-1E-CONFIRMATION-PREREGISTRATION.md"
  echo 'ENG1E_TRAIN_BYTES=131072,262144,524288'
  echo 'ENG1E_PRIMARY_UNITS=AUTHORS_WITH_THREE_VOLUMES_BLOCKED'
  echo 'ENG1E_INTERIM_TEST_READOUT=DISABLED'
} > "$root/evidence/environment-$language.txt"
phase() {
  (
    ulimit -v 262144
    timeout --signal=TERM --kill-after=5s 3000 "$bin" "$@"
  )
}
while IFS=$'\t' read -r id lang genre author title url; do
  [[ "$id" == id ]] && continue
  if [[ "$language" != ALL && "$lang" != "$language" ]]; then continue; fi
  echo "ENG1E_AUTHOR_BEGIN,id=$id,language=$lang"
  if [[ -n "$cache" ]]; then
    cp -- "$cache/$id.txt" "$root/private/raw/$id.txt"
  else
    curl --proto '=https' --tlsv1.2 --retry 5 --retry-connrefused --location \
      --silent --show-error --fail --connect-timeout 15 --max-time 120 \
      "$url" -o "$root/private/raw/$id.txt"
  fi
  for size in 131072 262144 524288; do
    phase seal "$id" "$size" "$root/private/raw" "$root/private" \
      "$root/evidence/$id-$size-seal.tsv"
    echo "ENG1E_DEV_SEALED,id=$id,train_bytes=$size"
  done
  for size in 131072 262144 524288; do
    phase test "$id" "$size" "$root/private/raw" "$root/private" "$root/evidence" \
      "$root/evidence/$id-$size-test.tsv"
    # Completion only: no interim numerical TEST losses or choices are printed.
    echo "ENG1E_TEST_COMPLETE,id=$id,train_bytes=$size,integrity=PASS"
    rm -rf -- "$root/private/$id-$size-store"
  done
  rm -- "$root/private/raw/$id.txt"
  echo "ENG1E_AUTHOR_COMPLETE,id=$id,integrity=PASS"
done < "$repo/scripts/eng1e/confirm-roster.tsv"
if [[ "$language" == ALL ]]; then
  "$bin" analyze "$root/evidence" "$root/evidence/panel.tsv"
  cat "$root/evidence/panel.tsv"
fi
( cd "$root/evidence"; sha256sum ./*.tsv ./*.txt > "manifest-$language.sha256" )
