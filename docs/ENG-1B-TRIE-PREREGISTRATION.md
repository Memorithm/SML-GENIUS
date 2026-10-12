# ENG-1b — lossless reversed-suffix trie and memory/latency comparison

**Preregistered 2026-10-12 before any ENG-1b implementation, tests or performance runs.** 
Work tracking: [SML-GENIUS issue #526](https://github.com/Memorithm/SML-GENIUS/issues/526).
Code/test changes and real-data results must follow this original commit. The frozen
ENG-1a (PR #525, merge `bab562c6c9fe485f045c425e9db4d80e47e00a86`)
and ENG-0 references remain unmodified.

## Scientific question

Can a byte-wise reverse-suffix trie share exact context keys for depths 0..32
while reproducing the full 256-class Witten–Bell probabilities **bit for bit**
and reducing measured memory relative to ENG-1a's complete `Vec<u8>` keys?

The lossless representation is a hypothesis; lower key *payload* alone is not
proof of reduced allocated memory, cache traffic or lookup latency.
This does not train or authorize dynamic selection of N-gram depth.

## Exact fixed architecture

- Root node 0 holds depth-0 histogram. Every non-root node has one
  parent and an edge `(parent:u32, older_byte:u8) -> child:u32`.
  Nodes of depth d correspond bijectively to the **last d bytes** of a
  history in reverse traversal order (newest byte first). No hashing,
  lossy signatures, alternative grouping or merging of unequal suffixes.
- Each node stores its own TRAIN count total and an ordered `BTreeMap<u8,u32>`
  of every observed next byte and its exact frequency. Nodes are created
  before recording target counts; root is counted for every TRAIN event.
- TRAIN exactly matches `anf_char::ChunkLayout`: 64 independent contiguous
  lanes, one next-byte event for every t>=1. For each event, update root
  and all present suffix lengths through min(t,max_depth). The immutable
  fitted model is returned only if every check succeeds.
- Limits: `max_depth` 0..32; max TRAIN events <=500000; max nodes
  <=250000 including root; max edges <=250000 (exact tree edges);
  max distinct node/follower pairs <=500000; node/edge references fit
  `u32`; no unbounded transient history buffers.
  Reject 0 or over-ceiling limits, underlength corpus, counter overflow
  and any exceeded capacity; no caller-visible partial result.
- For a frozen requested depth r, identify the longest reachable
  reverse-trie path, then apply Witten–Bell **in increasing depth**
  order, exactly as ENG-1a:
  `P_r(y) = (n_{h,y} + T_h * P_(r-1)(y))/(N_h + T_h)`.
  A missing parent means all longer suffixes are missing, so a shorter
  distribution remains in effect. Each requested r remains explicit and
  may change model semantics; no "RAM-pressure exact depth downgrade".
- Use fixed [usize;33] query scratch, ordered follower iteration,
  and no new crate dependencies; forbid unsafe and proprietary NVIDIA SDKs.
- Report *logical* root+node total counter bytes, follower counter bytes,
  follower byte keys, u32 parent/child IDs, and byte edge labels separately
  from BTreeMap tree-node/allocator overhead and process RSS.
  The benchmark must not conflate these measurements.

## Frozen tests before benchmark

1. Deterministic 64-lane patterned TRAIN (40 bytes/lane) and unseen
   64-lane heldout: compare **every one of 256 f64 bit patterns** at
   all depths 0..32 against ENG-1a and at depths 0..7 against ENG-0
   and `char_baselines::NGramModel`; compare exact heldout bits/byte,
   argmax and token denominators at depths 0/4/7/12/16/24/32.
2. Histories `[0,0*11]` and `[1,0*11]` share the last 7 bytes
   but lead to different targets 200/201 on a 64-lane synthetic corpus.
   Depth-7 probabilities must be bit-identical; depth-12 predictions
   must separate the teachers (correct-class probability >0.9).
3. Distinct nested prefixes and permutations of TRAIN lane order
   preserve correct node/parent/label resolution and exact probability
   vectors. Frozen predict/score never mutates the trained trie.
4. For all 0..32 requested depths: finite, positive 256-way distribution
   and normalized sum within 1e-10. Training determinism, root event total
   and sum-of-node/pair counters match ENG-1a when both have budget.
5. Exercise rejection of depth >32, zero/oversized budgets, events, nodes,
   edges, follower pairs, too-short corpus and invalid query/profile depth.
   No reconfiguration of existing A6 or ENG code to make these pass.

## Predeclared *diagnostic* physical measurement

- Rust-only opt-in binary `sml-eng1b-resource-bench` compiled in release;
  exactly three **separate OS processes** on the same GitHub-hosted
  Ubuntu 24.04 runner, `trie`, `long` and `legacy` (ENG-0 max depth 7).
  No persistent/private runner; the legacy arm is a different-depth
  historical context only and cannot be directly ranked at depth 32.
- Training fixture: 64 lanes * 96 deterministic bytes each, fixed public
  generator `((lane*19 + t*17 + ((t/7)%5)*31) % 251)`; heldout uses
  the same deterministic generator with `lane+137`. All arms consume
  identical bytes; no human selection or heldout tuning.
- `trie` and `long` train max depth 32 and query max depth 32;
  `legacy` trains depth 7 and queries depth 7.
  Each process reports training wall duration, Linux `/proc/self/status`
  VmRSS after training and VmHWM peak (KiB), p50/p95 monotonic lookup
  latency (ns) from exactly **2048 timed distributions**, after **256
  untimed warmups**, chosen deterministically from heldout valid contexts,
  and 64-bit rolling checksum of the same 256 probability values.
  The checksum must agree byte-for-byte for trie and long.
  The timing uses one sequential same-runner sample and remains sensitive
  to hosted interference, CPU scheduling, DVFS and allocator variability.
  VmRSS/VmHWM include the whole process, not just count tables. Zero,
  unsupported or missing Linux proc values fail measurement rather than
  being substituted with logical counts. No claim about GPU/Thor hardware.
- Physical **promotion** beyond opt-in reference is barred unless
  both of these preregistered criteria hold for trie vs long:
  `VmHWM_trie <= 0.90 * VmHWM_long` **and**
  `p95_ns_trie <= 1.10 * p95_ns_long` on the complete same-runner
  measurement, with exact probability checksum matching. A single
  hosted run cannot prove generalized performance; require separate
  repeated/independent hardware runs before production promotion.
  **Do not fail correctness CI merely because the promotion gate is STOP.**
- Bounded diagnostic not a valid low-level malloc profiling method:
  per-allocation bytes, real disk traffic, cache faults, CPU cycles and
  cache misses remain unmeasured and must be explicitly reported as such.

## CI/merging/evidence

- Pin Rust 1.89, `cargo fmt --all -- --check`, `cargo check --locked
  -p sml-core --all-targets`, `cargo clippy --locked -p sml-core
  --all-targets -- -D warnings`, `cargo test --locked -p sml-core --lib
  elastic_ngram_trie`, plus the older ENG-1a and ENG-0 tests.
- Run the three-process benchmark on a GitHub-hosted runner and archive
  **unmodified** run logs as Actions artifacts. Parse only preregistered
  fields; provenance includes exact source SHA, CPU architecture and rustc
  version. No TEST-label-driven model selection.
- Mandatory full SML CI and dedicated ENG-1b CI must pass at final HEAD
  before merge. Merging an **opt-in negative result** is allowed; selecting
  the trie for production inference is not.
- Model semantics remain SML-GENIUS-owned. ElasticXxx generic resource
  authorization and physical actuation are separate issue
  [#244](https://github.com/Memorithm/ElasticXxx/issues/244).
  No live resource mutation or model-performance benefit is implied.
