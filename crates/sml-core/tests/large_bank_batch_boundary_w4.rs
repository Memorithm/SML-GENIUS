//! Verify batch packing and query budget for a 4-gate sparse ANF bank.
use sml_core::anf_large::LargeAnfGate;
use sml_core::anf_large_bank::{LargeAnfBank, LargeBankError};

#[test]
fn packed_batch_matches_independent_literal_oracle() {
    const WIDTH: usize = 4;
    let bank = LargeAnfBank::new(
        (0..WIDTH)
            .map(|i| LargeAnfGate::new(20, vec![0, 1_u32 << i]).unwrap())
            .collect(),
    )
    .unwrap();
    let queries = [0_u32, 1, 3, 65536, 1048575];
    let words = WIDTH.div_ceil(64);
    let packed = bank.evaluate_packed(&queries, queries.len(), queries.len() * words).unwrap();
    assert_eq!(packed.len(), queries.len() * words);
    for (row, &query) in queries.iter().enumerate() {
        for gate in 0..WIDTH {
            let constant = true;
            let literal = query & (1_u32 << gate) != 0;
            let expected = constant ^ literal;
            assert_eq!((packed[row * words + gate / 64] >> (gate % 64)) & 1 != 0, expected);
        }
    }
    assert_eq!(bank.evaluate_packed(&queries, 4, 5 * words), Err(LargeBankError::BudgetExceeded));
    assert_eq!(bank.evaluate_packed(&queries, 5, 4 * words), Err(LargeBankError::BudgetExceeded));
    assert_eq!(bank.evaluate_packed(&[1048576], 1, words), Err(LargeBankError::QueryOutOfRange(1048576)));
}
