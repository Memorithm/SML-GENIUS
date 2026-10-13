#!/usr/bin/env bash
# ENG-1c/CAP1: copy only the immutably pinned TRAIN bytes into a temporary file.
# Zero model scoring, no authenticated access and no VAL/TEST output artifact.
set -euo pipefail
export LC_ALL=C

if [[ "$#" != 2 || -e "$2" ]]; then
  echo 'usage: fetch-cap1-train.sh FR|EN <new-temp-train-path>' >&2
  exit 2
fi
lang="$1"
output="$2"

case "$lang" in
  FR)
    book=17989
    expected_title='Title: Le comte de Monte-Cristo, Tome I'
    expected_language='Language: French'
    expected_raw_bytes=784416
    expected_raw_sha='9ea7a8b127a9dfa36de0b0cf9c2e26293beea5c37a0fc988fb9308b63160f282'
    expected_body_bytes=747855
    expected_body_sha='24ebc9a0037fc57bf93bf3ac6788c5ca64a9a6bbaf92b9fdbdfe6ca0f4114dde'
    expected_train_bytes=598284
    ;;
  EN)
    book=1342
    expected_title='Title: Pride and Prejudice'
    expected_language='Language: English'
    expected_raw_bytes=772386
    expected_raw_sha='3f6bb9d6f78e0293b56acd4714dd68cb7d6d1d293402031ce9d5a216bcaf9d75'
    expected_body_bytes=737950
    expected_body_sha='e79303d04de9260489f9d5a2ad4aae5776680fcec484f2e732d6ded7411bf402'
    expected_train_bytes=590360
    ;;
  *)
    echo 'ENG1C_CAP1_UNKNOWN_FROZEN_CORPUS' >&2
    exit 2
    ;;
esac

dir="$(mktemp -d "${RUNNER_TEMP:-/tmp}/eng1c-cap1.XXXXXXXX")"
trap 'rm -rf -- "$dir"' EXIT
url="https://www.gutenberg.org/cache/epub/$book/pg$book.txt"
curl --proto '=https' --tlsv1.2 --retry 5 --retry-connrefused \
  --location --silent --show-error --fail \
  --connect-timeout 15 --max-time 120 "$url" -o "$dir/raw"

raw_bytes="$(wc -c < "$dir/raw" | tr -d '[:space:]')"
raw_sha="$(sha256sum "$dir/raw" | cut -d' ' -f1)"
tr -d '\r' < "$dir/raw" > "$dir/lf"
grep -Fxq "$expected_title" "$dir/lf"
grep -Fxq "$expected_language" "$dir/lf"

start_re='^\*\*\* START OF THE PROJECT GUTENBERG EBOOK '
end_re='^\*\*\* END OF THE PROJECT GUTENBERG EBOOK '
test "$(grep -Ec "$start_re" "$dir/lf")" = 1
test "$(grep -Ec "$end_re" "$dir/lf")" = 1
start="$(grep -En "$start_re" "$dir/lf" | cut -d: -f1)"
end="$(grep -En "$end_re" "$dir/lf" | cut -d: -f1)"
test "$end" -gt "$start"

awk '
  /^\*\*\* START OF THE PROJECT GUTENBERG EBOOK / { keep=1; next }
  /^\*\*\* END OF THE PROJECT GUTENBERG EBOOK / { exit }
  keep { print }
' "$dir/lf" > "$dir/body"
body_bytes="$(wc -c < "$dir/body" | tr -d '[:space:]')"
body_sha="$(sha256sum "$dir/body" | cut -d' ' -f1)"

if [[ "$raw_bytes" != "$expected_raw_bytes"
   || "$raw_sha" != "$expected_raw_sha"
   || "$body_bytes" != "$expected_body_bytes"
   || "$body_sha" != "$expected_body_sha" ]]; then
  echo "ENG1C_CAP1_FROZEN_CORPUS_DRIFT,language=$lang,ebook=$book" >&2
  exit 1
fi

head -c "$expected_train_bytes" "$dir/body" > "$output"
test "$(wc -c < "$output" | tr -d '[:space:]')" = "$expected_train_bytes"
printf 'ENG1C_CAP1_SOURCE,language=%s,ebook=%s,raw_bytes=%s,raw_sha256=%s,body_bytes=%s,body_sha256=%s,train_bytes=%s,integrity=PASS\n' \
  "$lang" "$book" "$raw_bytes" "$raw_sha" "$body_bytes" "$body_sha" "$expected_train_bytes"
