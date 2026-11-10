# CYCLOID-6 — order-robust frozen page-access selection

**Status:** opt-in, deterministic native-Rust guard with a real two-source read
adapter. Not an inference-time speedup or measured physical superiority.

## Why a new gate is necessary

CYCLOID-3 chooses an access route from medians of caller-supplied (and
potentially pooled) timing observations. CYCLOID-4 records source-access order,
but without a separate guard the end-to-end decision can be inadvertently
driven by filesystem page-cache warmth, CPU scheduling, or the order in which
the two sources are read. A pooled median improvement does **not** imply
read-order robustness.

The new \`sml_runtime::cycloid_order_guard\` module keeps all baseline
CYCLOID-3 checks and strengthens experimental route promotion.

### Frozen inputs and qualification

\`freeze_order_guarded\` requires one CYCLOID-4 panel of 3–64 true
AB/BA counterbalanced pairs. The entries must preserve:
- exactly two individually identified reads per pair;
- the order FileThenRam, RamThenFile for even pairs and the reverse for
  odd pairs;
- a positive, finite (u128 nanoseconds) observed time for each source;
- byte-exact verification flags for every observation;
- exact page identity/precision/version/length and fresh file-versus-RAM
  integrity validation via CYCLOID-3.

It builds a normal frozen costed access plan through the existing bounded
CYCLOID-1 planner. **If this chooses indirect RAM access**, that choice is
permitted only when BOTH observed order strata satisfy:

\[
\mathrm{p95}(T_{\mathrm{RAM\ read+CRC}}) +
T_{\mathrm{relay\ lookup}} + T_{\mathrm{dispatch}}
<
\mathrm{median}(T_{\mathrm{file\ read+parse+CRC}})
\]

If either stratum fails the strict inequality or arithmetic overflows, the
candidate fails closed with \`IndirectNotOrderRobust\`. Direct baseline is
retained when CYCLOID-3 would select direct. This does not turn synthetic,
untrusted or single-host observations into a statistical confidence interval.

\`collect_and_freeze_order_guarded\` actually executes the CYCLOID-4 bounded
file/RAM reads, derives a panel in-process, freezes it once, and returns the
original panel alongside the immutable decision and per-order quantiles.
No graph search occurs when the resulting \`FrozenCycloidAccess::read\` is
called. The overhead inputs remain **caller-declared**; they must be
independently measured before deployment claims.

### Regression cases

Five unit tests use actual committed native SML RAM pages and temporary
SMLWGT01 files, with synthetic timing inputs to make branch decisions
deterministic:

1. Indirect path dominates both orders and executes the original byte-exact
   page through RAM.
2. Pooled median says RAM, but one order reverses the advantage — rejected.
3. Direct-file baseline remains available when RAM is not robust.
4. Wrong AB/BA order and invalid/zero timing fail closed.
5. Same timings attached to a mismatched page representation version fail.

All exact-head Rust workspace formatting, Clippy and unit/integration tests
remain mandatory. No default MOR change and no new GPU/vendor dependency.

### Boundaries

The guard is stricter than CYCLOID-3 but remains a **host-call diagnostic**.
Its profile values may be fabricated by a caller; only actual byte identity
is reverified on freeze. Deterministic AB/BA observations from a single host
are not randomized, do not isolate a cold filesystem cache or account for
the total amortized cost of calibration, and do not prove end-to-end model
throughput, task quality, DRAM traffic, energy or latency distributions
outside this experiment.

For physical qualification run the reviewed SML synthetic probe through
Memorithm/RemoteOps on Dell with pinned SHA, and separately qualify measured
lookup/dispatch overhead and model-task parity. No inference default may be
enabled without that evidence.
