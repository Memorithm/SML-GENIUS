//! Table-driven merge of the 7 former `large_bank_batch_boundary_w_*.rs` test files.
//!
//! Each `case_*!` invocation below reproduces one former file verbatim (same
//! helpers, constants, assertions and test function names) inside a module named
//! after that file; only the per-file constants are passed as arguments.

macro_rules! case {
    ($name:ident, $p0:expr) => {
        mod $name {
            use sml_core::anf_large::LargeAnfGate;
            use sml_core::anf_large_bank::{LargeAnfBank, LargeBankError};

            #[test]
            fn packed_batch_matches_independent_literal_oracle() {
                const WIDTH: usize = $p0;
                let bank = LargeAnfBank::new(
                    (0..WIDTH)
                        .map(|i| LargeAnfGate::new(20, vec![0, 1_u32 << i]).unwrap())
                        .collect(),
                )
                .unwrap();
                let queries = [0_u32, 1, 3, 65536, 1048575];
                let words = WIDTH.div_ceil(64);
                let packed = bank
                    .evaluate_packed(&queries, queries.len(), queries.len() * words)
                    .unwrap();
                assert_eq!(packed.len(), queries.len() * words);
                for (row, &query) in queries.iter().enumerate() {
                    for gate in 0..WIDTH {
                        let constant = true;
                        let literal = query & (1_u32 << gate) != 0;
                        let expected = constant ^ literal;
                        assert_eq!(
                            (packed[row * words + gate / 64] >> (gate % 64)) & 1 != 0,
                            expected
                        );
                    }
                }
                assert_eq!(
                    bank.evaluate_packed(&queries, 4, 5 * words),
                    Err(LargeBankError::BudgetExceeded)
                );
                assert_eq!(
                    bank.evaluate_packed(&queries, 5, 4 * words),
                    Err(LargeBankError::BudgetExceeded)
                );
                assert_eq!(
                    bank.evaluate_packed(&[1048576], 1, words),
                    Err(LargeBankError::QueryOutOfRange(1048576))
                );
            }
        }
    };
}

// Verify batch packing and query budget for a 1-gate sparse ANF bank.
case!(large_bank_batch_boundary_w1, 1);
// Verify batch packing and query budget for a 2-gate sparse ANF bank.
case!(large_bank_batch_boundary_w2, 2);
// Verify batch packing and query budget for a 4-gate sparse ANF bank.
case!(large_bank_batch_boundary_w4, 4);
// Verify batch packing and query budget for a 8-gate sparse ANF bank.
case!(large_bank_batch_boundary_w8, 8);
// Verify batch packing and query budget for a 12-gate sparse ANF bank.
case!(large_bank_batch_boundary_w12, 12);
// Verify batch packing and query budget for a 16-gate sparse ANF bank.
case!(large_bank_batch_boundary_w16, 16);
// Verify batch packing and query budget for a 20-gate sparse ANF bank.
case!(large_bank_batch_boundary_w20, 20);
