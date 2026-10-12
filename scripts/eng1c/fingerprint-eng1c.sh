#!/usr/bin/env bash
# ENG-1c Stage 0: bounded, public HTTPS fingerprint only; NEVER run a model.
# Source choices and normalization were preregistered before this script.
set -euo pipefail
export LC_ALL=C

if [[ "$#" != 2 ]]; then
  echo "usage: fingerprint-eng1c.sh fr|en <output-report-path>" >&2
  exit 2
fi

case "$1" in
  fr)
    language=FR
    book=17989
    expected_title='Title: Le comte de Monte-Cristo, Tome I'
    expected_language='Language: French'
    ;;
  en)
    language=EN
    book=1342
    expected_title='Title: Pride and Prejudice'
    expected_language='Language: English'
    ;;
  *)
    echo "ENG1C_INVALID_LANGUAGE" >&2
    exit 2
    ;;
esac

out="$2"
if [[ -z "$out" || -e "$out" ]]; then
  echo "ENG1C_REPORT_MUST_BE_NEW" >&2
  exit 2
fi
url="https://www.gutenberg.org/cache/epub/$book/pg$book.txt"

tmp="$(mktemp -d "${RUNNER_TEMP:-/tmp}/eng1c-fingerprint.XXXXXXXX")"
cleanup() { rm -rf -- "$tmp"; }
trap cleanup EXIT

curl --proto '=https' --tlsv1.2 --retry 5 --retry-connrefused \
  --location --silent --show-error --fail \
  --connect-timeout 15 --max-time 120 \
  "$url" -o "$tmp/raw.txt"

grep -Fxq "$expected_title" "$tmp/raw.txt"
grep -Fxq "$expected_language" "$tmp/raw.txt"
tr -d '\r' < "$tmp/raw.txt" > "$tmp/lf.txt"

start_re='^\*\*\* START OF THE PROJECT GUTENBERG EBOOK '
end_re='^\*\*\* END OF THE PROJECT GUTENBERG EBOOK '
start_count="$(grep -Ec "$start_re" "$tmp/lf.txt")"
end_count="$(grep -Ec "$end_re" "$tmp/lf.txt")"
if [[ "$start_count" != 1 || "$end_count" != 1 ]]; then
  echo "ENG1C_MARKER_COUNT_FAILURE,book=$book,start=$start_count,end=$end_count" >&2
  exit 1
fi
start_line="$(grep -En "$start_re" "$tmp/lf.txt" | cut -d: -f1)"
end_line="$(grep -En "$end_re" "$tmp/lf.txt" | cut -d: -f1)"
if (( end_line <= start_line )); then
  echo "ENG1C_MARKER_ORDER_FAILURE" >&2
  exit 1
fi
awk '
  /^\*\*\* START OF THE PROJECT GUTENBERG EBOOK / { keep=1; next }
  /^\*\*\* END OF THE PROJECT GUTENBERG EBOOK / { exit }
  keep { print }
' "$tmp/lf.txt" > "$tmp/body.txt"

n="$(wc -c < "$tmp/body.txt" | tr -d '[:space:]')"
if [[ ! "$n" =~ ^[0-9]+$ ]] || (( n < 100000 || n > 10000000 )); then
  echo "ENG1C_BODY_SIZE_FAILURE,book=$book,bytes=$n" >&2
  exit 1
fi
raw_sha="$(sha256sum "$tmp/raw.txt" | cut -d' ' -f1)"
body_sha="$(sha256sum "$tmp/body.txt" | cut -d' ' -f1)"
raw_bytes="$(wc -c < "$tmp/raw.txt" | tr -d '[:space:]')"

a=$((n * 8 / 10))
b=$((n * 9 / 10))
train=$a
val=$((b - a))
test=$((n - b))
for split in "$train" "$val" "$test"; do
  if (( split / 64 < 2 )); then
    echo "ENG1C_SPLIT_TOO_SHORT,book=$book" >&2
    exit 1
  fi
done

{
  printf 'ENG1C_FINGERPRINT,language=%s,ebook=%s,raw_bytes=%s,raw_sha256=%s,body_bytes=%s,body_sha256=%s,train_bytes=%s,val_bytes=%s,test_bytes=%s,train_events=%s,val_events=%s,test_events=%s,train_residual=%s,val_residual=%s,test_residual=%s,source_url=%s,integrity=PASS\n' \
    "$language" "$book" "$raw_bytes" "$raw_sha" "$n" "$body_sha" \
    "$train" "$val" "$test" \
    "$((64 * (train / 64 - 1)))" "$((64 * (val / 64 - 1)))" "$((64 * (test / 64 - 1)))" \
    "$((train % 64))" "$((val % 64))" "$((test % 64))" "$url"
} > "$out"
cat "$out"
