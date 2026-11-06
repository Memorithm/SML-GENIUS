# PCEQ-5 — Exact Boolean XOR zero certificate from immutable identity

**Status:** Research-only exact GF(2) bitplane operation in `sml-core`, not
a replacement for SML-GENIUS production PVP6, not a quantization codec, and
not a measured performance result.

## Distinction from PCEQ-0 through PCEQ-4

PCEQ-0..4 examine **lossy i16→i8** block quantization and zero-block
certificates supplied by a trusted producer. They cannot independently
establish that a `CertifiedZero` claim is truthful without reading its
source values. PCEQ-5 examines a completely different but more
SML-native **exact Boolean** theorem:

For any bitplane A over GF(2), `A XOR A = 0`, bit for bit. If both
operands name **the same immutable canonical snapshot**, pointer equality
between shared `Arc<CanonicalBitplane>` instances is sufficient to
conclude an all-zero result *without examining either operand's words*.

This does not generalize to `A XOR B` when two snapshots merely contain
equal values; they must be read or otherwise compared/proven equal.

## Closed immutable source and sparse output representation

- `CanonicalBitplane::from_words(logical_bits, words)` checks exact
  `ceil(logical_bits/64)` length and rejects nonzero bits outside the
  declared logical width. The source data and width are private and no
  mutable accessor is exposed.
- `xor_sparse_exact(&Arc<...>, &Arc<...>)` checks width equality.
  Shared `Arc` identity implies XOR-zero immediately: zero source
  payload words read, zero output payload words written.
- Distinct immutable operands require the ordinary exact wordwise XOR,
  reading each source word once in the code and emitting only nonzero
  output words. Output is a `u64` bitmap indexed per word, and a
  packed `Vec<u64>` of the nonzero XOR results.
- The decoder reconstructs the exact canonical word stream. No model
  weights or lossy rounding are involved.
- `source_word_reads` is a deterministic **algorithmic access counter**,
  not hardware cache misses, DDR bytes, instructions or energy.

## Correctness validation

1. Shared immutable pointer alias: exact output zero, without source
   word access, for empty, partially filled, and multiword bitplanes.
2. Two *distinct* allocations with equal values: exact zero output but
   all source words are read, because identity is not proven.
3. Bitwise reference oracle and commutativity over deterministic cases,
   including the 63/64 active-bitmap word boundary.
4. Reject wrong input lengths, dirty high tail bits and unequal widths.
5. Test sparse payload and reconstruction with nonzero words at indices
   63, 64 and 65.
6. All required exact-HEAD Rustfmt, `cargo check`, Clippy and tests
   pass before merge.

## Security and model semantics

`Arc::ptr_eq` is valid as an identity proof here because
`CanonicalBitplane` owns private immutable boxed words, not an
externally mutable borrowed buffer. If later replacing this type with
a mapped, remotely mutable, or unsynchronized `UnsafeCell`-like
storage, this proof **must be reconsidered**.

This is an algebraic optimization for an exact GF(2) XOR operation; it
is *not* a proof that any arbitrary ANF gate, subset-zeta transform,
linear attention kernel or entire SML model has zero output.
Do not splice it into PVP6 execution without a formal operation-aligned
equivalence check against the scalar model-owned oracle and a separate
performance qualification.

The current implementation uses a naive dense `Arc<[u64]>` input
snapshot for its non-self paths, and its output bitmap costs extra
space when no words are zero. Logical bytes are not physical DRAM bytes.

## Next benchmark protocol

A future RemoteOps campaign could compare within Dell and Thor:
(i) ordinary XOR into dense output; (ii) sparse XOR for distinct
operands; (iii) same-snapshot identity proof. It must benchmark an
**admitted fraction of actual self-XOR operations** in native Boolean
workloads, separately accounting for the cost of constructing and
retaining immutable snapshots. If the real PVP workload rarely
reuses precisely the same shared input object on both operands,
the identity fast path has little or no effect.

Do not claim a speedup, saved DDR traffic or model inference gain
until these controlled native and workload-representative tests exist.

## Frozen research microbenchmark (added before native runs)

`crates/sml-core/src/bin/sml-pceq5-bench.rs` compares two variants
over **the same prebuilt immutable inputs**:

- `dense_xor`: conventional exact wordwise XOR and dense vector output.
- `sparse_identity`: exact XOR with immutable shared-`Arc` identity
  fast path and sparse nonzero-word output.

Frozen case grid: widths 256, 4096 and 65536 bits; self-XOR selection
thresholds 0%, 25%, 50% and 100%, 32 operand pairs per case. The
deterministic self-selection rule is
`(index * 37 + 19) % 100 < same_percent`, so its threshold is **not
necessarily the exact observed self-XOR incidence** at 32 pairs.
Producer construction and immutable allocation occur **outside** the
timed region, unlike a full model workload. Three warmups and nine
paired timed samples, with alternating candidate order, report
median and nearest-rank p95. Exactly 24 CSV records per host.

Preflight compares every result word to the independently computed
dense XOR oracle, checks shared-identity zero-result and access
counts, and compares total popcounts. The reported
`algorithmic_source_word_reads` count is derived from the algorithm,
not from measured cache/DRAM events. `logical_bytes` is output payload
+ sparse bitmap only; it excludes temporary allocations and input
snapshots. The benchmark explicitly records `drambytes=UNMEASURED`.

Native qualification must only use trusted merged-main code and
RemoteOps source SHA pin, retain per-host evidence, and must not
pool Dell and Thor latency distributions. No claim of real-model
speedup or DDR traffic reduction is admissible from this microbenchmark.
