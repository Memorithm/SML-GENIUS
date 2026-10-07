# PVP6 — SML-owned ANF fixture and exact task contract v1

Date: 2026-10-07. Status: correctness qualification, not timing or model promotion.

## Purpose and provenance

Freeze a common input/output artifact before cross-repository activation-plus-query comparisons.
The source is PB8 at SML commit `013741325d02cd35eabc0086383b1d050c7ab1e8`,
example `crates/sml-core/examples/pascal_boolean_pb8_gate_bank.rs`,
original blob `afdcea93eb2d0dc8689e6f1b35602d3c383ba0db`.
The existing generator, monomial semantics, schedules and PB8 benchmark remain unchanged.
This is a deterministic synthetic ANF reference aligned with the PB8 task shape,
not a bank exported from trained SML weights and not proof of learned utility.

## Logical contract

- Eleven Boolean input features, K=2048, G=16.
- Four coefficient banks with per-gate densities 4, 32, 256, 1024.
- The original xorshift seed and sorted unique monomial generation are retained.
- Gate g computes `f_g(x) = XOR(a_g[S] for S subseteq x)`.
- Coefficients and truth are gate-major packed u64, least significant bit first.
  Word w contains addresses 64w..64w+63. Each bank is 4096 logical bytes.
- Schedules: AFFINE_UNIFORM (405*i+17 modulo 2048), LOW_POPCOUNT
  (ascending popcount then address), HIGH_POPCOUNT (descending popcount then address).
- Q=32,128,512,2048 selects a prefix. R=1,4,16 repeats that prefix
  with unchanged coefficients; R is reuse, not recurrent model depth.
- Task output has exactly Q*R u64 words, index repeat*Q+position.
  Bit g is gate g's value at that query. Bits 16..63 must be zero.
  Byte exchange, when needed, uses little-endian u64.
- The full vector and its length are compared; a checksum alone cannot qualify correctness.

## Canonical fixture CSV

UTF-8, LF, no BOM, no spaces, one trailing LF. Record order is fixed:

1. Header: `PVP6_FIXTURE,1,K,2048,G,16,word_bits,64,bit_order,lsb0`.
2. BANK,density,gate,word,coefficient_hex,truth_hex: density order as above,
   gate 0..15, word 0..31; 2048 rows. Hex words are lowercase, exactly 16 digits.
3. For each schedule in the listed order: QUERY,schedule,position,address (2048 rows),
   then OUTPUT,density,schedule,position,word_hex for each density (8192 rows).
4. END,banks,4,bank_words,2048,query_rows,6144,output_rows,24576.

There are 32770 lines including header/trailer. Consumers must reject missing,
duplicate, reordered, out-of-range or unexpected records, invalid hex/padding and
unmatched fixture SHA-256. They must reconstruct coefficients and independently
verify truth and task outputs; the supplied truth is evidence, not their oracle.
The whole-file SHA-256 is frozen in `scripts/check-pvp6-fixture.sh`.
No physical layout (address-major bitplanes or packed7) changes these semantics.

## Qualification

`pvp6-fixture` exports all coefficients, truth words, schedules and unique full-prefix
query outputs. `pvp6-exact-task` verifies every word for all 144 density/schedule/Q/R
cells and five arms: Pascal materialization, sparse scan, submask query,
generic truth materialization, resident truth lookup.
The independent task oracle evaluates the original monomial list.
Candidate materialization occurs once per gate before its R repeats.
Resident lookup excludes activation and remains a separate lifecycle reference.

Run the repository validation script. It tests corruption, padding, length,
query uniqueness and coefficient-change invalidation, exports twice and requires
byte-identical fixture and exact-task evidence. Any mismatch exits nonzero.
CI also retains the frozen fixture hash; changes require a new contract/version
rather than quietly accepting a different dataset.

## Next timing gate — not executed by this contract

First implement and freeze a full-output timing endpoint that includes activation,
allocations and queries, verifies every measured vector outside the timer, and
retains raw samples and failures. Record source/compiler/device, order, repetitions,
independent processes, RAM/VRAM and time caps before measurements.
Compare the competent direct controls, not only generic materialization.
Separate unchanged-bank reuse from coefficient updates and resident lifecycle.
Native SIMD must consume the identical fixture; emulation is correctness only.
GPU must include declared conversion/upload/readback/query costs and controlled
RemoteOps admission before confirmatory gain. Thor/Dell results remain separate.
Do not reuse the frozen PB8 checksum benchmark as full-output PVP6 qualification.

## Boundaries

No router, SBG/MOR, decision head, Delta-KV or learned model semantics are changed.
No architecture winner, speed, training quality, tokens/s or attention-replacement
claim follows from this contract. Final SML stays self-sufficient; SciRust, FLAT and
NNIS are optional qualification partners. Rust CPU/WGPU only; the installed driver
is the only permitted vendor-specific software layer. The future harness is separate.
