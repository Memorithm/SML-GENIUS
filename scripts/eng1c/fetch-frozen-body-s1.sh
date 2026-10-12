#!/usr/bin/env bash
# ENG-1c/S1 controlled private temporary corpus materialization.
# Strictly identical Gutenberg LF/body extraction as preregistered ENG-1c/0.
# SHA and byte size must match BOTH raw source and normalized body before
# releasing a body to the Rust VAL-only predictor.
set -euo pipefail
export LC_ALL=C

if [[ "$#" != 2 || -z "$2" || -e "$2" ]]; then
  echo "usage: fetch-frozen-body-s1.sh FR|EN <new-temporary-output-path>" >&2
  exit 2
fi

case "$1" in
  FR)
    book=17989
    expected_raw_bytes=784416
    expected_raw_sha='9ea7a8b127a9dfa36de0b0cf9c2e26293beea5c37a0fc988fb9308b63160f282'
    expected_body_bytes=747855
    expected_body_sha='24ebc9a0037fc57bf93bf3ac6788c5ca64a9a6bbaf92b9fdbdfe6ca0f4114dde'
    title='Title: Le comte de Monte-Cristo, Tome I'
    language='Language: French'
    ;;
  EN)
    book=1342
    expected_raw_bytes=772386
    expected_raw_sha='3f6bb9d6f78e0293b56acd4714dd68cb7d6d1d293402031ce9d5a216bcaf9d75'
    expected_body_bytes=737950
    expected_body_sha='e79303d04de9260489f9d5a2ad4aae5776680fcec484f2e732d6ded7411bf402'
    title='Title: Pride and Prejudice'
    language='Language: English'
    ;;
  *)
    echo "ENG1C_S1_INVALID_LANGUAGE" >&2
    exit 2
    ;;
esac

dest="$2"
base="${RUNNER_TEMP:-/tmp}"
tmp="$(mktemp -d "$base/eng1c-s1.XXXXXXXX")"
cleanup() { rm -rf -- "$tmp"; }
trap cleanup EXIT
url="https://www.gutenberg.org/cache/epub/$book/pg$book.txt"

curl --proto '=https' --tlsv1.2 --retry 5 --retry-connrefused \
  --location --silent --show-error --fail --connect-timeout 15 \
  --max-time 120 "$url" -o "$tmp/source"

raw_bytes="$(wc -c < "$tmp/source" | tr -d '[:space:]')"
raw_sha="$(sha256sum "$tmp/source" | cut -d' ' -f1)"
test "$raw_bytes" = "$expected_raw_bytes"
test "$raw_sha" = "$expected_raw_sha"

tr -d '\r' < "$tmp/source" > "$tmp/lf"
grep -Fxq "$title" "$tmp/lf"
grep -Fxq "$language" "$tmp/lf"

start_count="$(grep -Ec '^\*\*\* START OF THE PROJECT GUTENBERG EBOOK ' "$tmp/lf")"
end_count="$(grep -Ec '^\*\*\* END OF THE PROJECT GUTENBERG EBOOK ' "$tmp/lf")"
test "$start_count" = 1
test "$end_count" = 1
start_line="$(grep -En '^\*\*\* START OF THE PROJECT GUTENBERG EBOOK ' "$tmp/lf" | cut -d: -f1)"
end_line="$(grep -En '^\*\*\* END OF THE PROJECT GUTENBERG EBOOK ' "$tmp/lf" | cut -d: -f1)"
test "$end_line" -gt "$start_line"

awk '
  /^\*\*\* START OF THE PROJECT GUTENBERG EBOOK / { keep=1; next }
  /^\*\*\* END OF THE PROJECT GUTENBERG EBOOK / { exit }
  keep { print }
' "$tmp/lf" > "$tmp/body"

body_bytes="$(wc -c < "$tmp/body" | tr -d '[:space:]')"
body_sha="$(sha256sum "$tmp/body" | cut -d' ' -f1)"
if [[ "$body_bytes" != "$expected_body_bytes" || "$body_sha" != "$expected_body_sha" ]]; then
  echo "ENG1C_S1_PINNED_NORMALIZED_BODY_DRIFT,ebook=$book" >&2
  exit 1
fi

mv -- "$tmp/body" "$dest"
printf 'ENG1C_S1_BODY_ADMITTED,lang=%s,ebook=%s,body_bytes=%s,body_sha256=%s,integrity=PASS\n' \
  "$1" "$book" "$body_bytes" "$body_sha"
