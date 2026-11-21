# SML integrated loop — pre-registered next-byte evaluation with baselines

Date 2026-11-21. Protocol frozen **before any number of this evaluation
exists**. Engineering research only: no model promotion, no language-model
quality claim, no biological-realism claim. Author: Grok (assisting GPT6 on
the integrated SBG → MOR → memory → recurrence → generation loop).

Code: `crates/sml-core/src/next_byte_eval.rs` (trait, baselines, evaluator,
read-only manifest loader), `crates/sml-core/src/bin/sml-next-byte-eval.rs`
(read-only CLI), `crates/sml-core/tests/next_byte_eval.rs` (synthetic tests).
This PR adds no measured value; every number below is either a frozen input
hash or a value already published by an earlier run, cited with its source.

## Frozen inputs

| Item | Value | Source |
| --- | --- | --- |
| Document manifest SHA-256 | `739457d01202031d2aed2f79c209073a6c49c4cd1ff1e4be74564252b409a70e` | S1b/S2a frozen manifest (`docs/SML-50M-S2A-EIGHT-EPOCH-CONTINUATION-PREREGISTRATION.md`) |
| DEV roster | 583 documents, 16,318,912 bytes, 16,317,746 scored positions | S2a `metrics.tsv` (RemoteOps run 39504420134) |
| TRAIN roster | 3,506 documents, 81,314,949 bytes | same |
| Q4 atlas (arm S2a) | `weights.q4` SHA-256 `3b2a0ea0f665d14afd6867e3cdec8c01fef10a426435d74f33ad0810a7cd427c` | S2a `metrics.tsv` |
| Atlas config | 48,829 pages, 32 cached pages, MSB-tree factorization | stream-fit CLI constants |
| S2a published DEV Q4 | `4.007063135` bits/byte | S2a `metrics.tsv`, engine `b13217cadac12443b5aef00ec22bc72dd57759b1` |

Weights and corpus are host-local on the Dell
(`/home/remoteops/sml-artifacts/sml50m-hf500-cc-s2a-b13217cadac12443b5aef00ec22bc72dd57759b1/`);
nothing is published or copied off-host by this protocol.

**TEST stays unopened.** The loader accepts only `TRAIN` rows followed by
`DEV` rows; any other role aborts the run. DEV is never used for fitting,
tuning, early stopping or arm selection.

## Arms

All arms implement `NextBytePredictor` and are scored by the same
`NextByteEvaluator` on exactly the same positions: for every DEV document in
manifest order, every position `i >= 2`, predicting `doc[i]` from
`doc[..i]` (the S2a event definition, `len - 2` per document).

1. **S2a Q4 atlas** (`q4_atlas_s2a`): the frozen Q4 file through its public
   API only — `QuantizedSparseAtlas::probability` for the log-loss and
   `predict_next_byte` (exact whole-byte MAP) for top-1. Fixed hash routing,
   two-byte context; not a trained router.
2. **Order-2 counts** (`order2_dirichlet_backoff_train`): two previous bytes
   (same context as the atlas), fixed Dirichlet back-off with
   `beta = 1.0` chosen a priori and **not tuned**:
   `P2(c|ab) = (n(abc) + beta·P1(c|b)) / (n(ab·) + beta)`,
   `P1(c|b) = (n(bc) + beta·P0(c)) / (n(b·) + beta)`, `P0` = add-one unigram.
   Counts from TRAIN scored positions only.
3. **Unigram counts** (`unigram_add1_train`): add-one over 256 byte values,
   TRAIN scored positions only.
4. **Integrated loop** (slot, *no number until implemented*): GPT6's native
   SBG → MOR → memory → recurrence → generation loop wired to the trained
   atlas, implementing `NextBytePredictor`. It must honour the trait call
   contract (reset at `begin_document`, history grows by one byte per call,
   `top1` never sees the target, no learning from DEV). It is added as a new
   arm in a follow-up PR with its own frozen code SHA, before it is run.

Count baselines freeze at their first `begin_document`; fitting after that is
an error (leakage control).

## Metrics (all fixed now)

Per arm, over the 583 DEV documents:

- **bits/byte** = Σ −log2 P(target) / scored positions (primary).
- **Q4 / arm top-1 byte accuracy** = fraction of positions where the
  deterministic MAP byte (lowest byte on ties) equals the target.
- **top-1 stream SHA-256**: SHA-256 of the emitted top-1 bytes in order
  (exact reproducibility checksum).
- **latency**: wall-clock ns per scored byte, separately for the
  probability call and the top-1 call, measured inside the evaluator.
- **page loads**: Q4 pages read from disk during the arm (cold 32-page LRU;
  0 for in-RAM baselines). The grouped gate pass reports its own count.
- **exact input checksums**: manifest SHA-256, Q4 SHA-256, per-document
  SHA-256/length/UTF-8 verified on every read.

Single deterministic pass; there are no seeds. No confidence interval is
claimed from one pass. Comparisons between arms are descriptive only; a
superiority claim for the integrated loop requires that it beats **both**
count baselines and the S2a atlas on DEV bits/byte under this exact protocol,
and still is not a language-model-quality claim.

## Validity gates (run is INVALID if any fails)

1. Manifest SHA-256, DEV document count/bytes/positions and Q4 SHA-256 equal
   the frozen values above (pinned in code; `--mode pinned` refuses overrides).
2. **Reproduction gate:** before any arm is scored, the S2a atlas is scored on
   a cold cache through the exact S2a path
   (`evaluate_document_grouped`, batch 4096, summed over documents in manifest
   order) and the value printed with nine decimals must equal **`4.007063135`
   exactly**. On failure the CLI exits non-zero and prints no arm rows.
3. The trait-path bits/byte of the atlas arm must agree with the gate value
   within 1e-6 (only floating-point summation order differs).
4. Every arm scores exactly 16,317,746 positions; every distribution sums to 1
   within 1e-9 (post-scoring probe); all log2 probabilities finite and ≤ 0.

## Execution (follow-up, not in this PR)

A RemoteOps Dell workflow (exclusive Dell group, exact Git SHA check, no GPU
toolkit, bounded memory, read-only access to the S2a artifact directory and
the frozen corpus) runs:

```bash
cargo run --locked --release -p sml-core --bin sml-next-byte-eval -- \
  --mode pinned --data-root DATA_ROOT --manifest MANIFEST.tsv \
  --checkpoint /home/remoteops/sml-artifacts/sml50m-hf500-cc-s2a-b13217cadac12443b5aef00ec22bc72dd57759b1
```

**Execution amendment (2026-11-21, before any number of this evaluation
exists).** The Q4 arm is read through the fail-closed loader of #1016
(`Q4Checkpoint::open_verified_pinned`) instead of a bare `--q4` file: a
headerless `weights.q4` cannot prove its own factorization (audit F8). In
pinned mode the CLI refuses `--q4` and additionally requires the sealed S2a
directory to be the `SML50M_STREAM_CONTINUE_V1` continuation, scored on this
manifest and DEV roster (583 files, 16,317,746 events), with 48,829 pages,
32 cached pages, MSB-tree, and a recorded `dev_q4_bpc` of exactly
`4.007063135`. Arms, metrics, positions and validity gates are unchanged; the
reproduction gate still re-measures the value instead of trusting the record.
It prints a `CHECKPOINT_V1` line (metrics/snapshot/Q4 SHA-256) after the
`NEXT_BYTE_EVAL_V1` header. Tests: `crates/sml-core/tests/next_byte_eval_checkpoint.rs`.

It records stdout verbatim. Results are published as a separate results
document that keeps negative outcomes; promotion stays `NOT_PROMOTED`.
