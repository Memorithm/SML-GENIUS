//! Sparse ANF packed boundary: 17-bit domain, 2 gates.
use sml_core::anf_large::LargeAnfGate;
use sml_core::anf_large_bank::{LargeAnfBank, LargeBankError};
#[test]
fn exact_packed_boundary_and_rejection() {
    const BITS: u32 = 17;
    const GATES: usize = 2;
    let bank = LargeAnfBank::new(
        (0..GATES)
            .map(|i| LargeAnfGate::new(BITS, vec![0, 1_u32 << (i % BITS as usize)]).unwrap())
            .collect(),
    )
    .unwrap();
    let domain = 1_u32 << BITS;
    let queries = [0, 1, domain / 2, domain - 1];
    let words = GATES.div_ceil(64);
    let packed = bank.evaluate_packed(&queries, queries.len(), queries.len() * words).unwrap();
    for (row, &query) in queries.iter().enumerate() {
        for gate in 0..GATES {
            let mask = 1_u32 << (gate % BITS as usize);
            let expected = query & mask == 0;
            assert_eq!((packed[row * words + gate / 64] >> (gate % 64)) & 1 != 0, expected);
        }
    }
    assert_eq!(bank.evaluate_packed(&[domain], 1, words), Err(LargeBankError::QueryOutOfRange(domain)));
    assert_eq!(bank.evaluate_packed(&queries, 3, 4 * words), Err(LargeBankError::BudgetExceeded));
    assert_eq!(bank.evaluate_packed(&queries, 4, 3 * words), Err(LargeBankError::BudgetExceeded));
}
