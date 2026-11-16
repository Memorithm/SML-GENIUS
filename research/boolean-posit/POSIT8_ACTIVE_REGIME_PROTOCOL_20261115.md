# REGIME-LEARN1 — Actively learned Boolean posit8 regime prefix

## Goal
Replace the previously hard-coded leading-run comparator loop of EXP-MUL1
with real **learned Boolean gate identities** while preserving the entire
posit8 multiplication result. This is functional Rust development, not only
a test harness.

## Algorithm and limits
- Grammar frozen in advance: AND, OR, XOR, NAND, NOR, XNOR.
- Two *independent* teacher tasks: 2-input bit-equality and 2-input
  prefix-continuation. The teacher only supplies the selected query's output.
- For each of the two tasks, begin with all six gates; choose an as-yet-unseen
  2-bit input giving the most balanced split between candidate predictions;
  query the teacher; discard inconsistent candidates; stop at one gate.
- Maximum four queries per task, eight total; verify the full 4-case truth
  table *after* selection. All six gates are candidate hypotheses from the
  outset; topology is deliberately **not** learned.
- Build the 7-bit positive-magnitude regime-run detector from one learned
  equality gate followed by one learned continuation gate per bit. The count,
  exponent extraction, power-of-two scaling and rounding remain engineered.
- Use this detector in the actual EXP-MUL1 posit8 multiplication implementation,
  with one immutable lazy process-wide learner, not retraining on every value.

## Independent exactness gates
1. All 127 valid positive magnitude patterns checked against the imperative
   reference prefix-counting semantics, including minPos/maxPos and the
   termination-bit boundary.
2. The complete 256-encoding field decode verified against the pre-existing
   posit8 oracle.
3. All 65,536 signed/NaR/zero posit8 multiplication pairs must match the two
   separately structured project oracles with zero mismatches.
4. Rust 1.89 strict formatting, warning-free unit tests, standalone
   model reports, full repository CI on exact SHA. No GPU, hardware host,
   quality or latency claims.

## Scientific boundary
This experiment learns exactly **two Boolean gate identities**, each from
active sample selection in an already fixed grammar/topology. It does NOT
learn regime topology, full numeric multiplication, exponent decoding,
rounding, the posit8 addition operation or a trained language model.
The pre-existing six-query full-adder learning budget remains separately
reported from the new regime-gate query budget. The two arithmetic oracles
share the project's numeric interpretation and are not an independent external
Posit Standard certification.

Source contract: Posit Working Group, Posit Standard (2022),
https://posithub.org/docs/posit_standard-2.pdf .
