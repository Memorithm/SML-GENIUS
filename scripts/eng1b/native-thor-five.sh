#!/usr/bin/env bash
# ENG-1b/N1: report-only five-round native CPU measurement on trusted SML main.
# A negative resource comparison is a scientific STOP, not a CI failure.
set -euo pipefail
export LC_ALL=C

if [[ "$#" != 2 || ! -x "$1" || -z "$2" ]]; then
  echo "usage: native-thor-five.sh <frozen-benchmark-binary> <evidence-directory>" >&2
  exit 2
fi
binary="$1"
evidence="$2"
mkdir -p "$evidence"

expected_digest='5c4ad424180a35ae'
expected_events=6080
expected_contexts=96733
expected_followers=101212
max_rounds=5
passed=0

get_field() {
  local line="$1" field="$2" segment match='' occurrences=0
  local -a items
  IFS=',' read -r -a items <<< "$line"
  for segment in "${items[@]}"; do
    if [[ "$segment" == "$field="* ]]; then
      match="${segment#*=}"
      occurrences=$((occurrences + 1))
    fi
  done
  if [[ "$occurrences" != 1 || -z "$match" ]]; then
    echo "ENG1B_NATIVE_INVALID_FIELD=$field" >&2
    return 1
  fi
  printf '%s' "$match"
}

require_positive() {
  local label="$1" value="$2"
  if [[ ! "$value" =~ ^[0-9]+$ ]] || (( 10#$value < 1 )); then
    echo "ENG1B_NATIVE_BAD_METRIC=$label,value=$value" >&2
    return 1
  fi
}

require_equal() {
  local label="$1" got="$2" want="$3"
  if [[ "$got" != "$want" ]]; then
    echo "ENG1B_NATIVE_MISMATCH=$label,actual=$got,expected=$want" >&2
    return 1
  fi
}

measure() {
  local round="$1" arm="$2" logfile="$evidence/round${round}-${arm}.txt"
  "$binary" "$arm" | tee "$logfile" >&2
  local lines
  lines="$(grep -c '^ENG1B_MEASURE,' "$logfile")"
  require_equal "measure_rows" "$lines" "1"
  local record
  record="$(grep '^ENG1B_MEASURE,' "$logfile")"
  require_equal "arm" "$(get_field "$record" arm)" "${arm^^}"
  local depth=32
  if [[ "$arm" == "legacy" ]]; then depth=7; fi
  require_equal "depth" "$(get_field "$record" depth)" "$depth"
  require_equal "train_events" "$(get_field "$record" train_events)" "$expected_events"
  require_equal "samples" "$(get_field "$record" samples)" "2048"
  require_equal "warmup" "$(get_field "$record" warmup)" "256"
  require_equal "architecture" "$(get_field "$record" arch)" "aarch64"
  require_equal "integrity" "$(get_field "$record" integrity)" "PASS"
  for field in rss_training_kib vmhwm_kib train_us p50_ns p95_ns contexts followers logical_key_bytes logical_counter_bytes; do
    require_positive "$arm.$field" "$(get_field "$record" "$field")"
  done
  if [[ "$arm" != "legacy" ]]; then
    require_equal "contexts" "$(get_field "$record" contexts)" "$expected_contexts"
    require_equal "followers" "$(get_field "$record" followers)" "$expected_followers"
    require_equal "reference_digest" "$(get_field "$record" digest)" "$expected_digest"
  fi
  printf '%s\n' "$record"
}

printf 'round,trie_vmHWM_KiB,long_vmHWM_KiB,trie_p95_ns,long_p95_ns,gate\n' > "$evidence/rounds.csv"
for round in 1 2 3 4 5; do
  arms=(trie long legacy)
  if (( round % 2 == 0 )); then arms=(long trie legacy); fi
  for arm in "${arms[@]}"; do
    measure "$round" "$arm" > "$evidence/round${round}-${arm}.validated"
    cat "$evidence/round${round}-${arm}.validated"
  done

  trie="$(cat "$evidence/round${round}-trie.validated")"
  long="$(cat "$evidence/round${round}-long.validated")"
  require_equal "paired_prediction_digest" "$(get_field "$trie" digest)" "$(get_field "$long" digest)"
  require_equal "paired_contexts" "$(get_field "$trie" contexts)" "$(get_field "$long" contexts)"
  require_equal "paired_followers" "$(get_field "$trie" followers)" "$(get_field "$long" followers)"
  trie_peak="$(get_field "$trie" vmhwm_kib)"
  long_peak="$(get_field "$long" vmhwm_kib)"
  trie_p95="$(get_field "$trie" p95_ns)"
  long_p95="$(get_field "$long" p95_ns)"

  gate=STOP
  if awk -v tr="$trie_peak" -v lr="$long_peak" -v tp="$trie_p95" -v lp="$long_p95" \
    'BEGIN {exit !(tr*10 <= lr*9 && tp*10 <= lp*11)}'; then
    gate=PASS
    passed=$((passed + 1))
  fi
  printf '%s,%s,%s,%s,%s,%s\n' "$round" "$trie_peak" "$long_peak" "$trie_p95" "$long_p95" "$gate" \
    | tee -a "$evidence/rounds.csv"
  echo "ENG1B_THOR_ROUND,round=$round,gate=$gate,trie_peak_kib=$trie_peak,long_peak_kib=$long_peak,trie_p95_ns=$trie_p95,long_p95_ns=$long_p95"
done

if [[ "$passed" == "$max_rounds" ]]; then
  verdict='PASS_NATIVE_SYNTHETIC_DIAGNOSTIC_ONLY'
else
  verdict='STOP_RESOURCE_NO_PROMOTION'
fi
{
  echo "ENG1B_THOR_RESOURCE_VERDICT=$verdict"
  echo "ENG1B_THOR_COMPLETED_ROUNDS=$max_rounds"
  echo "ENG1B_THOR_RESOURCE_PASS_ROUNDS=$passed"
  echo "ENG1B_THOR_PRODUCTION_PROMOTION=NOT_AUTHORIZED"
  echo "ENG1B_THOR_REAL_LANGUAGE_QUALITY=NOT_MEASURED"
  echo "ENG1B_THOR_GPU_METRICS=NOT_MEASURED"
} | tee "$evidence/decision.txt"
