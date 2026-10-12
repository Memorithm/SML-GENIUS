#!/usr/bin/env bash
# Frozen independent real-byte corpus for E4; no model training occurs here.
# Gutenberg #14155: Gustave Flaubert, French "Madame Bovary" (French).
# Do not weaken source/body SHA256 guards to make a failed download pass.
set -euo pipefail
export LC_ALL=C

URL="https://www.gutenberg.org/cache/epub/14155/pg14155.txt"
SOURCE_SHA256="93f0a2259dbaf5fde6e7b96adc22346d52f80eb1e9319336c8381a045efe40cd"
BODY_SHA256="7aa8b5f8871e46d88d3715dcbfd4e8e46cc8ba89218b134454b0778df4942eb8"
BODY_BYTES=717674
if [ "$#" -gt 0 ]; then
  OUT="$1"
else
  OUT="data/char/flaubert-14155-frozen.txt"
fi
mkdir -p "$(dirname "$OUT")"

RAW="$(mktemp)"
trap 'rm -f "$RAW"' EXIT
curl --proto '=https' --tlsv1.2 --retry 5 --retry-connrefused --location --silent --show-error --fail "$URL" -o "$RAW"

ACTUAL_SOURCE_SHA256="$(sha256sum "$RAW" | cut -d' ' -f1)"
if [ "$ACTUAL_SOURCE_SHA256" != "$SOURCE_SHA256" ]; then
  echo "A6-E5 upstream raw source checksum mismatch: $ACTUAL_SOURCE_SHA256" >&2
  exit 1
fi
if [ "$(grep -c '^\*\*\* START OF' "$RAW")" -ne 1 ] \
  || [ "$(grep -c '^\*\*\* END OF' "$RAW")" -ne 1 ]; then
  echo "A6-E5 Gutenberg body markers are missing or duplicated" >&2
  exit 1
fi

tr -d '\r' < "$RAW" |
  awk '/^\*\*\* START OF/{body=1;next} /^\*\*\* END OF/{body=0} body' > "$OUT"

ACTUAL_BODY_SHA256="$(sha256sum "$OUT" | cut -d' ' -f1)"
ACTUAL_BODY_BYTES="$(wc -c < "$OUT")"
if [ "$ACTUAL_BODY_SHA256" != "$BODY_SHA256" ] \
  || [ "$ACTUAL_BODY_BYTES" -ne "$BODY_BYTES" ]; then
  echo "A6-E5 normalized corpus mismatch: bytes=$ACTUAL_BODY_BYTES sha256=$ACTUAL_BODY_SHA256" >&2
  exit 1
fi
printf 'A6E5_CORPUS_FROZEN,normalized_bytes=%s,sha256=%s,raw_sha256=%s\n' \
  "$ACTUAL_BODY_BYTES" "$ACTUAL_BODY_SHA256" "$ACTUAL_SOURCE_SHA256"
