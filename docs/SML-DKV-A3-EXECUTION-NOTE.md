# A3 execution-path disclosure

The prospective scientific/data contract is unchanged in
`SML-DKV-A3-BATCH10-PREREGISTRATION.md` at
`38664b871cde7cea7e507725ceff9a1ab4597ccd`.

A new task-specific RemoteOps workflow could not be created in this session.
No alternate remote dispatch, credential change or workflow bypass was used.
The batch instead commits ordinary Rust source and tests through GitHub and
uses the repository's existing, unmodified CI. Each required run is bound to
its PR head and must finish successfully before merge and the next PR.

This is an execution-path deviation from the planned RemoteOps task, not a
claim that a new RemoteOps task ran. Existing CI owns its runner selection,
build parallelism and timeout. The prospective two-build-job/20-minute task
settings are NOT claimed as enforced for these CI runs. No physical memory,
latency, resource-isolation or whole-host budget qualification follows.

The data/element bounds in the Rust APIs remain explicit and executable. The
new diagnostic uses only the declared synthetic Development panel. It does not
change A/A2, access new protected evaluations, run training, tune thresholds,
or replace latent values with source answers. Ordinary existing regression
checks remain enabled. A3 full diagnostic output is emitted by the dedicated
tests in the existing store-evidence CI step; run identities and replay hashes
will identify that actual execution instead of inventing a remote-run record.

Source integrity, scientific/context promotion and external-action authority
remain separate. Successful code integration does not qualify any of them.
