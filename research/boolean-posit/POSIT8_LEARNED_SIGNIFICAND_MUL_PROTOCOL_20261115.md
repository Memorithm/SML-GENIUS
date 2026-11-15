# EXP-MUL1 — posit8 multiplication using a learned significand subcircuit

Status: research-only functional experiment. Not a fully learned posit arithmetic unit, a normative Posit-compliance certificate, or a hardware speedup claim.

## Scientific provenance

The Posit Standard (2022), Posit Working Group (2022-03-02), sections 3.2–3.3 and 4.1, specifies the eight-bit posit sign, run-length regime, two-bit exponent, fraction, NaR and rounding behavior: https://posithub.org/docs/posit_standard-2.pdf . It also specifies many other operations, fused expressions, quire and conversions that this experiment does not implement. No external code or tables imported.

## Functional hypothesis

Can the five-gate full adder learned by #990 and its reusable 8x8-to-16 unsigned product composer (#991, corrected integration #999) perform an actual posit8 *significand product*, combined with explicitly engineered field decode and rounding, while reproducing all 65,536 possible posit8 multiplication outputs?

1. Parse all 256 raw encodings into sign, integer significand (1–15 or zero) and dyadic shift in 2^-24 units. NaR 0x80 is distinct from zero.
2. Pass the two actual parsed integer significands to #991's trained LearnedByteMultiplier.multiply, not to any multiplication arithmetic oracle. The multiplier calls sixteen learned-byte additions, with no full-result lookup table.
3. Combine shifts and sign XOR, then use an engineered 127-code monotone binary-search rounder in exact unsigned 128-bit units of 2^-48. NaR propagation, nonzero under/overflow saturation and midpoint/even-LSB selection are explicit.
4. Audit (never invoke at runtime in the multiplication path) two existing Memorithm Rust oracles across all operand pairs, and independently audit all 256 field decodes; cover zero, one, negative one, minPos, maxPos and NaR.
5. Report the query budget used for the one learned five-gate adder. There is no new learning for the regime, exponent or rounding.

## Independent proof boundary

The implementation is hybrid: *learned integer significand multiplication* plus engineered posit parsing, scale and rounding. It is not autonomous discovery of the complete arithmetic circuit. The two comparison oracles share the project's numeric interpretation of the 2022 specification; exhaustive agreement is not an external standard compliance certificate. No model inference defaults or host services are changed.

## Qualification

Pinned Rust 1.89 rustfmt, strict rustc tests and standalone program; all 65,536 operand pairs and all 256 decoded values must match existing oracle contracts. Full repository CI must pass on the exact PR head. Machine marker: POSIT8_LEARNED_SIGNIFICAND_MUL,pairs=65536,teacher_queries=...,errors=0,full_posit_gate_learning=false. No physical timing is performed. Dell/Thor requires separate RemoteOps admission.

## Follow-on

Replace the engineered regime, exponent and rounding predicates one subcircuit at a time with actively learned Boolean gates, preserving exhaustive audit and oracle independence. Measure actual native latency/code size and complete-model benefit only after arithmetic qualification.
