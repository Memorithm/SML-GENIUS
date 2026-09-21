#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

echo "SBG-0.12 code-generation audit"
echo "arch=$(uname -m)"
rustc --version --verbose

rm -f target/release/deps/sml_core-*.s
cargo rustc -p sml-core --release --lib -- --emit=asm

mapfile -t asm_files < <(find target/release/deps -maxdepth 1 -type f -name 'sml_core-*.s' -print | sort)
if [[ "${#asm_files[@]}" -eq 0 ]]; then
  echo "error=no_sml_core_assembly"
  exit 1
fi

symbol="packed_word4_score_16384"
asm_file=""
line=""

for candidate in "${asm_files[@]}"; do
  match="$(grep -n -m1 "$symbol" "$candidate" || true)"
  if [[ -n "$match" ]]; then
    asm_file="$candidate"
    line="${match%%:*}"
    break
  fi
done

if [[ -z "$asm_file" || -z "$line" ]]; then
  echo "error=audit_symbol_not_found"
  printf 'assembly_files=%s\n' "${asm_files[*]}"
  exit 1
fi

snippet="$(mktemp)"
trap 'rm -f "$snippet"' EXIT

# Start at the first symbol-bearing assembly line and stop at the first LLVM
# function-end label. This keeps instruction counts scoped to the audited
# function instead of relying on an arbitrary context window.
tail -n +"$line" "$asm_file" | awk '
  { print }
  /\.Lfunc_end[0-9]+:/ { exit }
' > "$snippet"

if ! grep -Eq '\.Lfunc_end[0-9]+:' "$snippet"; then
  echo "error=audit_function_end_not_found"
  exit 1
fi

count_pattern() {
  local pattern="$1"
  local count
  count="$(grep -Eic "$pattern" "$snippet" || true)"
  printf '%s' "$count"
}

echo "assembly_file=$asm_file"
echo "symbol_line=$line"
echo "snippet_lines=$(wc -l < "$snippet" | tr -d ' ')"
echo "aarch64_cnt_mentions=$(count_pattern '(^|[[:space:]])cnt([[:space:]]|$)')"
echo "aarch64_addv_mentions=$(count_pattern '(^|[[:space:]])(addv|uaddlv|saddlv)([[:space:]]|$)')"
echo "aarch64_vector_load_mentions=$(count_pattern '(^|[[:space:]])(ld1|ldp[[:space:]]+q|ldr[[:space:]]+q)([[:space:]]|$)')"
echo "xor_mentions=$(count_pattern '(^|[[:space:]])(eor|xor)([[:space:]]|$)')"
echo "and_mentions=$(count_pattern '(^|[[:space:]])and([[:space:]]|$)')"
echo "vector_register_mentions=$(count_pattern '\bv[0-9]+\.')"

echo "snippet_begin"
cat "$snippet"
echo "snippet_end"
