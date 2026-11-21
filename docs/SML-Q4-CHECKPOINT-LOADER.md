# SML Q4 checkpoint — self-describing, fail-closed loader

`sml_core::q4_checkpoint::Q4Checkpoint::open_verified(dir)` opens an output
directory of `sml-sparse-gradient-stream-fit` (`SML50M_STREAM_V1`) or of the
S2a eight-epoch continuation (`SML50M_STREAM_CONTINUE_V1`) without the caller
having to know page count, cache size or factorization. A raw `weights.q4`
has no header; those values are taken from the sealed `snapshot.tsv` and
`metrics.tsv` instead of being re-typed by hand.

## Checks (any failure returns an error; nothing is written)

1. Directory and `COMPLETE`, `metrics.tsv`, `snapshot.tsv`, `weights.q4` are
   regular, non-symlink entries; any `*.tmp` file (interrupted writer) fails.
2. `COMPLETE` equals `<schema>\t<sha256(metrics.tsv)>\n` for one of the two
   schemas (COMPLETE is written last by the trainer).
3. `metrics.tsv` is exactly the trainer's header plus one row (24 or 31
   columns), `test=UNOPENED`, `publication=NOT_PROMOTED`.
4. `snapshot.tsv` is one 10-field record; its version gives the
   factorization, which must equal the `factorization` column of metrics.
5. `capacity_slots`, `master_bytes`, `q4_bytes` follow from the snapshot page
   count; `learned_unique` / `parameter_updates` equal the snapshot counters.
6. `weights.q4` length is `pages × 512` and its SHA-256 equals `q4_sha256`;
   `snapshot.tsv` SHA-256 equals `snapshot_sha256`.

`open_verified_pinned(dir, sha)` additionally requires a pre-registered Q4
SHA-256 (for S2a: `3b2a0ea0f665d14afd6867e3cdec8c01fef10a426435d74f33ad0810a7cd427c`).
`open_atlas()` re-hashes `weights.q4` before returning a
`QuantizedSparseAtlas` with the recorded config and factorization.

Metric values (`dev_f32_bpc`, `dev_q4_bpc`) are returned as the verbatim
strings of `metrics.tsv`; the loader never recomputes or rounds them.
`master.f32` is not read: this is an inference loader, not a training resume
path (resume stays in the stream-fit CLI, which is unchanged).

Tests: `cargo test --locked -p sml-core --test q4_checkpoint` (synthetic
stream-fit-shaped outputs only; no Dell artifact is needed or claimed).
