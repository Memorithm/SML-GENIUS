//! Boolean batch validation must reject malformed inputs before evaluation.
use sml_core::anf_large::LargeAnfGate;
use sml_core::anf_large_bank::{LargeAnfBank, LargeBankError};

#[test]
fn boolean_batch_matches_packed_bits_and_rejects_invalid_queries() {
    let bank = LargeAnfBank::new(
        (0..65)
            .map(|i| LargeAnfGate::new(20, vec![0, 1_u32 << (i % 20)]).unwrap())
            .collect(),
    )
    .unwrap();
    let queries = [0_u32, 1, 3, 65536, 1048575];
    let bool_rows = bank.evaluate_bool(&queries, 5).unwrap();
    let packed = bank.evaluate_packed(&queries, 5, 10).unwrap();
    assert_eq!(bool_rows.len(), queries.len());
    for (row, values) in bool_rows.iter().enumerate() {
        assert_eq!(values.len(), 65);
        for (gate, &value) in values.iter().enumerate() {
            assert_eq!(value, (packed[row * 2 + gate / 64] >> (gate % 64)) & 1 != 0);
        }
    }
    assert_eq!(
        bank.evaluate_bool(&[0, 1048576], 2),
        Err(LargeBankError::QueryOutOfRange(1048576))
    );
    assert_eq!(
        bank.evaluate_bool(&queries, 4),
        Err(LargeBankError::BudgetExceeded)
    );
}
