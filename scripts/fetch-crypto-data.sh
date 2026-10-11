#!/usr/bin/env bash
# Fetches the A6 crypto benchmark data (not committed to the repository).
#
# Source: Binance public data dumps (https://data.binance.vision), spot
# monthly klines, interval 1h, symbols BTCUSDT and ETHUSDT, pinned range
# 2019-01 .. 2025-12 (84 months). No API key is used.
#
# Every monthly zip is checked against the SHA-256 published next to it
# (".CHECKSUM"). The klines are merged into one CSV per symbol with columns
# open_time_ms,open,high,low,close,volume. Binance switched spot timestamps
# from milliseconds to microseconds in 2025; they are normalised to ms. The
# merged files are checked against pinned SHA-256 values so every run uses
# identical bytes.
#
# Usage: scripts/fetch-crypto-data.sh [output-dir]   (default: data/crypto)
set -euo pipefail

OUT="${1:-data/crypto}"
BASE="https://data.binance.vision/data/spot/monthly/klines"
SYMBOLS=(BTCUSDT ETHUSDT)
FIRST_YEAR=2019
LAST_YEAR=2025
declare -A EXPECTED=(
  [BTCUSDT]="41c9fc96e7544734a478859003c98dafc855568811d75dfbafe32045c48e396d"
  [ETHUSDT]="5025660b7735fc163ad0d4d4e4557665e55056e0fb78ea0690893354e94cd025"
)

mkdir -p "$OUT"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
for symbol in "${SYMBOLS[@]}"; do
  merged="$OUT/$symbol-1h.csv"
  : > "$TMP/$symbol.rows"
  for year in $(seq "$FIRST_YEAR" "$LAST_YEAR"); do
    for month in 01 02 03 04 05 06 07 08 09 10 11 12; do
      name="$symbol-1h-$year-$month"
      curl -sSf "$BASE/$symbol/1h/$name.zip" -o "$TMP/$name.zip"
      curl -sSf "$BASE/$symbol/1h/$name.zip.CHECKSUM" -o "$TMP/$name.zip.CHECKSUM"
      (cd "$TMP" && sha256sum -c --quiet "$name.zip.CHECKSUM")
      unzip -p "$TMP/$name.zip" \
        | tr -d '\r' \
        | awk -F, '$1 ~ /^[0-9]+$/ { t = $1; if (length(t) > 13) t = substr(t, 1, 13); print t "," $2 "," $3 "," $4 "," $5 "," $6 }' \
        >> "$TMP/$symbol.rows"
      rm -f "$TMP/$name.zip" "$TMP/$name.zip.CHECKSUM"
    done
  done
  { echo "open_time_ms,open,high,low,close,volume"; sort -t, -k1,1n -u "$TMP/$symbol.rows"; } > "$merged"
  actual="$(sha256sum "$merged" | cut -d' ' -f1)"
  if [ "$actual" != "${EXPECTED[$symbol]}" ]; then
    echo "checksum mismatch for $merged: expected ${EXPECTED[$symbol]}, got $actual" >&2
    exit 1
  fi
  echo "$merged: $(($(wc -l < "$merged") - 1)) hourly bars, sha256 $actual"
done
