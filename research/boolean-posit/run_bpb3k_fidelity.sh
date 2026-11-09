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
        for mode in stock fused fused_v2 stock_vector stock_ref fused_ref; do
          binary="$work/llama/build/bin/llama-debug"
          model="$experimental"
          kernel="$mode"
          tiled=1
          if [[ "$mode" == stock || "$mode" == stock_ref ]]; then binary="$work/stock/build/bin/llama-debug"; model="$work/original.gguf"; fi
          if [[ "$mode" == stock_ref || "$mode" == fused_ref ]]; then tiled=0; fi
          if [[ "$mode" == fused_ref ]]; then kernel=fused; fi
          # --save-logits disables upstream's verbose tensor-evaluation callback;
          # our observer writes its dump before the example's normal save path.
          GGML_CPU_TILED_MM="$tiled" MEM_BPB3K_KERNEL="$kernel" MEM_BPB3K_FIDELITY_DUMP="$prefix-$mode.bin" \
            timeout -k 10s 180s "$binary" -m "$model" -ngl 0 -t "$threads" \
              -c "$context" -b 512 -ub 512 -n 8 -p "$prompt" --save-logits \
              > "$prefix-$mode.log" 2>&1
          grep -q PASS_BPB3K_FIDELITY_DUMP "$prefix-$mode.log"
          if [[ "$mode" != stock && "$mode" != stock_ref ]]; then
            expected=Fused
            if [[ "$mode" == fused_v2 ]]; then expected=FusedV2; fi
            if [[ "$mode" == stock_vector ]]; then expected=StockVector; fi
            grep -q "BPB3K_DIRECT_DOT_EXECUTED kernel=$expected rows=" "$prefix-$mode.log"
            grep -q 'avx2=true' "$prefix-$mode.log"
            if [[ "$mode" == stock_vector ]]; then grep -q 'fma=true' "$prefix-$mode.log"; fi
          fi
          echo "BPB3K_FIDELITY_CASE threads=$threads context=$context prompt=$case_id repeat=$repeat mode=$mode"
        done
        echo "BPB3K_PRIMARY_COMPARISON=$prefix v1_v2=exact stock_v2=frozen_tolerance"
        "$work/bpb3k-fidelity" compare "$prefix-fused.bin" "$prefix-fused_v2.bin" exact || failed=1
        "$work/bpb3k-fidelity" compare "$prefix-stock.bin" "$prefix-fused_v2.bin" stock || failed=1
        echo "BPB3K_STOCK_VECTOR_DIAGNOSTIC=$prefix boundary=not_a_qualification_candidate"
        if ! "$work/bpb3k-fidelity" compare "$prefix-stock.bin" "$prefix-stock_vector.bin" stock; then
          echo BPB3K_DIAGNOSTIC_STOCK_VECTOR_DISAGREEMENT
        fi
        # Additional diagnostic controls only. They never replace the optimized
        # stock baseline or alter the two primary qualification criteria above.
        echo "BPB3K_REFERENCE_DIAGNOSTIC=$prefix boundary=not_a_performance_or_qualification_baseline"
        if ! "$work/bpb3k-fidelity" compare "$prefix-stock.bin" "$prefix-stock_ref.bin" stock; then
          echo BPB3K_DIAGNOSTIC_STOCK_TILED_VS_VECTOR_DISAGREEMENT
        fi
        if ! "$work/bpb3k-fidelity" compare "$prefix-stock_ref.bin" "$prefix-fused_ref.bin" stock; then
          echo BPB3K_DIAGNOSTIC_VECTOR_STOCK_VS_BPB_DISAGREEMENT
        fi
        if [[ "$repeat" == 2 ]]; then
          for mode in stock fused fused_v2 stock_vector stock_ref fused_ref; do
            "$work/bpb3k-fidelity" compare "${prefix/-r2/-r1}-$mode.bin" "$prefix-$mode.bin" exact || failed=1
          done
        fi
      done
    done
  done
done
(( failed == 0 )) || { echo STOP_BPB3K_MODEL_FIDELITY; exit 1; }
echo PASS_BPB3K_MULTIPROMPT_ALL_LOGITS_AND_TOKENS
