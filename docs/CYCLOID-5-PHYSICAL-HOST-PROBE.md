# CYCLOID-5 — bounded synthetic physical host-access probe

Status: Rust experimental CLI; produces **real host-call observations**
on whichever host runs it, not a model-inference or DRAM-bandwidth claim.
Requires CYCLOID-4's balanced paired-read API.

Executable: \`sml-runtime/src/bin/sml-cycloid-physical-probe.rs\`.

## Executable work

The CLI constructs an in-memory deterministic pseudo-random Boolean
weight-page payload (64–1024 KiB), serializes it with the **native**
SMLWGT01 writer, writes exactly one temporary file under \`RUNNER_TEMP\`
(or the platform temp directory), and stages an identical page into
the **actual committed** \`ElasticRamRuntime\`.

It builds one native MOR-selected page identity and executes 3–16
counterbalanced file→RAM / RAM→file pairs through the CYCLOID-4
physical read functions. Every pair revalidates the SML artifact
header/CRC, resident CRC and byte-exact equality; all samples must
preserve the page/precision/version/length identity.

With bounded memory, no model weights, no network, no root access and
no GPU dependencies, the tool summarizes independent columns for
\`file_median_ns\`, \`file_p95_ns\`, \`resident_median_ns\` and
\`resident_p95_ns\`; includes sample cardinalities, byte counts and
explicit negative model/DRAM claim flags. It fails on invalid inputs
or integrity mismatches and removes its temporary directory on normal
exit.

## Invocation on already reviewed source

\`\`\`sh
cargo run --locked --offline --release -p sml-runtime \
  --bin sml-cycloid-physical-probe -- 4 256
\`\`\`

On the Dell T430, the **only authorized remote host route** is the
on-demand \`Memorithm/RemoteOps\` workflow
\`.github/workflows/sml-cycloid-physical-dell.yml\`.
It admits only a full SHA already reachable from SML main, an
authorized dispatch, a compatible and sufficiently unloaded
non-root runner, and bounded resource controls.

The RemoteOps workflow archives only the short JSON report, SHA-256
and non-secret run provenance. The unit tests check statistical
indexing, reproducible fixture bytes, and refusal of empty measurements.

## Explicit limits

- Pseudo-random bytes are **synthetic** and contain no learned model
  semantics. No quality parity or relative end-to-end model throughput
  is measured.
- \`std::time::Instant\` reports wall-clock durations of verified host
  calls; filesystem cache, dynamic scheduling and memory allocator
  behavior can influence them.
- Pairwise deterministic AB/BA balance reduces systematic read-order
  asymmetry but is not randomization; there are no independent
  hardware replication guarantees.
- The nominal \`artifact_bytes\` and payload copy bytes are logical
  counts, **not physical disk reads, page faults or DRAM bus events**.
- This is diagnostic groundwork. The choice of Cycloid routing cannot
  be enabled in inference until physical overhead, long-run cost,
  confidence intervals and model-quality gates are satisfied.
