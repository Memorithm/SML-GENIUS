# PVP6 model-owned executor — destination qualification

Date: 2026-10-08. Scope: exact synthetic semantics, not latency or learned utility.

## Previous gate and defects exposed

PR #235 integrates the model-owned executor following selector PR #234. Its
packed-output unit test originally expected true for monomials [0,1,3,7] at
query 1. The constant and first-variable terms cancel in GF(2); false is correct.
The correction was merged only after exact-head CI succeeded.

The next qualification exposes a separate contract gap: the first executor
accepted a reuse coordinate but returned only Q query values. The frozen PVP6
contract requires Q*R values with unchanged coefficients. It also had no check
that the declared density matched every gate, and allowed callers to mutate
result shape metadata independently of the private packed buffer.

## Model-owned contract

`Pvp6Bank::evaluate(workload, queries)` accepts one prefix of Q addresses and
returns Q*R values for each gate. Query index is repeat*Q+position. Pascal
materialization occurs once per gate, before its reuse loop. Sparse and submask
paths evaluate the same repeated prefix. This is bank reuse, not recurrent depth.
Every gate must have the declared density; mixed or falsely labelled banks fail
with `DensityMismatch` before output allocation. Query count/range and zero
coordinates continue to fail closed.

Result dimensions and selection are read-only through accessors. Gate-major
words retain query bits least-significant first. `to_query_major_words()` returns
ceil(G/64) words per query; banks above 64 gates are not truncated. With G=16,
this is exactly the PVP6 v1 canonical output, including zero padding.
`query_count()` now includes reuse rounds; its input prefix still has Q entries.
Callers consuming the diagnostic API must use the accessors rather than public
fields and must not repeat the output themselves.

The selection's `measured_on_thor` flag identifies policy coordinates present in
the historical campaign. It does not assert that an arbitrary supplied bank,
query population, width, host or this new executor has been timed. All selections
remain diagnostic-only. No timing threshold is requalified by this test.

## Frozen-corpus destination check

New evidence mode: `pvp6-model-executor` in the existing PB8 example. It invokes
the public model API, not the old benchmark arm functions. It uses the unchanged
PVP6 generator and independent task oracle, checks all reconstructed coefficient
words, and compares both entire output layouts, their lengths and padding.

The existing fixture SHA-256 remains:
`feb1a144e23d9a29439b7cad6704f56637a7f7ead1a9b9abb39ad9c0ac5325fd`.
No old fixture, exact-task, plan or benchmark mode is changed.

Acceptance counts per complete execution:

- 144 cells: 4 densities * 3 populations * 4 query counts * 3 reuse counts;
- 2,048 coefficient words, 685,440 canonical query words and 171,456 packed words;
- 141 Pascal selections, 2 sparse selections and 1 submask selection;
- 20 historically measured coordinate tuples; 144 diagnostic-only decisions;
- zero differing words, including padding and all reuse rounds.

The mandatory `scripts/check-pvp6-fixture.sh` now executes this mode twice,
requires byte-identical reports and checks the exact counts above in addition to
its original frozen hash and five-arm exact-task checks. The pre-existing CI
step invokes this script on fresh hosted PR workers and on integrated main
through the managed self-hosted runner; it remains mandatory.

## Native workflow provenance repair

The previous native workflow automatically repeated timing on PB8 source changes,
while recording a fixed historical source string rather than the actual checkout
and invoking the benchmark without an explicit release profile. That job does
not qualify the timing of the new executor. The initial automatically queued run
for this PR was cancelled; no new timing evidence is admitted here.

Automatic functional admission stays in the existing mandatory CI (fresh hosted
PR execution, native integrated-main execution). The separate native workflow is
now a main-only manual correctness/admission job: exact checkout SHA verification,
Rust 1.89.0, isolated toolchain/build homes, locked release builds, full destination
qualification and the pinned SciRust fixture consumer. Artifacts include the
actual source and are labelled correctness-only. It cannot execute a PR ref or
quietly launch a timing campaign. No existing CI gate is disabled or made optional.
This manual workflow is not itself evidence of an executed native run; use the
observed CI/run receipts. Future timing needs a separately qualified, source-pinned
RemoteOps control and declared measurement protocol. Historical archives remain
unchanged; this repair does not retrospectively upgrade their provenance.

## Independent boundary checks

Seven executor unit tests cover the three selected arms; malformed gates and
batches; declared density and zero coordinates; the original XOR cancellation;
coefficient replacement without stale truth; a literal-by-literal oracle on
64/128/256/512/1024/2048-address domains; Q=1/31/32/63/64/65/129 with reuse 1/4
and all three declared populations; and a 65-gate, 260-query projection that
crosses both gate-word and query-word boundaries. Entire words and unused high
bits are checked, not just valid bit values. Out-of-range access returns None.
The literal oracle shares no transform, sparse evaluator or submask evaluator
with the implementation.

## Reproduction

```bash
cargo +1.89.0 test --locked -p sml-core --lib pvp6_executor
cargo +1.89.0 run --locked --release -p sml-core \
  --example pascal_boolean_pb8_gate_bank -- pvp6-model-executor
bash scripts/check-pvp6-fixture.sh
cargo +1.89.0 clippy --locked --workspace --all-targets -- -D warnings
cargo +1.89.0 test --locked --workspace
```

## Next gate and ecosystem boundary

The next model gate must declare an SML-owned recurrent integration bank export,
state-update interface and bounded sequence protocol. Compare full state/output
trajectories against direct evaluation, include coefficient changes between
steps, and distinguish unchanged-bank reuse from recurrent depth. Learning,
matched ablations, capacity scaling and admitted physical speed measurements
remain separate open gates in `SML-ARCHITECTURE-DECISION-GATES.md`.

The canonical query-major projection and complete-word/padding tests can be
reused to qualify SciRust, FLAT-ATTENTION and NNIS consumers. Their runtime is not
imported into SML; destination requalification remains required. RemoteOps stays
the physical control plane. No GPU is used by this check, no vendor SDK is added,
and the final model remains independent of the future separate SML-HARNESS.
