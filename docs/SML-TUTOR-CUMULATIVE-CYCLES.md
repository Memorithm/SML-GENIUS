# Cumulative certified cycles — first admission boundary

The existing daemon stores each cycle separately. This slice introduces a native
engine-to-store publication API without importing opaque on-disk certificate
digests as authority. Only a fully consolidated live engine can reconstruct the
certificates from its exam reports, verify every original object/record, and
publish the entire batch in one transaction. The target rejects partial overlaps,
duplicate keys and changes to the certificate or lesson even when payloads match.
A complete exact replay is idempotent. Stale concurrent target versions fail and
can be retried by the caller. The ordinary store commit API is unchanged.

A math-only curriculum excludes the synthetic domain tags used for scheduler
qualification. A separate retention function replays every input of a prior
ANF function against the original deterministic mathematical oracle, reading
only the cumulative checkpoint; no relearning or teacher calls. The test
publishes two distinct six-lesson batches and re-examines all prior lessons,
checks immutable records, exact replay, no fresh certificate from a reopened
engine without live exam reports, and injected interruption before HEAD publish.

Scope: this is a library integration gate, not a daemon rollout. It does not
migrate previous archives, install systemd, link the V9 or lexical stores, or
assert general mathematical reasoning. Next wire publication and retained
curriculum identities into the supervisor with crash-consistent job receipts
and bounded audit/storage costs before long endurance. No production writes.
