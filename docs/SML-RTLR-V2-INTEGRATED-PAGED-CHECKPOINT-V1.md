# RTLR-ELASTIC V2 — integrated paged MOR snapshot / verified recurrent reload v1

The earlier 50,000,896-slot expert experiments [Dell #39046541218](https://github.com/Memorithm/RemoteOps/actions/runs/39046541218) validated physical online RTLR-V2 and dormant SHA cells, but **their sidecars were not a complete restorable model**. This PR closes the read-only **checkpoint integrity / restoration** subtask of [issue #605](https://github.com/Memorithm/SML-GENIUS/issues/605); it is NOT a quality improvement or a trained 25B model.

## Checkpoint chain

Run `MorAnfRecurrent::checkpoint_rtlr_v2(root, &rtlr)` only after the final TRAIN document. It performs the **existing atlas checkpoint** (flush dirty expert pages, SHA-256 the entire physical `master.f32` and touched bitfield, plus counters stored in `snapshot.tsv`) and the existing `mor_anf_head.tsv` (trained MOR router gate), bound together by the preexisting `joint.sha256.tsv`. It then writes:

- `rtlr_v2_full.tsv`: canonical versioned exact IEEE-754-bit-value hex serialization of recurrent local weights, hidden states, eligibility sensitivities, bounded residual gate, all RTLR-ELASTIC config and V2 TRAIN policy hyperparameters, EWMA, warmup/grow/shrink counters, full training statistics, index-consecutive active cell topology and SHA256 digest for each parked cell.
- `rtlr_v2_joint.sha256.tsv`: hash of **the exact preexisting MOR+atlas joint manifest bytes** and hash of the exact complete RTLR body bytes.
- `rtlr_v2_dormant/cell-NNN.tsv`: existing per-cell exact f64 bitwise-hex saved weights with local SHA. The new full manifest enumerates their **entire file hashes** and rejects unknown extra files.

Use `MorAnfRecurrent::reopen_rtlr_v2(root, config)`: fail closed unless recurrent joint digest and entire body digest match, all schema/index/finite bounds and growth/prune conservation invariants hold, all parked cells are present and correct with no unknown entries, and the existing MOR/atlas loader separately checks full physical f32 master SHA, touched bitfield SHA, gate SHA, expected router/atlas configs and recurrent physical/update counts. No fallback to scalar RTRL-2 if any artifact is missing or invalid.

Pilot `--pilot-rtlr-v2-{1,8,16}` now checks that a final on-disk sealed model can **actually reload** and produces identical read-only inference loss/events on a deterministic independent probe. The full 50M physical master is not uploaded in CI artifacts and is locally retained on Dell under controlled access.

## Strong boundaries / remaining work

- Integrity uses local SHA-256 digests, which detect accidental or uncoordinated corruption, **not adversarial tampering by someone who can change all files and recalculate all hashes**. Add trusted signatures, trusted immutable root commitments and secure multi-tenant permissions before security-sensitive production use.
- Recurrent optimizer consists of online SGD with forward eligibility, policy EWMA and structural counters. We preserve the on-disk active recurrent state/eligibility in a finished-document snapshot and parked cell learned weights. However **no concurrency-safe atomic multi-file snapshot transaction** is implemented: crash between component writes leaves a partial checkpoint that fails closed rather than automatically recovering or rolling back.
- Current checkpoint is a **read-only inference and document-boundary reactivation snapshot**. Exact continuation across a fully independent training process needs a separate matched multi-document reproduction test with deterministic document boundaries, optimizer counters and distinct output roots, plus state admission/rollback policy. Avoid claiming bitwise training resumability solely from this initial inference roundtrip.
- V2 controller's 0.001 bits/byte per cell is a heuristic compute proxy; it is not actual CPU cost. Old SEC DEV is already inspected; no independent accuracy improvements are claimed. Keep RTRL-2 as the empirical accuracy baseline.
- G0 commercial rights `NOT_ADMITTED`, frozen 2024 TEST `UNOPENED`. 50M denotes addressable capacity (prior trials touched 376,429 distinct scalars), not fully trained weights; no trained 25B model claim.
