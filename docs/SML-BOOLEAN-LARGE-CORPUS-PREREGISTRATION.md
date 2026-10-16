# Rust Boolean large-corpus fit — preregistration (2026-10-16)

Status: **exploratory, no GRU-v2 admission, no TEST, no weight promotion**.

## Immutable source roots
- TRAIN: `Memorithm/SciRust`, branch `master`, SHA `68d544679df465d3b85539c80926ed2644fbbba1`.
- TRAIN: `Memorithm/TDI`, branch `main`, SHA `7fc1a25bb4b86b6880e521ef270bce6b0b77ebff`.
- TRAIN: `Memorithm/FLAT-ATTENTION`, SHA `4d95909dec22f8d16c8b5312d502325d584f93df`.
- TRAIN: `Memorithm/CCOS-Enterprise`, SHA `ac3e64b57aec8120ab98922c76afae4df48d6336`.
- DEV: `Memorithm/SML-GENIUS`, at the exact code SHA in the execution workflow and **not** a TRAIN source.

The repositories are Memorithm-controlled sources used under the owner's experimental authorization. This authorization is not an independent chain-of-title determination for individual third-party contributions. Do not extend this pilot to MDN, Gutenberg, Gallica, USPTO, Hugging Face or any unreviewed corpus; it does not override G0.

## Frozen model and recipe
- Language/runtime: Rust, `sml-core` executable `sml-boolean-large-corpus-fit`.
- Model: **8192 trainable Boolean context cells**, each storing two `u64` target counts. A context is (bit index 0..7, upper nibble of t-2, upper nibble of t-1, same-bit values in t-2/t-1). Frozen Laplace alpha = 1.
- Processing: all eligible UTF-8 `.rs` and `.md` source files under top-level `src/`, `crates/`, `docs/`, plus `README.md` and `BOOTSTRAP.md`, lexicographic relative path order. Exclude symlinks, hidden paths, tests, fixtures, vendor, generated, third-party, node_modules, target and other unrelated subtrees. Exclude files below 128 or above 2,097,152 bytes; SHA-256-deduplicate identical file content within each role.
- Hard limits: 50,000 files per role, 256 MiB per role. Require at least 8 MiB of **actual** TRAIN source bytes and 64 KiB DEV bytes; on failure, report no successful large-corpus fit.
- TRAIN updates only the counts; DEV is scored against the **frozen** table, without update. All input bytes are read from the exact pinned Git clones; TEST is never opened.
- Scores: DEV frozen conditional log loss in **bits/byte**; uniform 0.5-bit baseline 8 bpc. This is not a competitive independent held-out benchmark or evidence of 25B capability.
- Outputs: `weights.tsv` (all learned counts), `summary.tsv` (actual source/event size, frozen DEV metric), `provenance.tsv` (all paths, roles, raw SHA-256), logged SHA-256 of each output. Never upload raw corpus, credentials or checkout files.
- Executor: RemoteOps `dell-t430` self-hosted non-root, pinned Rust 1.89.0, GitHub Actions source SHA verified. CPU only and bounded by timeout/concurrency.

## Advancement gates
This is a true fitting of a *discrete Boolean conditional-probability bank*, not a recurrent GRU or latent gradient training run. The previous eight `LatentGate2` model and GRU-v1 remain distinct. No GRU-v2 promotion, G0 admission, model-25B claim or production deployment follows from a successful job. Any positive or negative DEV change must be reported with all attempted sample sizes and exact identities.

If the run proves useful, qualify independently a trainable recurrent/ANF memory model with checkpoint-and-resume, admission evidence and a newly frozen TEST before making broader claims.

## Pre-execution amendment — measured insufficient corpus

The first Dell attempt, [RemoteOps run #38905222687](https://github.com/Memorithm/RemoteOps/actions/runs/38905222687), compiled and passed binary unit tests but correctly stopped: SciRust + TDI eligible TRAIN bytes = **6,690,818**, below the frozen 8,388,608-byte minimum. This is not a trained checkpoint. Before attempting a new run, the TRAIN source family has been expanded, still at the same 8 MiB minimum, with two public Memorithm-controlled source repositories and immutable SHAs above. No failed threshold is retroactively reclassified as success. The expanded recipe must be executed only from a newly pinned model commit and an updated RemoteOps workflow.
