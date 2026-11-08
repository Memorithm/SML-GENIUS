#!/usr/bin/env bash
# Outside timed runs; all tokens and every logit retained for eight decode steps.
set -euo pipefail
work="$1"
experimental="$2"
mkdir -p "$work/fidelity"
failed=0
for threads in 2 8; do
  for context in 128 512; do
    for case_id in arithmetic french context; do
      case "$case_id" in
        arithmetic) prompt='In mathematics, two plus two equals';;
        french) prompt='La capitale de la France est';;
        context)
          prompt="$(python3 - "$context" <<'PY'
import sys
print('The blue book is on the wooden table. ' * (6 if sys.argv[1]=='128' else 36) + 'What color is the book?')
PY
)";;
      esac
      for repeat in 1 2; do
        prefix="$work/fidelity/t${threads}-c${context}-${case_id}-r${repeat}"
        for mode in stock fused fused_v2; do
          binary="$work/llama/build/bin/llama-debug"
          model="$experimental"
          if [[ "$mode" == stock ]]; then binary="$work/stock/build/bin/llama-debug"; model="$work/original.gguf"; fi
          MEM_BPB3K_KERNEL="$mode" MEM_BPB3K_FIDELITY_DUMP="$prefix-$mode.bin" \
            timeout -k 10s 180s "$binary" -m "$model" -ngl 0 -t "$threads" \
              -c "$context" -b 512 -ub 512 -n 8 -p "$prompt" \
              > "$prefix-$mode.log" 2>&1
          grep -q PASS_BPB3K_FIDELITY_DUMP "$prefix-$mode.log"
          if [[ "$mode" != stock ]]; then
            expected=Fused
            if [[ "$mode" == fused_v2 ]]; then expected=FusedV2; fi
            grep -q "BPB3K_DIRECT_DOT_EXECUTED kernel=$expected rows=" "$prefix-$mode.log"
            grep -q 'avx2=true' "$prefix-$mode.log"
          fi
          echo "BPB3K_FIDELITY_CASE threads=$threads context=$context prompt=$case_id repeat=$repeat mode=$mode"
        done
        "$work/bpb3k-fidelity" compare "$prefix-fused.bin" "$prefix-fused_v2.bin" exact || failed=1
        "$work/bpb3k-fidelity" compare "$prefix-stock.bin" "$prefix-fused_v2.bin" stock || failed=1
        if [[ "$repeat" == 2 ]]; then
          for mode in stock fused fused_v2; do
            "$work/bpb3k-fidelity" compare "${prefix/-r2/-r1}-$mode.bin" "$prefix-$mode.bin" exact || failed=1
          done
        fi
      done
    done
  done
done
(( failed == 0 )) || { echo STOP_BPB3K_MODEL_FIDELITY; exit 1; }
echo PASS_BPB3K_MULTIPROMPT_ALL_LOGITS_AND_TOKENS
