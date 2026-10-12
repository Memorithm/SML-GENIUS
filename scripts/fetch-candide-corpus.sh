#!/usr/bin/env bash
# Frozen independent real-byte corpus for E4; no model training occurs here.
# Gutenberg #4650: Voltaire, "Candide, ou l'optimisme" (French).
# Do not weaken source/body SHA256 guards to make a failed download pass.
set -euo pipefail
export LC_ALL=C

URL="https://www.gutenberg.org/cache/epub/4650/pg4650.txt"
SOURCE_SHA256="1cc10ab3268a76aa01bd3f9722ae2ae0e5e36b869a18b0bb0832b75bc0327806"
BODY_SHA256="48e04595f5fc681ef6c0498123180bce0125d8fbe2167f3bcaeef189b45bd4b9"
BODY_BYTES=210655
if [ "$#" -gt 0 ]; then
  OUT="$1"
else
  OUT="data/char/candide-4650-frozen.txt"
fi
mkdir -p "$(dirname "$OUT")"

RAW="$(mktemp)"
trap 'rm -f "$RAW"' EXIT
curl --proto '=https' --tlsv1.2 --retry 5 --retry-connrefused --location --silent --show-error --fail "$URL" -o "$RAW"

ACTUAL_SOURCE_SHA256="$(sha256sum "$RAW" | cut -d' ' -f1)"
if [ "$ACTUAL_SOURCE_SHA256" != "$SOURCE_SHA256" ]; then
  echo "A6-E4 upstream raw source checksum mismatch: $ACTUAL_SOURCE_SHA256" >&2
  exit 1
fi
if [ "$(grep -c '^\*\*\* START OF' "$RAW")" -ne 1 ] \
  || [ "$(grep -c '^\*\*\* END OF' "$RAW")" -ne 1 ]; then
  echo "A6-E4 Gutenberg body markers are missing or duplicated" >&2
  exit 1
fi

tr -d '\r' < "$RAW" |
  awk '/^\*\*\* START OF/{body=1;next} /^\*\*\* END OF/{body=0} body' > "$OUT"

ACTUAL_BODY_SHA256="$(sha256sum "$OUT" | cut -d' ' -f1)"
ACTUAL_BODY_BYTES="$(wc -c < "$OUT")"
if [ "$ACTUAL_BODY_SHA256" != "$BODY_SHA256" ] \
  || [ "$ACTUAL_BODY_BYTES" -ne "$BODY_BYTES" ]; then
  echo "A6-E4 normalized corpus mismatch: bytes=$ACTUAL_BODY_BYTES sha256=$ACTUAL_BODY_SHA256" >&2
  exit 1
fi
printf 'A6E4_CORPUS_FROZEN,normalized_bytes=%s,sha256=%s,raw_sha256=%s\n' \
  "$ACTUAL_BODY_BYTES" "$ACTUAL_BODY_SHA256" "$ACTUAL_SOURCE_SHA256"
