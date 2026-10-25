# HYB H-1d + causal copy expert (COPY-1)

Status: **research-only, opt-in**. Protocol frozen before running any new real-text comparison. It does not revise H-1d/H-3 verdicts, replace the H-1d parameters, or introduce an attention Q/K/V pathway.

## Hypothesis

For repeated strings, a simple recurrent *copy expert* can recover a byte after a repeated recent suffix, complementing the frozen ternary H-1d neural head. The original H-1d head remains active; the copy distribution is **mixed with the H-1d-corrected probability**, not used to replace or retrain its logits.

## Candidate and causality

- Per document a new, bounded 256-byte ring is maintained **only from previously decoded bytes**.
- At the start of each new byte, search all previous positions within the ring for the **longest suffix match** of length 1–4, breaking suffix-length ties by retaining all candidate next-bytes from matching past occurrences. If no length-1 match exists, copy is inactive.
- For each bit decision 0–7, filter those candidate bytes by the bits of the **current byte already decoded at higher positions**. The outcome bit itself and all lower bits remain unread. With `ones` among `n` matching candidates, use Laplace smoothing `P_copy(bit=1) = (ones + 1)/(n+2)` expressed as an integer Q32 probability. If no candidates survive the decoded prefix, use H-1d alone.
- For copy strength `w` in eighths, mixed probability is `round(((8-w) * P_H1d + w * P_copy)/8)`; `w=0` must be **exactly the legacy H-1d stream**; defaults remain unchanged. Initial exploratory `w=2`; `w=8` is a diagnostic copy-only-when-available control.
- Matching and probability are frozen deterministic integer operations, no labels/targets, gradients, synthetic pseudo-future or floating-point branch changes. Inference resident storage is fixed and independent of total context length. O(256 × 4) worst-case comparisons per decoded byte; **no latency advantage is assumed**.
- Compression and decompression both advance the same ring when each current byte is complete. Document boundaries reset ring state. The first two bytes remain coded at 1/2 as in the H-3 native engine.

## Required qualification

1. New opt-in head constructor `HybridHead::single_with_copy` reuses the *same* frozen `BankHead`, `eighths`, P1 counts and arithmetic coder as `HybridHead::single`. Invalid copy weight is rejected. The existing constructors and recorded H-3 synthetic replay output must be byte-identical when the copy expert is unused.
2. Independent oracle recalculates candidate suffixes over `data[..position]` (not the ring), then all eight bit probabilities over the previous decoded prefix only. `engine_p1s == oracle_p1s` must hold for short, repeated, random and long documents.
3. `compress`–`decompress` must recover all input bytes exactly on synthetic repeated, random and boundary-length inputs. A test must show a true repeated-suffix copy activation and no activation for an unseen suffix.
4. Repeated-byte patterns and varied copy weights, `0`, `2`, `8`, must be tested. Weight zero needs byte-for-byte parity to legacy H-1d, not merely equal BPC. Reset and first-two-byte handling must be checked.
5. Experimental outputs report BPC, active fraction, copy matched-prefix rate and CPU timing before any promotion. Compare P1, frozen H-1d, frozen H-1d+copy, and frozen P1+copy, on the **same frozen data** and seed pair 11/23. The model may increase BPC; this is not silently dropped.

## Validation/selection and limits

- Benchmark the fixed initial `w=2` on a document-disjoint CAL-SEL panel first. If its BPC exceeds H-1d by more than 0.001 or its runtime exceeds 1.5×, mark `NO_GO_CAL`; do not open a fresh evaluation sample. No tuning of window, match length, smoothing or weight on sealed panels.
- Only if CAL qualifies, a separately licensed, source-pinned, SHA-verified never-before-read evaluation corpus may be opened exactly once and judged. Neither EVAL3 nor EVAL5 qualifies as fresh.
- A `GO` requires copy+H-1d container BPC ≤ H-1d minus 0.010 for both seeds, strict bit/probability oracle agreement, exact round trips, matching CPU x86/aarch64 replay and end-to-end latency ≤ 1.5× on matched equipment. Otherwise retain a negative result without retroactive selection.
- This provides **a mixture of two complementary experts** in a complete lossless engine. It does not demonstrate a stand-alone Boolean LLM outperforming a trained neural network, nor any speed claim.

Copyright and license remain those of the surrounding SML-GENIUS repository.
