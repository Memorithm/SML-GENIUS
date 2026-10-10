//! Table-driven merge of the 42 former `large_bank_b50_*.rs` test files.
//!
//! Each `case_*!` invocation below reproduces one former file verbatim (same
//! helpers, constants, assertions and test function names) inside a module named
//! after that file; only the per-file constants are passed as arguments.

macro_rules! case {
    ($name:ident, $p0:expr, $p1:expr) => {
        mod $name {
            use sml_core::anf_large::LargeAnfGate;
            use sml_core::anf_large_bank::{LargeAnfBank, LargeBankError};
            #[test]
            fn exact_packed_boundary_and_rejection() {
                const BITS: u32 = $p0;
                const GATES: usize = $p1;
                let bank = LargeAnfBank::new(
                    (0..GATES)
                        .map(|i| {
                            LargeAnfGate::new(BITS, vec![0, 1_u32 << (i % BITS as usize)]).unwrap()
                        })
                        .collect(),
                )
                .unwrap();
                let domain = 1_u32 << BITS;
                let queries = [0, 1, domain / 2, domain - 1];
                let words = GATES.div_ceil(64);
                let packed = bank
                    .evaluate_packed(&queries, queries.len(), queries.len() * words)
                    .unwrap();
                for (row, &query) in queries.iter().enumerate() {
                    for gate in 0..GATES {
                        let mask = 1_u32 << (gate % BITS as usize);
                        let expected = query & mask == 0;
                        assert_eq!(
                            (packed[row * words + gate / 64] >> (gate % 64)) & 1 != 0,
                            expected
                        );
                    }
                }
                assert_eq!(
                    bank.evaluate_packed(&[domain], 1, words),
                    Err(LargeBankError::QueryOutOfRange(domain))
                );
                assert_eq!(
                    bank.evaluate_packed(&queries, 3, 4 * words),
                    Err(LargeBankError::BudgetExceeded)
                );
                assert_eq!(
                    bank.evaluate_packed(&queries, 4, 3 * words),
                    Err(LargeBankError::BudgetExceeded)
                );
            }
        }
    };
}

// Sparse ANF packed boundary: 13-bit domain, 1 gates.
case!(large_bank_b50_416, 13, 1);
// Sparse ANF packed boundary: 13-bit domain, 2 gates.
case!(large_bank_b50_417, 13, 2);
// Sparse ANF packed boundary: 13-bit domain, 4 gates.
case!(large_bank_b50_418, 13, 4);
// Sparse ANF packed boundary: 13-bit domain, 8 gates.
case!(large_bank_b50_419, 13, 8);
// Sparse ANF packed boundary: 13-bit domain, 16 gates.
case!(large_bank_b50_420, 13, 16);
// Sparse ANF packed boundary: 13-bit domain, 32 gates.
case!(large_bank_b50_421, 13, 32);
// Sparse ANF packed boundary: 16-bit domain, 1 gates.
case!(large_bank_b50_422, 16, 1);
// Sparse ANF packed boundary: 16-bit domain, 2 gates.
case!(large_bank_b50_423, 16, 2);
// Sparse ANF packed boundary: 16-bit domain, 4 gates.
case!(large_bank_b50_424, 16, 4);
// Sparse ANF packed boundary: 16-bit domain, 8 gates.
case!(large_bank_b50_425, 16, 8);
// Sparse ANF packed boundary: 16-bit domain, 16 gates.
case!(large_bank_b50_426, 16, 16);
// Sparse ANF packed boundary: 16-bit domain, 32 gates.
case!(large_bank_b50_427, 16, 32);
// Sparse ANF packed boundary: 17-bit domain, 1 gates.
case!(large_bank_b50_428, 17, 1);
// Sparse ANF packed boundary: 17-bit domain, 2 gates.
case!(large_bank_b50_429, 17, 2);
// Sparse ANF packed boundary: 17-bit domain, 4 gates.
case!(large_bank_b50_430, 17, 4);
// Sparse ANF packed boundary: 17-bit domain, 8 gates.
case!(large_bank_b50_431, 17, 8);
// Sparse ANF packed boundary: 17-bit domain, 16 gates.
case!(large_bank_b50_432, 17, 16);
// Sparse ANF packed boundary: 17-bit domain, 32 gates.
case!(large_bank_b50_433, 17, 32);
// Sparse ANF packed boundary: 18-bit domain, 1 gates.
case!(large_bank_b50_434, 18, 1);
// Sparse ANF packed boundary: 18-bit domain, 2 gates.
case!(large_bank_b50_435, 18, 2);
// Sparse ANF packed boundary: 18-bit domain, 4 gates.
case!(large_bank_b50_436, 18, 4);
// Sparse ANF packed boundary: 18-bit domain, 8 gates.
case!(large_bank_b50_437, 18, 8);
// Sparse ANF packed boundary: 18-bit domain, 16 gates.
case!(large_bank_b50_438, 18, 16);
// Sparse ANF packed boundary: 18-bit domain, 32 gates.
case!(large_bank_b50_439, 18, 32);
// Sparse ANF packed boundary: 19-bit domain, 1 gates.
case!(large_bank_b50_440, 19, 1);
// Sparse ANF packed boundary: 19-bit domain, 2 gates.
case!(large_bank_b50_441, 19, 2);
// Sparse ANF packed boundary: 19-bit domain, 4 gates.
case!(large_bank_b50_442, 19, 4);
// Sparse ANF packed boundary: 19-bit domain, 8 gates.
case!(large_bank_b50_443, 19, 8);
// Sparse ANF packed boundary: 19-bit domain, 16 gates.
case!(large_bank_b50_444, 19, 16);
// Sparse ANF packed boundary: 19-bit domain, 32 gates.
case!(large_bank_b50_445, 19, 32);
// Sparse ANF packed boundary: 20-bit domain, 1 gates.
case!(large_bank_b50_446, 20, 1);
// Sparse ANF packed boundary: 20-bit domain, 2 gates.
case!(large_bank_b50_447, 20, 2);
// Sparse ANF packed boundary: 20-bit domain, 4 gates.
case!(large_bank_b50_448, 20, 4);
// Sparse ANF packed boundary: 20-bit domain, 8 gates.
case!(large_bank_b50_449, 20, 8);
// Sparse ANF packed boundary: 20-bit domain, 16 gates.
case!(large_bank_b50_450, 20, 16);
// Sparse ANF packed boundary: 20-bit domain, 32 gates.
case!(large_bank_b50_451, 20, 32);
// Sparse ANF packed boundary: 21-bit domain, 1 gates.
case!(large_bank_b50_452, 21, 1);
// Sparse ANF packed boundary: 21-bit domain, 2 gates.
case!(large_bank_b50_453, 21, 2);
// Sparse ANF packed boundary: 21-bit domain, 4 gates.
case!(large_bank_b50_454, 21, 4);
// Sparse ANF packed boundary: 21-bit domain, 8 gates.
case!(large_bank_b50_455, 21, 8);
// Sparse ANF packed boundary: 21-bit domain, 16 gates.
case!(large_bank_b50_456, 21, 16);
// Sparse ANF packed boundary: 21-bit domain, 32 gates.
case!(large_bank_b50_457, 21, 32);
