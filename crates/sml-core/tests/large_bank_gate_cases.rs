//! Table-driven merge of the 15 former `large_bank_gate_*.rs` test files.
//!
//! Each `case_*!` invocation below reproduces one former file verbatim (same
//! helpers, constants, assertions and test function names) inside a module named
//! after that file; only the per-file constants are passed as arguments.

macro_rules! case {
    ($name:ident, $p0:expr) => {
        mod $name {
            use sml_core::anf_large::LargeAnfGate;
            use sml_core::anf_large_bank::{LargeAnfBank, LargeBankError};
            const BITS: u32 = 20;
            const GATES: usize = $p0;
            fn masks(index: usize) -> Vec<u32> {
                let domain = 1u32 << BITS;
                vec![
                    0,
                    (index as u32 * 131 + 1) % domain,
                    (index as u32 * 8191 + 3) % domain,
                ]
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect()
            }
            fn oracle(mask: u32, q: u32) -> bool {
                let mut product = true;
                for bit in 0..BITS {
                    if mask & (1 << bit) != 0 {
                        product &= q & (1 << bit) != 0;
                    }
                }
                product
            }
            #[test]
            fn complete_packed_rows_match_independent_literal_oracle() {
                let bank = LargeAnfBank::new(
                    (0..GATES)
                        .map(|i| LargeAnfGate::new(BITS, masks(i)).unwrap())
                        .collect(),
                )
                .unwrap();
                let limit = 1u32 << BITS;
                let queries = [0, 1, 3, limit / 2, limit - 1];
                let words = GATES.div_ceil(64);
                let packed = bank.evaluate_packed(&queries, 5, 5 * words).unwrap();
                assert_eq!(packed.len(), queries.len() * words);
                for (row, &q) in queries.iter().enumerate() {
                    for gate in 0..GATES {
                        let expected = masks(gate)
                            .iter()
                            .fold(false, |parity, &mask| parity ^ oracle(mask, q));
                        assert_eq!(
                            (packed[row * words + gate / 64] >> (gate % 64)) & 1 != 0,
                            expected,
                            "row={row} gate={gate}"
                        );
                    }
                    for bit in GATES..words * 64 {
                        assert_eq!((packed[row * words + bit / 64] >> (bit % 64)) & 1, 0);
                    }
                }
            }
            #[test]
            fn query_and_output_budgets_fail_closed() {
                let bank = LargeAnfBank::new(
                    (0..GATES)
                        .map(|i| LargeAnfGate::new(BITS, masks(i)).unwrap())
                        .collect(),
                )
                .unwrap();
                assert_eq!(bank.gate_count(), GATES);
                assert_eq!(bank.address_bits(), BITS);
                assert_eq!(
                    bank.evaluate_packed(&[0], 0, 100),
                    Err(LargeBankError::BudgetExceeded)
                );
                assert_eq!(
                    bank.evaluate_packed(&[1u32 << BITS], 1, 100),
                    Err(LargeBankError::QueryOutOfRange(1u32 << BITS))
                );
                assert_eq!(
                    bank.evaluate_packed(&[0], 1, 0),
                    Err(LargeBankError::BudgetExceeded)
                );
            }
        }
    };
}

// Multi-gate packed ANF control for 1 gates and a 20-bit domain.
case!(large_bank_gate_1, 1);
// Multi-gate packed ANF control for 2 gates and a 20-bit domain.
case!(large_bank_gate_2, 2);
// Multi-gate packed ANF control for 7 gates and a 20-bit domain.
case!(large_bank_gate_7, 7);
// Multi-gate packed ANF control for 31 gates and a 20-bit domain.
case!(large_bank_gate_31, 31);
// Multi-gate packed ANF control for 32 gates and a 20-bit domain.
case!(large_bank_gate_32, 32);
// Multi-gate packed ANF control for 33 gates and a 20-bit domain.
case!(large_bank_gate_33, 33);
// Multi-gate packed ANF control for 63 gates and a 20-bit domain.
case!(large_bank_gate_63, 63);
// Multi-gate packed ANF control for 64 gates and a 20-bit domain.
case!(large_bank_gate_64, 64);
// Multi-gate packed ANF control for 65 gates and a 20-bit domain.
case!(large_bank_gate_65, 65);
// Multi-gate packed ANF control for 66 gates and a 20-bit domain.
case!(large_bank_gate_66, 66);
// Multi-gate packed ANF control for 127 gates and a 20-bit domain.
case!(large_bank_gate_127, 127);
// Multi-gate packed ANF control for 128 gates and a 20-bit domain.
case!(large_bank_gate_128, 128);
// Multi-gate packed ANF control for 129 gates and a 20-bit domain.
case!(large_bank_gate_129, 129);
// Multi-gate packed ANF control for 256 gates and a 20-bit domain.
case!(large_bank_gate_256, 256);
// Multi-gate packed ANF control for 257 gates and a 20-bit domain.
case!(large_bank_gate_257, 257);
