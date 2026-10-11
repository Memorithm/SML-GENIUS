#!/usr/bin/env bash
# Fetches the A5 step-3 character-level corpus (not committed to the repository).
#
# Source: Project Gutenberg eBook #800, Jules Verne, "Le tour du monde en
# quatre-vingts jours" (1873), French. Jules Verne died in 1905; the work is in
# the public domain. Only the book body between the "*** START OF" and
# "*** END OF" markers is kept (Project Gutenberg header, footer and license
# text are stripped), line endings are normalised to LF, and the result is
# checked against a pinned SHA-256 so every run uses identical bytes.
#
# Usage: scripts/fetch-char-corpus.sh [output-path]   (default: data/char/verne-tour-du-monde.txt)
set -euo pipefail

URL="https://www.gutenberg.org/cache/epub/800/pg800.txt"
EXPECTED_SHA256="b51f36fb2baf0847fd3632fbd6ff939b8858abc1d728b5f86c94d99e64766a74"
OUT="${1:-data/char/verne-tour-du-monde.txt}"

mkdir -p "$(dirname "$OUT")"
TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT
curl -sSfL "$URL" -o "$TMP"
tr -d '\r' < "$TMP" \
  | awk '/^\*\*\* START OF/{body=1; next} /^\*\*\* END OF/{body=0} body' > "$OUT"
ACTUAL="$(sha256sum "$OUT" | cut -d' ' -f1)"
if [ "$ACTUAL" != "$EXPECTED_SHA256" ]; then
  echo "checksum mismatch for $OUT: expected $EXPECTED_SHA256, got $ACTUAL" >&2
  exit 1
fi
echo "corpus ready: $OUT ($(wc -c < "$OUT") bytes, sha256 $ACTUAL)"
