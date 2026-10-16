# SML FR/EN assistant-target experiment — 2026-10-16

Goal: eventually exchange naturally in French and English, including context continuity, multi-turn follow-up and code switching. This is a scientific milestone **not yet achieved**.

## Scope of this implementation

The Rust `sml-bilingual-dialogue-pilot` uses the native 50,000,896-capacity paged sparse-gradient atlas. The document is tagged FR/EN and contains a user prompt followed by an assistant answer. Only answer bytes incur supervised SGD loss; user/role prefix bytes are visible to the scorer but are not used as targets.

The input TSV header must be `split<TAB>lang<TAB>tree_id<TAB>prompt<TAB>answer`. Files are SHA-256 checked, UTF-8 decoded, bounded (24MiB), free of split overlap by full conversation-tree ID, and each FR/EN TRAIN/DEV set must contain at least 8 pairs. The program will not overwrite any prior model artifact. Training occurs before DEV scoring. No TEST is opened.

## Provenance and rights gate

Candidate corpus: OpenAssistant/oasst1 at https://huggingface.co/datasets/OpenAssistant/oasst1 — dataset card lists Apache-2.0 and 35 languages, including FR/EN. See Köpf et al., OpenAssistant Conversations (2023), https://arxiv.org/abs/2304.07327. The dataset-level license does **not** prove the absence of PII or that every conversation passes full content rights review. Any unapproved material remains exploratory: `G0=NOT_ADMITTED`. Filtering excludes deleted, rejected, synthetic rows, bad language pairs and obvious PII-tagged rows. No raw turns may be published in CI logs or artifacts.

RemoteOps owns private Dell ingestion and bounded physical execution; SML-GENIUS owns model/training quality decisions. A RemoteOps pinned pilot is not production admission.

## Critical limitations

The existing sparse atlas routes by fixed hashes of only **two preceding bytes**. Despite physical gradient updates, it cannot learn the semantics of a full prompt, coherent long-context chat, dialogue history or bilingual instruction following. Do not equate 50M file capacity with a useful 50M language model and do not call this fluent conversation. The mandatory output is `conversation_ready=NO`, `TEST=UNOPENED` and `G0=NOT_ADMITTED`.

## Next scientific gates

Independent FR and EN validation losses (bits per *answer byte*) must be reported separately. Introduce a shuffled-user-prompt ablation at equal data/compute: if prompt changes do not alter answers, there is no evidence of conditioning. Progress toward coherent dialogue requires learned multi-byte/recurrent content state, token generation plus UTF-8 decoding, precise end-of-turn stopping, true role conditioning, multi-turn training, safe data approval and independent bilingual human-quality assessment. Keep baseline and model experiments disjoint; never tune on frozen TEST.

No model-readiness claim until a held-out French and English conversational evaluation succeeds.
