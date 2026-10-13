#!/usr/bin/env bash
# ENG-1c/VAL1: frozen independent Gutenberg TRAIN and VAL only.
# Validates entire source/body SHA-256, never creates/exports TEST.
set -euo pipefail
export LC_ALL=C

if [[ "$#" != 3 || -e "$2" || -e "$3" || "$2" == "$3" ]]; then
  echo 'usage: fetch-val1-splits.sh FR|EN <NEW-TRAIN> <NEW-VAL>' >&2
  exit 2
fi
lang="$1"
train="$2"
val="$3"
case "$lang" in
  FR)
    book=17989
    expected_title='Title: Le comte de Monte-Cristo, Tome I'
    expected_language='Language: French'
    raw_bytes=784416
    raw_sha='9ea7a8b127a9dfa36de0b0cf9c2e26293beea5c37a0fc988fb9308b63160f282'
    body_bytes=747855
    body_sha='24ebc9a0037fc57bf93bf3ac6788c5ca64a9a6bbaf92b9fdbdfe6ca0f4114dde'
    train_sha='ec538157302ad0d39668288c8109779b70492a93041d432f7f97b2dc96e535cd'
    train_size=598284
    val_size=74785
    ;;
  EN)
    book=1342
    expected_title='Title: Pride and Prejudice'
    expected_language='Language: English'
    raw_bytes=772386
    raw_sha='3f6bb9d6f78e0293b56acd4714dd68cb7d6d1d293402031ce9d5a216bcaf9d75'
    body_bytes=737950
    body_sha='e79303d04de9260489f9d5a2ad4aae5776680fcec484f2e732d6ded7411bf402'
    train_sha='43553238be6ec4a88e01a679a53381cd89aca8374d815e021e92317f71cb47d0'
    train_size=590360
    val_size=73795
    ;;
  *) echo 'ENG1C_VAL1_UNKNOWN_FROZEN_CORPUS' >&2; exit 2 ;;
esac

tmp="$(mktemp -d "${RUNNER_TEMP:-/tmp}/eng1c-val1.XXXXXXXX")"
trap 'rm -rf -- "$tmp"' EXIT
url="https://www.gutenberg.org/cache/epub/$book/pg$book.txt"
curl --proto '=https' --tlsv1.2 --retry 5 --retry-connrefused \
  --location --silent --show-error --fail \
  --connect-timeout 15 --max-time 120 "$url" -o "$tmp/raw"
received_bytes="$(wc -c < "$tmp/raw" | tr -d '[:space:]')"
received_sha="$(sha256sum "$tmp/raw" | cut -d' ' -f1)"

tr -d '\r' < "$tmp/raw" > "$tmp/lf"
grep -Fxq "$expected_title" "$tmp/lf"
grep -Fxq "$expected_language" "$tmp/lf"
start_re='^\*\*\* START OF THE PROJECT GUTENBERG EBOOK '
end_re='^\*\*\* END OF THE PROJECT GUTENBERG EBOOK '
test "$(grep -Ec "$start_re" "$tmp/lf")" = 1
test "$(grep -Ec "$end_re" "$tmp/lf")" = 1
start="$(grep -En "$start_re" "$tmp/lf" | cut -d: -f1)"
end="$(grep -En "$end_re" "$tmp/lf" | cut -d: -f1)"
test "$end" -gt "$start"
awk '
  /^\*\*\* START OF THE PROJECT GUTENBERG EBOOK / { keep=1; next }
  /^\*\*\* END OF THE PROJECT GUTENBERG EBOOK / { exit }
  keep { print }
' "$tmp/lf" > "$tmp/body"

normal_bytes="$(wc -c < "$tmp/body" | tr -d '[:space:]')"
normal_sha="$(sha256sum "$tmp/body" | cut -d' ' -f1)"
if [[ "$received_bytes" != "$raw_bytes" || "$received_sha" != "$raw_sha"
      || "$normal_bytes" != "$body_bytes" || "$normal_sha" != "$body_sha" ]]; then
  echo "ENG1C_VAL1_SOURCE_DRIFT,language=$lang" >&2
  exit 1
fi

a=$((body_bytes * 8 / 10))
b=$((body_bytes * 9 / 10))
if [[ "$a" -ne "$train_size" || "$((b - a))" -ne "$val_size" ]]; then
  echo 'ENG1C_VAL1_SPLIT_DRIFT' >&2
  exit 1
fi

head -c "$train_size" "$tmp/body" > "$train"
dd if="$tmp/body" of="$val" bs=65536 skip="$a" count="$val_size" iflag=skip_bytes,count_bytes status=none
test "$(wc -c < "$train" | tr -d '[:space:]')" = "$train_size"
test "$(wc -c < "$val" | tr -d '[:space:]')" = "$val_size"
actual_train_sha="$(sha256sum "$train" | cut -d' ' -f1)"
actual_val_sha="$(sha256sum "$val" | cut -d' ' -f1)"
if [[ "$actual_train_sha" != "$train_sha" || "$actual_val_sha" == "$train_sha" ]]; then
  echo 'ENG1C_VAL1_SPLIT_SHA_MISMATCH' >&2
  exit 1
fi
printf 'ENG1C_VAL1_SOURCE,language=%s,ebook=%s,raw_sha256=%s,body_sha256=%s,train_bytes=%s,train_sha256=%s,val_bytes=%s,val_sha256=%s,test_file=NOT_CREATED,integrity=PASS\n' \
  "$lang" "$book" "$raw_sha" "$body_sha" "$train_size" "$actual_train_sha" "$val_size" "$actual_val_sha"
