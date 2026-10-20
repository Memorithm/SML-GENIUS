# SML-50M S0 — large-corpus document-stream training (frozen before implementation)

Date: 2026-10-20. Status: experimental protocol; **not** a trained conversational model and **not** a claim that Boolean training universally outruns classical models.

## Objective
Replace the 2-MiB hard stop of the existing SML sparse-gradient SEC prefix pilot with an **opt-in separate Rust executable** capable of training the existing real disk-backed 50,000,896-slot atlas from a bounded, source-verified, document-separated stream up to **16 GiB of TRAIN data**. Do not alter any historical pilot CLI, seeds or result. Default factorization is explicit MSB-tree or independent-bit; no secret/proprietary corpus downloaded or silently admitted by the model.

This is the first ingestion slice toward a reproducible SML-50M artifact, **not a new architecture**. Routing is still the frozen two-byte hash; no falsely inferred long-context, learned MOR, 50M distinct learned parameters, assistant behavior or 25B quality claim.

## Frozen input and access contract
A human-curated UTF-8 TSV manifest, with exactly `role<TAB>path<TAB>bytes<TAB>sha256<TAB>rights_review` header, is the only source of files. Roles: `TRAIN` followed by `DEV`; files within each role are unique relative paths (no `.`, `..`, absolute paths, control characters, symlinks or path escape). Only `OWNED_REVIEWED` or `PUBLIC_DOMAIN_REVIEWED` declared document rights; **the declared value is not itself independent legal evidence**. Source roster plus licensing evidence must be externally curated before execution. SEC G0 remains `NOT_ADMITTED`, the 2024 SEC TEST remains `UNOPENED`. No arbitrary network fetch. Hash source bytes and lengths independently **before passing a document to the learner**, reject corrupted files and duplicate bytes across all splits. Disjoint document hashes are necessary, not proof of independent authorship or no semantic overlap.

Each document must contain 128 to 2,097,152 bytes; no document span can cross into the next one. Maximum 100,000 rows, 16 GiB TRAIN, 256 MiB DEV, with **at least 64 MiB TRAIN and 4 MiB DEV** in production mode. Order is frozen in manifest. No training may observe a DEV byte or label; DEV is not used for parameter tuning or routing. No TEST is used.

## Frozen model and resource envelope
Use `sml_core::sparse_gradient_atlas` existing `SparseGradientAtlas::create_with_factorization`, `train_document_grouped(document, 4096)`, `evaluate_document_grouped`, `checkpoint` and real `export_q4`; keep 48,829 pages × 1,024 f32 slots = **50,000,896 addressable slots**, 200,003,584-byte master, 6,250,112-byte touched bitmap, 25,000,448-byte Q4 export; max **32 resident pages**. Initial learning rate exactly 0.15; no tuning on DEV. Explicit physical RAM, disk, CPU, I/O and runtime admission via RemoteOps is required before running >64 MiB. Implement streaming one document at a time (<=2 MiB plus bounded batch state), never mmap/load the 16-GiB corpus. No GPU toolkit requirement.

Output in a new directory: master, touched bitmap, hash-verified snapshot, Q4 export, provenance manifest SHA, complete row with actual consumed bytes/events, unique touched parameters, f32/Q4 DEV loss in bits/byte, train timing and throughput, and final completion marker only after all validations. A partially written output is **not** a resumable or released checkpoint; no background job is assumed. Existing `SparseGradientAtlas::reopen` provides integrity checks for completed snapshots, but interruption recovery is not certified by this slice. No raw data or model weights are uploaded to public CI artifacts.

## Engineering qualification
- Deterministic mini-roster test at reduced atlas size: real document updates, snapshot reopens, Q4 readonly DEV parity of target count, no train-on-DEV parameter touches, versioned complete status.
- Adversarial tests: path traversal and symlink escape, repeated file/bytes across TRAIN and DEV, bad SHA/size, illegal rights markers, disorder, empty or too-small splits, oversized files, existing output, invalid numeric caps; reject without reporting a successful model.
- Full workspace `cargo fmt --check`, `cargo check`, `clippy -D warnings`, tests and exact-head CI before merge. Historical `sml-sparse-gradient-fit` behavior must be byte-for-byte unaffected.
- Separate RemoteOps controller and exact pinned source for Dell first, then Thor only when resource budget admitted. No benchmark acceleration claim until matched ngram4/similar quality comparisons include *all* costs; training rate alone is not model usefulness.

## Next slices (not claimed as accomplished by S0)
S1: dataset rights attestations and independent author/split manifest; first >=64 MiB physical training, >=4 MiB DEV, SHA/artifact preservation.
S2: reloadable byte-level Q4 causal inference CLI with deterministic replay/generation and quantization parity.
S3: coherent recurrent/SBG/MOR learned integration, matched ngram4 and conventional recurrent baselines at matched resources, robust out-of-domain texts.
S4: audited genuinely learned parameter scaling, larger controlled corpora (512 MiB / 1 GiB / 4 GiB) with bounded checkpoint/resume epochs and incremental validation, only after S1/S2.
