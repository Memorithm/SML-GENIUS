# PCEQ-4 — Producer identity and revision binding (research-only)

Status: Rust reference with adversarial contract tests. No production runtime
promotion, no FP32 path, no exact Boolean ANF/PVP substitution.

## Threat model and motivation

The PCEQ-3 sparse codec intentionally never reads samples for blocks
marked `CertifiedZero`. A false zero assertion cannot be detected from
the missing data; a stale version of a once-true zero certificate would
silently corrupt results. PCEQ-4 adds explicit producer identity, epoch
tracking and fail-closed invalidation when a sampled revision mismatch
is observed. This is a **mitigation of stale-epoch failures**, not a
cryptographic or mathematical proof that any zero certificate is true.

A `VersionedBlockProducer` must supply:

- stable `producer_id() -> u128`,
- strictly monotonic `revision() -> u64` whenever inputs or bounds change,
- block envelopes `block_bound(block) -> ProducerBound`,
- a single-read source callback `sample(index) -> i16`.

The source is borrowed immutably, allowing a trusted implementation to
manage its own strictly-versioned metadata while consumers do not own
mutable raw activation buffers. The caller is responsible for a coherent
immutable version or equivalent synchronization of producer data.

## Enforcement boundaries

`quantize_sparse_versioned` captures identity and revision, then
validates revision before and after each block envelope and each sample.
If revision changes, the first error is retained and the entire
in-progress snapshot is discarded; no partial output can be returned.
The producer identity and revision are also checked immediately before
publication. Published `VersionedSparsePceqI16` contains the immutable
PCEQ-3 snapshot with its source ID and revision.

Important caveats:

1. If a producer lies about `CertifiedZero` while preserving its revision,
   no reader that skips the values can independently detect the lie. An
   explicit negative test demonstrates this.
2. An ABA revision (change 7 → 8 → 7) can evade sampling. The monotonically
   increasing revision guarantee is a **trusted producer contract**, not
   enforced by the quantizer.
3. Unsynchronized concurrent mutation violates the contract. Do not claim
   atomic snapshot isolation merely because the revision is checked.
4. A `Cell` check adds small **metadata** reads per callback; no claim of
   increased speed or fewer physical DRAM accesses is made.
5. PCEQ-4 still quantizes i16→i8 with loss, and active blocks may
   temporarily reread local quantized i8 scratch to append codes. This
   does not re-materialize a full unquantized tensor.

## Correctness gates

- Fixed source epoch: output, inactive bitmap and weighted sum equal
  PCEQ-3 oracle for empty, tail and whole blocks.
- Revision changes during block-envelope generation or within sample
  generation cause explicit errors, not partial published snapshots.
- Invalid bounds/block sizes propagate without silently bypassing
  PCEQ-3 validation.
- A deliberately false zero certificate is documented as undetectable
  under the given API: do not conflate stale-epoch detection with
  an authenticated or independently verified source guarantee.
- Required exact-head Rustfmt, Clippy and workspace test CI must pass.

## Promotion criteria

Before adoption into any SML model or cross-project SciRust primitive:
(i) define immutable source identity and strictly monotonic version
semantics owned by producer, (ii) independently evaluate the cost
of generating and refreshing summaries, (iii) use real SML activation or
Boolean gate workloads with preserved model semantics, (iv) measure
same-host latency and qualified physical DRAM traffic through RemoteOps,
(v) complete numerical/model parity and scientific review.

The PCEQ-3 native same-host benchmark run #42288718273 demonstrates
logical payload savings for synthetic certified-zero workloads but
does not prove actual DDR-read savings or model-level improvement.
