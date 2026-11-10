# DACR-2G — transactional latent-address rebinding with epoch invalidation

**Preregistered 2026-11-09, before executable changes.** Source base: `Memorithm/SML-GENIUS` main at `9febddcb8048f6ba9f0f7912796d2c5db04aedd1`. Native Rust `sml-store::dacr_latent` only; independent of pending DACR-2C/D/E/F proposals. No default model behavior, source authentication, neural learning or physical-performance claim.

## Objective

The DACR-2A `CandidateIndex` currently supports immutable per-segment association after insertion. A model whose SBG representation changes needs a controlled way to **reassign a previously registered candidate segment from one discrete address to another**, without deleting or scanning the entire history and without continuing to accept stale epoch tokens.

Add `CandidateIndex::rebind(segment, expected_epoch, next_epoch, old_key, new_key)` and `CandidateIndex::epoch()`. This is a transactional *index maintenance* primitive; it does not itself learn the replacement address or validate the source content. Source reads still require DACR-2B/CONTEXT-3 verification.

## Frozen invariants

- `segment` must be nonzero, same stream, and already present in this index. `expected_epoch` must equal the current epoch; `next_epoch` must be exactly `expected_epoch + 1` with checked overflow and nonzero. Reject mismatches with explicit `WrongEpoch`.
- Both `old_key` and `new_key` must have known values on all selected SBG features, using unchanged `DiscreteAddress::encode` (unknown selected bit fails closed). The alleged `old_key` must address the bucket actually containing this source. If not, reject with `SourceAddressMismatch`.
- Before changing any bucket or the epoch, ensure the destination bucket has sufficient capacity when `new_address != old_address`. If the target is full, reject `BucketFull`, leaving source, bucket order and epoch **exactly unchanged**. If old and new addresses coincide, preserve the bucket order but advance the epoch.
- On success remove exactly that segment from its old bucket, append it to the destination bucket in insertion order, and **only then** publish the next epoch. The total number of unique entries and configured per-bucket/total caps stay unchanged. No source is duplicated or evicted, and no global scan occurs on lookup. A rebind is an explicit bounded maintenance operation, not a query-time scan; old-bucket search is limited to `max_bucket_entries`.
- A stale `lookup` with the previous epoch must reject immediately. A current-epoch `lookup` from the old address must no longer return the segment; the new address must return it. Both still return *unverified candidates only*. Other segment memberships remain unchanged.
- Keep all existing `CandidateIndex::new/insert/lookup` APIs and behavior unchanged. No new NVIDIA dependencies, no physical host intervention.

## Fixed tests

1. Two-source index, different addresses: successful rebind moves precisely one segment and increments epoch; old epoch rejects, old bucket misses, new bucket returns it. Verify unrelated source unchanged, cardinality unchanged, expected accounting and deterministic repeat with an independently built index.
2. Wrong stream, missing segment, wrong current epoch, skipped/unchanged next epoch, incorrect old address, unknown selected `old_key` or `new_key`: fail without modifying epoch or candidate lookups.
3. Destination bucket full: fail without mutation; same-address rebind remains an allowed epoch advance with stable order.
4. `u64::MAX` epoch cannot advance and rejects cleanly.
5. Rebind call does not authenticate content or imply model-quality improvement; no source status/result beyond candidate IDs.

Full workspace `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, and `cargo test --locked --workspace` required on exact final head before merge. No reuse of protected LONGCTX TEST; no benchmark claims. If CI is not scheduled or cannot execute, hold the PR and report that blocker.
